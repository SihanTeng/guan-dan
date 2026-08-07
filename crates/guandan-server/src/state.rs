//! Global server state: sessions, rooms, matchmaking.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use guandan_core::{team_of, Action, MatchPhase};
use guandan_protocol::{encode_server, ClientMessage, RoomSummary, SeatInfo, ServerMessage};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::room::Room;
use crate::settings::GameSettings;

pub type OutTx = mpsc::Sender<String>;

/// Hard cap on live rooms; guards against room-creation spam.
const MAX_ROOMS: usize = 1024;

struct Session {
    #[allow(dead_code)]
    player_id: Uuid,
    tx: OutTx,
    room_id: Option<String>,
    name: String,
}

pub struct AppState {
    sessions: Mutex<HashMap<Uuid, Session>>,
    rooms: Mutex<HashMap<String, Room>>,
    room_seq: AtomicU64,
    quick_queue: Mutex<Vec<Uuid>>,
    pub settings: GameSettings,
}

impl AppState {
    pub fn new(settings: GameSettings) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            rooms: Mutex::new(HashMap::new()),
            room_seq: AtomicU64::new(1),
            quick_queue: Mutex::new(Vec::new()),
            settings,
        }
    }

    pub async fn register(&self, session_id: Uuid, player_id: Uuid, tx: OutTx) {
        let mut sessions = self.sessions.lock().await;
        sessions.insert(
            session_id,
            Session {
                player_id,
                tx,
                room_id: None,
                name: format!("玩家{}", &player_id.to_string()[..8]),
            },
        );
    }

    pub async fn unregister(&self, session_id: Uuid) {
        let room_id = {
            let mut sessions = self.sessions.lock().await;
            let room = sessions.get(&session_id).and_then(|s| s.room_id.clone());
            sessions.remove(&session_id);
            room
        };
        if let Some(rid) = room_id {
            self.seat_departed(&rid, session_id).await;
        }
        let mut q = self.quick_queue.lock().await;
        q.retain(|id| *id != session_id);
    }

    /// Vacate `session_id`'s seat in `rid` (left or disconnected).
    ///
    /// - **Playing / Tribute / HandOver / MatchOver**: bot substitutes
    ///   immediately so the hand/confirm never soft-locks. Bots auto-confirm
    ///   the result board. A human may reclaim the seat between hands
    ///   (`TakeSeat`) for the next round.
    /// - **Idle / lobby**: seat opens for re-party (bot fill after timeout).
    /// - Room is dropped once no human sessions remain.
    async fn seat_departed(&self, rid: &str, session_id: Uuid) {
        let mut rooms = self.rooms.lock().await;
        let Some(room) = rooms.get_mut(rid) else {
            return;
        };
        let mut followup: Vec<(Option<Uuid>, ServerMessage)> = Vec::new();
        if let Some(seat) = room.seat_of(session_id) {
            let phase = room.game.as_ref().map(|g| g.phase);
            let needs_bot = matches!(
                phase,
                Some(
                    MatchPhase::Playing
                        | MatchPhase::Tribute
                        | MatchPhase::HandOver
                        | MatchPhase::MatchOver
                )
            );

            // Drop the human seat first.
            room.slots[seat].session_id = None;
            room.slots[seat].ready = false;

            let left = encode_server(&ServerMessage::PlayerLeft { seat }).unwrap();
            self.broadcast_room_locked(room, None, left).await;

            if needs_bot {
                // Immediate bot substitute — plays / confirms for the rest of
                // this hand; stays until a human TakeSeat between hands.
                room.slots[seat].is_bot = true;
                room.slots[seat].name = format!("机器人{}", seat + 1);
                room.slots[seat].ready = true;
                // No open re-party window while the seat is bot-filled.
                room.reparty_deadline = None;
                let taken = ServerMessage::SeatTaken {
                    seat,
                    info: SeatInfo {
                        seat,
                        name: room.slots[seat].name.clone(),
                        is_bot: true,
                        ready: true,
                        team: team_of(seat),
                    },
                };
                if let Ok(t) = encode_server(&taken) {
                    self.broadcast_room_locked(room, None, t).await;
                }
                // If we're on the result board, confirm for every bot now.
                if matches!(phase, Some(MatchPhase::HandOver | MatchPhase::MatchOver)) {
                    followup.extend(room.auto_confirm_bots());
                }
            } else {
                // Between hands / lobby: leave vacant for a human substitute.
                room.slots[seat].is_bot = false;
                room.slots[seat].name = format!("空位{}", seat + 1);
                room.reparty_deadline = Some(
                    Instant::now()
                        + Duration::from_secs(guandan_protocol::REPARTY_TIMEOUT_SECS as u64),
                );
                let opened = ServerMessage::SeatOpened {
                    seat,
                    reason: "玩家离开".into(),
                };
                if let Ok(t) = encode_server(&opened) {
                    self.broadcast_room_locked(room, None, t).await;
                }
            }
        }
        // A room lives only while at least one human session is seated.
        let empty = rooms
            .get(rid)
            .map(|r| r.slots.iter().all(|s| s.session_id.is_none()))
            .unwrap_or(false);
        if empty {
            rooms.remove(rid);
            return;
        }
        drop(rooms);
        if !followup.is_empty() {
            self.dispatch_stored(rid, followup).await;
        }
    }

    /// Vacate any seat the session currently holds (before seating it elsewhere).
    async fn vacate_current_room(&self, session_id: Uuid) {
        let room_id = {
            let mut sessions = self.sessions.lock().await;
            match sessions.get_mut(&session_id) {
                Some(s) => s.room_id.take(),
                None => None,
            }
        };
        if let Some(rid) = room_id {
            self.seat_departed(&rid, session_id).await;
        }
    }

    pub async fn online_count(&self) -> u32 {
        self.sessions.lock().await.len() as u32
    }

    pub async fn send_to(&self, session_id: Uuid, text: String) -> Result<()> {
        let sessions = self.sessions.lock().await;
        if let Some(s) = sessions.get(&session_id) {
            // Drop on full queue (slow reader) instead of blocking the server.
            let _ = s.tx.try_send(text);
        }
        Ok(())
    }

    async fn broadcast_room_locked(&self, room: &Room, except: Option<Uuid>, text: String) {
        for slot in &room.slots {
            if let Some(sid) = slot.session_id {
                if except == Some(sid) {
                    continue;
                }
                // need sessions — re-lock carefully
                let sessions = self.sessions.lock().await;
                if let Some(s) = sessions.get(&sid) {
                    let _ = s.tx.try_send(text.clone());
                }
            }
        }
    }

    pub async fn handle(&self, session_id: Uuid, msg: ClientMessage) -> Result<()> {
        match msg {
            ClientMessage::Ping => {
                let t = encode_server(&ServerMessage::Pong)?;
                self.send_to(session_id, t).await?;
            }
            ClientMessage::Reconnect { session_id: _ } => {
                // Minimal: already connected as new session
                let t = encode_server(&ServerMessage::Error {
                    message: "重连请重新加入房间".into(),
                })?;
                self.send_to(session_id, t).await?;
            }
            ClientMessage::CreateRoom { name } => {
                if !name.is_empty() {
                    let mut sessions = self.sessions.lock().await;
                    if let Some(s) = sessions.get_mut(&session_id) {
                        s.name = name;
                    }
                }
                // One room per session: leave any previous room first.
                self.vacate_current_room(session_id).await;
                if self.rooms.lock().await.len() >= MAX_ROOMS {
                    bail!("服务器繁忙，请稍后再试");
                }
                let rid = format!("R{}", self.room_seq.fetch_add(1, Ordering::SeqCst));
                let mut room = Room::new(rid.clone(), false, self.settings);
                let seat = room
                    .join(session_id, self.name_of(session_id).await)
                    .unwrap();
                {
                    let mut sessions = self.sessions.lock().await;
                    if let Some(s) = sessions.get_mut(&session_id) {
                        s.room_id = Some(rid.clone());
                    }
                }
                let infos = room.seat_infos();
                self.rooms.lock().await.insert(rid.clone(), room);
                let t = encode_server(&ServerMessage::RoomCreated {
                    room_id: rid.clone(),
                })?;
                self.send_to(session_id, t).await?;
                let t = encode_server(&ServerMessage::RoomJoined {
                    room_id: rid,
                    seat,
                    players: infos,
                })?;
                self.send_to(session_id, t).await?;
            }
            ClientMessage::JoinRoom { room_id } => {
                {
                    let sessions = self.sessions.lock().await;
                    let already = sessions.get(&session_id).and_then(|s| s.room_id.as_deref())
                        == Some(room_id.as_str());
                    if already {
                        bail!("已在该房间");
                    }
                }
                let name = self.name_of(session_id).await;
                let mut rooms = self.rooms.lock().await;
                let room = rooms
                    .get_mut(&room_id)
                    .ok_or_else(|| anyhow::anyhow!("房间不存在"))?;
                if room.game.is_some() {
                    bail!("游戏已开始");
                }
                let seat = room
                    .join(session_id, name)
                    .ok_or_else(|| anyhow::anyhow!("房间已满"))?;
                let infos = room.seat_infos();
                let joined = encode_server(&ServerMessage::RoomJoined {
                    room_id: room_id.clone(),
                    seat,
                    players: infos.clone(),
                })?;
                let broadcast = encode_server(&ServerMessage::PlayerJoined {
                    seat,
                    info: infos[seat].clone(),
                })?;
                self.broadcast_room_locked(room, None, broadcast).await;
                drop(rooms);
                // Seat secured — leave any previous room, then record the new one.
                self.vacate_current_room(session_id).await;
                {
                    let mut sessions = self.sessions.lock().await;
                    if let Some(s) = sessions.get_mut(&session_id) {
                        s.room_id = Some(room_id.clone());
                    }
                }
                // RoomJoined to joiner (broadcast already sent PlayerJoined to all including joiner)
                let _ = self.send_to(session_id, joined).await;
            }
            ClientMessage::LeaveRoom => {
                self.leave_room(session_id).await?;
            }
            ClientMessage::PracticeMatch => {
                // One room per session: leave any previous room first.
                self.vacate_current_room(session_id).await;
                if self.rooms.lock().await.len() >= MAX_ROOMS {
                    bail!("服务器繁忙，请稍后再试");
                }
                let rid = format!("P{}", self.room_seq.fetch_add(1, Ordering::SeqCst));
                let mut room = Room::new(rid.clone(), true, self.settings);
                let name = self.name_of(session_id).await;
                let seat = room.join(session_id, name).unwrap();
                room.fill_bots();
                for s in room.slots.iter_mut() {
                    s.ready = true;
                }
                {
                    let mut sessions = self.sessions.lock().await;
                    if let Some(s) = sessions.get_mut(&session_id) {
                        s.room_id = Some(rid.clone());
                    }
                }
                let infos = room.seat_infos();
                let msgs = room.start_game();
                // Bots wait for reveal/turn tick so plays are visible.
                self.rooms.lock().await.insert(rid.clone(), room);
                let t = encode_server(&ServerMessage::MatchFound {
                    room_id: rid.clone(),
                    seat,
                })?;
                self.send_to(session_id, t).await?;
                let t = encode_server(&ServerMessage::RoomJoined {
                    room_id: rid.clone(),
                    seat,
                    players: infos,
                })?;
                self.send_to(session_id, t).await?;
                self.dispatch_stored(&rid, msgs).await;
            }
            ClientMessage::QuickMatch => {
                let mut q = self.quick_queue.lock().await;
                if !q.contains(&session_id) {
                    q.push(session_id);
                }
                if q.len() >= 4 {
                    let players: Vec<Uuid> = q.drain(..4).collect();
                    drop(q);
                    self.make_quick_room(players).await?;
                } else {
                    let t = encode_server(&ServerMessage::Error {
                        message: format!("快速匹配排队中 ({}/4)…", q.len()),
                    })?;
                    // use info not error ideally
                    self.send_to(session_id, t).await?;
                }
            }
            ClientMessage::Ready => {
                self.set_ready(session_id, true).await?;
            }
            ClientMessage::CancelReady => {
                self.set_ready(session_id, false).await?;
            }
            ClientMessage::ListRooms => {
                let rooms = self.rooms.lock().await;
                let list: Vec<RoomSummary> = rooms
                    .values()
                    .filter(|r| r.game.is_none())
                    .map(|r| RoomSummary {
                        room_id: r.id.clone(),
                        players: r.player_count(),
                        max_players: 4,
                        in_game: r.game.is_some(),
                    })
                    .collect();
                let t = encode_server(&ServerMessage::RoomList { rooms: list })?;
                self.send_to(session_id, t).await?;
            }
            ClientMessage::PlayCards { card_ids } => {
                self.game_action(session_id, Action::Play { card_ids })
                    .await?;
            }
            ClientMessage::Pass => {
                self.game_action(session_id, Action::Pass).await?;
            }
            ClientMessage::ReturnTribute { card_id, to_seat } => {
                self.game_action(session_id, Action::ReturnTribute { card_id, to_seat })
                    .await?;
            }
            ClientMessage::ConfirmResult => {
                self.game_action(session_id, Action::ConfirmResult).await?;
            }
            ClientMessage::TakeSeat { seat } => {
                self.take_seat(session_id, seat).await?;
            }
            ClientMessage::Chat { content } => {
                // Bound chat length — it is re-broadcast to every seat.
                let content: String = content.chars().take(200).collect();
                let (room_id, name) = {
                    let sessions = self.sessions.lock().await;
                    let s = sessions
                        .get(&session_id)
                        .ok_or_else(|| anyhow::anyhow!("无会话"))?;
                    let rid = s
                        .room_id
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("不在房间"))?;
                    (rid, s.name.clone())
                };
                let rooms = self.rooms.lock().await;
                if let Some(room) = rooms.get(&room_id) {
                    let seat = room.seat_of(session_id);
                    let t = encode_server(&ServerMessage::Chat {
                        seat,
                        name,
                        content,
                    })?;
                    self.broadcast_room_locked(room, None, t).await;
                }
            }
        }
        Ok(())
    }

    async fn name_of(&self, session_id: Uuid) -> String {
        self.sessions
            .lock()
            .await
            .get(&session_id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "玩家".into())
    }

    async fn dispatch_stored(&self, rid: &str, msgs: Vec<(Option<Uuid>, ServerMessage)>) {
        let rooms = self.rooms.lock().await;
        if let Some(room) = rooms.get(rid) {
            // can't call dispatch with lock of rooms held and sessions — clone seat sessions
            let targets: Vec<_> = room.slots.iter().filter_map(|s| s.session_id).collect();
            drop(rooms);
            for (target, msg) in msgs {
                let text = match encode_server(&msg) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                match target {
                    Some(sid) => {
                        let _ = self.send_to(sid, text).await;
                    }
                    None => {
                        for sid in &targets {
                            let _ = self.send_to(*sid, text.clone()).await;
                        }
                    }
                }
            }
        }
    }

    async fn leave_room(&self, session_id: Uuid) -> Result<()> {
        let room_id = {
            let mut sessions = self.sessions.lock().await;
            let s = sessions
                .get_mut(&session_id)
                .ok_or_else(|| anyhow::anyhow!("无会话"))?;
            s.room_id.take()
        };
        if let Some(rid) = room_id {
            self.seat_departed(&rid, session_id).await;
        }
        Ok(())
    }

    async fn set_ready(&self, session_id: Uuid, ready: bool) -> Result<()> {
        let room_id = self
            .sessions
            .lock()
            .await
            .get(&session_id)
            .and_then(|s| s.room_id.clone())
            .ok_or_else(|| anyhow::anyhow!("不在房间"))?;
        let mut rooms = self.rooms.lock().await;
        let room = rooms
            .get_mut(&room_id)
            .ok_or_else(|| anyhow::anyhow!("房间不存在"))?;
        let seat = room
            .seat_of(session_id)
            .ok_or_else(|| anyhow::anyhow!("无座位"))?;
        room.slots[seat].ready = ready;
        let t = encode_server(&ServerMessage::PlayerReady { seat, ready })?;
        self.broadcast_room_locked(room, None, t).await;

        if room.all_ready() && room.game.is_none() {
            let msgs = room.start_game();
            drop(rooms);
            self.dispatch_stored(&room_id, msgs).await;
        }
        Ok(())
    }

    async fn game_action(&self, session_id: Uuid, action: Action) -> Result<()> {
        let room_id = self
            .sessions
            .lock()
            .await
            .get(&session_id)
            .and_then(|s| s.room_id.clone())
            .ok_or_else(|| anyhow::anyhow!("不在房间"))?;
        let mut rooms = self.rooms.lock().await;
        let room = rooms
            .get_mut(&room_id)
            .ok_or_else(|| anyhow::anyhow!("房间不存在"))?;
        let seat = room
            .seat_of(session_id)
            .ok_or_else(|| anyhow::anyhow!("无座位"))?;
        // Human action only; bots follow after play_reveal via tick_game.
        let msgs = room
            .apply_action(seat, action)
            .map_err(|e| anyhow::anyhow!(e))?;
        drop(rooms);
        self.dispatch_stored(&room_id, msgs).await;
        Ok(())
    }

    async fn make_quick_room(&self, players: Vec<Uuid>) -> Result<()> {
        // Drop players who disconnected or got seated elsewhere while queued —
        // a stale seat would never act and never be bot-filled.
        let mut valid = Vec::new();
        for pid in players {
            let in_lobby = self
                .sessions
                .lock()
                .await
                .get(&pid)
                .map(|s| s.room_id.is_none())
                .unwrap_or(false);
            if in_lobby {
                valid.push(pid);
            }
        }
        if valid.is_empty() {
            return Ok(());
        }
        if self.rooms.lock().await.len() >= MAX_ROOMS {
            bail!("服务器繁忙，请稍后再试");
        }
        let rid = format!("Q{}", self.room_seq.fetch_add(1, Ordering::SeqCst));
        let mut room = Room::new(rid.clone(), false, self.settings);
        for pid in &valid {
            let name = self.name_of(*pid).await;
            room.join(*pid, name);
            let mut sessions = self.sessions.lock().await;
            if let Some(s) = sessions.get_mut(pid) {
                s.room_id = Some(rid.clone());
            }
        }
        for s in room.slots.iter_mut() {
            if s.session_id.is_some() {
                s.ready = true;
            }
        }
        // fill bots if needed
        room.fill_bots();
        let infos = room.seat_infos();
        let msgs = room.start_game();
        self.rooms.lock().await.insert(rid.clone(), room);
        for (i, pid) in valid.iter().enumerate() {
            let t = encode_server(&ServerMessage::MatchFound {
                room_id: rid.clone(),
                seat: i,
            })?;
            self.send_to(*pid, t).await?;
            let t = encode_server(&ServerMessage::RoomJoined {
                room_id: rid.clone(),
                seat: i,
                players: infos.clone(),
            })?;
            self.send_to(*pid, t).await?;
        }
        self.dispatch_stored(&rid, msgs).await;
        Ok(())
    }

    async fn take_seat(&self, session_id: Uuid, seat: usize) -> Result<()> {
        let name = self.name_of(session_id).await;
        // Session must already be in the room (JoinRoom first).
        let room_id = self
            .sessions
            .lock()
            .await
            .get(&session_id)
            .and_then(|s| s.room_id.clone());
        let Some(room_id) = room_id else {
            bail!("请先加入房间再占座");
        };
        let mut rooms = self.rooms.lock().await;
        let room = rooms
            .get_mut(&room_id)
            .ok_or_else(|| anyhow::anyhow!("房间不存在"))?;
        // If already seated, vacate old
        if let Some(old_seat) = room.seat_of(session_id) {
            if old_seat == seat {
                return Ok(());
            }
            room.slots[old_seat].session_id = None;
        }
        let msgs = room
            .take_seat(session_id, name, seat)
            .map_err(|e| anyhow::anyhow!(e))?;
        {
            let mut sessions = self.sessions.lock().await;
            if let Some(s) = sessions.get_mut(&session_id) {
                s.room_id = Some(room_id.clone());
            }
        }
        drop(rooms);
        self.dispatch_stored(&room_id, msgs).await;
        Ok(())
    }

    /// Periodic: turn timeouts, bot steps, result confirms, re-party.
    pub async fn tick_game(&self) {
        // Safety-net GC: rooms only live while a human session is seated.
        self.rooms
            .lock()
            .await
            .retain(|_, r| r.slots.iter().any(|s| s.session_id.is_some()));
        let room_ids: Vec<String> = self.rooms.lock().await.keys().cloned().collect();
        for rid in room_ids {
            let mut rooms = self.rooms.lock().await;
            let Some(room) = rooms.get_mut(&rid) else {
                continue;
            };
            if room.game.is_none() {
                continue;
            }
            let timeout_msgs = room.check_turn_timeout();
            let bot_msgs = room.bot_actions();
            let confirm_msgs = room.check_confirm_timeout();
            let reparty_msgs = room.check_reparty();
            drop(rooms);
            for batch in [timeout_msgs, bot_msgs, confirm_msgs, reparty_msgs] {
                if !batch.is_empty() {
                    self.dispatch_stored(&rid, batch).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guandan_protocol::decode_server;
    use std::time::{Duration, Instant};

    /// Full practice match driven end-to-end through AppState with all timers
    /// fast-forwarded: the hand-2 tribute phase must send TributeReturnTurn
    /// (the missing message that used to soft-lock human players).
    #[tokio::test]
    async fn practice_match_reaches_tribute_and_returns() {
        let state = AppState::new(GameSettings);
        let sid = Uuid::new_v4();
        let (tx, mut rx) = mpsc::channel::<String>(8192);
        state.register(sid, Uuid::new_v4(), tx).await;
        state
            .handle(sid, ClientMessage::PracticeMatch)
            .await
            .unwrap();

        let mut hands_started = 0u32;
        let mut saw_tribute_paid = false;
        let mut saw_return_turn = false;
        let mut saw_tribute_returned = false;

        for _ in 0..20_000 {
            // Fast-forward: expire every timer so bots act, timeouts fire,
            // and confirms complete immediately.
            {
                let mut rooms = state.rooms.lock().await;
                let past = Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .unwrap_or_else(Instant::now);
                for room in rooms.values_mut() {
                    room.reveal_until = None;
                    if room.turn_deadline.is_some() {
                        room.turn_deadline = Some(past);
                    }
                    if room.confirm_deadline.is_some() {
                        room.confirm_deadline = Some(past);
                    }
                }
            }
            state.tick_game().await;
            while let Ok(text) = rx.try_recv() {
                let Ok(sm) = decode_server(&text) else {
                    continue;
                };
                match sm {
                    ServerMessage::GameStart { hand_number, .. } => {
                        hands_started = hands_started.max(hand_number);
                    }
                    ServerMessage::TributePaid { .. } => saw_tribute_paid = true,
                    ServerMessage::TributeReturnTurn { .. } => saw_return_turn = true,
                    ServerMessage::TributeReturned { .. } => saw_tribute_returned = true,
                    _ => {}
                }
            }
            if hands_started >= 2 && saw_tribute_paid && saw_return_turn && saw_tribute_returned {
                break;
            }
        }

        assert!(hands_started >= 2, "second hand never dealt");
        assert!(saw_tribute_paid, "tribute never paid in hand 2");
        assert!(saw_return_turn, "no TributeReturnTurn message sent");
        assert!(saw_tribute_returned, "tribute return never completed");
    }

    /// Leaving mid-hand replaces the human with a bot so the match continues.
    #[tokio::test]
    async fn leave_mid_hand_bot_substitutes() {
        let state = AppState::new(GameSettings);
        let sid = Uuid::new_v4();
        let (tx, mut rx) = mpsc::channel::<String>(8192);
        state.register(sid, Uuid::new_v4(), tx).await;
        state
            .handle(sid, ClientMessage::PracticeMatch)
            .await
            .unwrap();
        // Drain startup messages.
        while rx.try_recv().is_ok() {}

        // Ensure game is in Playing.
        {
            let mut rooms = state.rooms.lock().await;
            let room = rooms.values_mut().next().unwrap();
            assert!(room.game.is_some());
            room.game.as_mut().unwrap().phase = MatchPhase::Playing;
        }

        state.handle(sid, ClientMessage::LeaveRoom).await.unwrap();

        let rooms = state.rooms.lock().await;
        // Room may still exist if... wait, all humans gone → room removed.
        // Practice: only one human; leave empties room.
        assert!(rooms.is_empty(), "room dropped when last human leaves");
        drop(rooms);

        // Two humans: leave one, bot fills, room stays.
        let state = AppState::new(GameSettings);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let (tx_a, _rx_a) = mpsc::channel::<String>(256);
        let (tx_b, mut rx_b) = mpsc::channel::<String>(256);
        state.register(a, Uuid::new_v4(), tx_a).await;
        state.register(b, Uuid::new_v4(), tx_b).await;
        state
            .handle(a, ClientMessage::CreateRoom { name: "A".into() })
            .await
            .unwrap();
        let room_id = {
            let sessions = state.sessions.lock().await;
            sessions.get(&a).unwrap().room_id.clone().unwrap()
        };
        state
            .handle(
                b,
                ClientMessage::JoinRoom {
                    room_id: room_id.clone(),
                },
            )
            .await
            .unwrap();
        // Fill remaining with bots and start.
        {
            let mut rooms = state.rooms.lock().await;
            let room = rooms.get_mut(&room_id).unwrap();
            room.fill_bots();
            for s in room.slots.iter_mut() {
                s.ready = true;
            }
            let msgs = room.start_game();
            drop(rooms);
            state.dispatch_stored(&room_id, msgs).await;
        }
        {
            let mut rooms = state.rooms.lock().await;
            rooms
                .get_mut(&room_id)
                .unwrap()
                .game
                .as_mut()
                .unwrap()
                .phase = MatchPhase::Playing;
        }
        while rx_b.try_recv().is_ok() {}

        state.handle(a, ClientMessage::LeaveRoom).await.unwrap();

        let rooms = state.rooms.lock().await;
        let room = rooms
            .get(&room_id)
            .expect("room stays with remaining human");
        let bots = room.slots.iter().filter(|s| s.is_bot).count();
        let humans = room.slots.iter().filter(|s| s.session_id.is_some()).count();
        assert_eq!(humans, 1);
        assert_eq!(bots, 3, "departed seat becomes bot");
        // B should have seen SeatTaken or PlayerLeft.
        drop(rooms);
        let mut saw_left = false;
        let mut saw_bot = false;
        while let Ok(text) = rx_b.try_recv() {
            if let Ok(sm) = decode_server(&text) {
                match sm {
                    ServerMessage::PlayerLeft { .. } => saw_left = true,
                    ServerMessage::SeatTaken { info, .. } if info.is_bot => saw_bot = true,
                    _ => {}
                }
            }
        }
        assert!(saw_left, "PlayerLeft broadcast");
        assert!(saw_bot, "bot SeatTaken broadcast");
    }

    /// Leaving on the result board: bot fills and auto-confirms that seat.
    #[tokio::test]
    async fn leave_on_result_board_bot_confirms() {
        let state = AppState::new(GameSettings);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let (tx_a, _rx_a) = mpsc::channel::<String>(256);
        let (tx_b, mut rx_b) = mpsc::channel::<String>(256);
        state.register(a, Uuid::new_v4(), tx_a).await;
        state.register(b, Uuid::new_v4(), tx_b).await;
        state
            .handle(a, ClientMessage::CreateRoom { name: "A".into() })
            .await
            .unwrap();
        let room_id = {
            let sessions = state.sessions.lock().await;
            sessions.get(&a).unwrap().room_id.clone().unwrap()
        };
        state
            .handle(
                b,
                ClientMessage::JoinRoom {
                    room_id: room_id.clone(),
                },
            )
            .await
            .unwrap();
        {
            let mut rooms = state.rooms.lock().await;
            let room = rooms.get_mut(&room_id).unwrap();
            room.fill_bots();
            for s in room.slots.iter_mut() {
                s.ready = true;
            }
            let msgs = room.start_game();
            // Put match on HandOver, only seat b unconfirmed among humans.
            {
                let g = room.game.as_mut().unwrap();
                g.phase = MatchPhase::HandOver;
                g.confirmed = [false; 4];
            }
            room.confirm_deadline = Some(Instant::now() + Duration::from_secs(10));
            drop(rooms);
            state.dispatch_stored(&room_id, msgs).await;
        }
        while rx_b.try_recv().is_ok() {}

        // A leaves on the board → bot + auto-confirm bots (seats 0,2,3 if bots).
        state.handle(a, ClientMessage::LeaveRoom).await.unwrap();

        let rooms = state.rooms.lock().await;
        let room = rooms.get(&room_id).unwrap();
        let g = room.game.as_ref().unwrap();
        // Seat of A was 0 (first join); now bot and confirmed.
        assert!(room.slots[0].is_bot);
        assert!(g.confirmed[0], "departed seat auto-confirmed as bot");
        // Other bots confirmed too.
        assert!(g.confirmed[2] && g.confirmed[3]);
        // Human B still needs to confirm (or wait 10s).
        assert!(!g.confirmed[1] || room.slots[1].session_id == Some(b));
    }
}
