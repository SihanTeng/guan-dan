//! Global server state: sessions, rooms, matchmaking.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Result};
use guandan_core::Action;
use guandan_protocol::{encode_server, ClientMessage, RoomSummary, ServerMessage};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::room::Room;
use crate::settings::GameSettings;

pub type OutTx = mpsc::UnboundedSender<String>;

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
            let mut rooms = self.rooms.lock().await;
            if let Some(room) = rooms.get_mut(&rid) {
                if let Some(seat) = room.seat_of(session_id) {
                    room.slots[seat].session_id = None;
                    room.slots[seat].ready = false;
                    room.slots[seat].name = format!("空位{}", seat + 1);
                    let msg = encode_server(&ServerMessage::PlayerLeft { seat }).unwrap();
                    self.broadcast_room_locked(room, None, msg).await;
                }
                if room.player_count() == 0 {
                    rooms.remove(&rid);
                }
            }
        }
        let mut q = self.quick_queue.lock().await;
        q.retain(|id| *id != session_id);
    }

    pub async fn online_count(&self) -> u32 {
        self.sessions.lock().await.len() as u32
    }

    pub async fn send_to(&self, session_id: Uuid, text: String) -> Result<()> {
        let sessions = self.sessions.lock().await;
        if let Some(s) = sessions.get(&session_id) {
            let _ = s.tx.send(text);
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
                    let _ = s.tx.send(text.clone());
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
                let mut rooms = self.rooms.lock().await;
                let room = rooms
                    .get_mut(&room_id)
                    .ok_or_else(|| anyhow::anyhow!("房间不存在"))?;
                if room.game.is_some() {
                    bail!("游戏已开始");
                }
                let name = {
                    drop(rooms);
                    self.name_of(session_id).await
                };
                let mut rooms = self.rooms.lock().await;
                let room = rooms.get_mut(&room_id).unwrap();
                let seat = room
                    .join(session_id, name)
                    .ok_or_else(|| anyhow::anyhow!("房间已满"))?;
                {
                    let mut sessions = self.sessions.lock().await;
                    if let Some(s) = sessions.get_mut(&session_id) {
                        s.room_id = Some(room_id.clone());
                    }
                }
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
                // RoomJoined to joiner (broadcast already sent PlayerJoined to all including joiner)
                let _ = self.send_to(session_id, joined).await;
            }
            ClientMessage::LeaveRoom => {
                self.leave_room(session_id).await?;
            }
            ClientMessage::PracticeMatch => {
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
            ClientMessage::Chat { content } => {
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
            let mut rooms = self.rooms.lock().await;
            if let Some(room) = rooms.get_mut(&rid) {
                if let Some(seat) = room.seat_of(session_id) {
                    room.slots[seat].session_id = None;
                    room.slots[seat].is_bot = false;
                    room.slots[seat].ready = false;
                    room.slots[seat].name = format!("空位{}", seat + 1);
                    let t = encode_server(&ServerMessage::PlayerLeft { seat })?;
                    self.broadcast_room_locked(room, None, t).await;
                }
            }
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
        let rid = format!("Q{}", self.room_seq.fetch_add(1, Ordering::SeqCst));
        let mut room = Room::new(rid.clone(), false, self.settings);
        for pid in &players {
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
        for (i, pid) in players.iter().enumerate() {
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

    /// Periodic: turn timeouts, bot steps (respecting play reveal), hand continue.
    pub async fn tick_game(&self) {
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
            let cont = room.maybe_continue();
            drop(rooms);
            if !timeout_msgs.is_empty() || !bot_msgs.is_empty() || !cont.is_empty() {
                self.dispatch_stored(&rid, timeout_msgs).await;
                self.dispatch_stored(&rid, bot_msgs).await;
                self.dispatch_stored(&rid, cont).await;
            }
        }
    }
}
