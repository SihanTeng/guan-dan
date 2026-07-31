//! Client application state.

use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use guandan_core::{
    find_card_indices_in_hand, find_cards_in_hand, Card, FinishRank, HandType, Rank, Seat, TeamId,
};
use guandan_protocol::{ClientMessage, PublicPlay, SeatInfo, ServerMessage};
use uuid::Uuid;

use crate::net::NetHandle;
use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Lobby,
    Room,
    Game,
    Help,
    HandResult,
    MatchOver,
}

pub struct App {
    pub net: NetHandle,
    pub screen: Screen,
    pub prev_screen: Screen,
    pub should_quit: bool,
    pub status: String,
    pub status_ticks: u32,
    pub session_id: Option<Uuid>,
    pub online: u32,
    pub room_id: Option<String>,
    pub my_seat: Seat,
    pub seats: Vec<SeatInfo>,
    pub hand: Vec<Card>,
    pub hand_level: Rank,
    pub team_levels: [Rank; 2],
    pub counts: [usize; 4],
    pub current: Option<Seat>,
    pub must_lead: bool,
    pub last_play: Option<PublicPlay>,
    pub cursor: usize,
    pub selected: Vec<bool>,
    pub show_counter: bool,
    pub finish_order: Vec<(Seat, FinishRank)>,
    pub last_hand_result: String,
    pub winner_team: Option<TeamId>,
    pub input_buf: String,
    pub lobby_focus: LobbyFocus,
    pub tribute_mode: bool,
    pub tribute_payers: Vec<Seat>,
    /// Typed rank sequence for play, e.g. `"34567"` / `"KK"` / `"3334"` (ddz-style).
    pub play_buf: String,
    pub settings: Settings,
    /// When the current turn ends (local Instant, from server timeout_secs).
    pub turn_deadline: Option<Instant>,
    pub turn_timeout_secs: u32,
    /// Hold last play on screen until this instant (others' 出牌 reveal).
    pub reveal_until: Option<Instant>,
    /// Seat that just played (for highlight during reveal).
    pub reveal_seat: Option<Seat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LobbyFocus {
    Practice,
    Create,
    Quick,
    Join,
    Help,
}

impl App {
    pub fn new(net: NetHandle, settings: Settings) -> Self {
        Self {
            net,
            screen: Screen::Lobby,
            prev_screen: Screen::Lobby,
            should_quit: false,
            status: "正在连接…".into(),
            status_ticks: 60,
            session_id: None,
            online: 0,
            room_id: None,
            my_seat: 0,
            seats: Vec::new(),
            hand: Vec::new(),
            hand_level: Rank::R2,
            team_levels: [Rank::R2, Rank::R2],
            counts: [0; 4],
            current: None,
            must_lead: true,
            last_play: None,
            cursor: 0,
            selected: Vec::new(),
            show_counter: false,
            finish_order: Vec::new(),
            last_hand_result: String::new(),
            winner_team: None,
            input_buf: String::new(),
            lobby_focus: LobbyFocus::Practice,
            tribute_mode: false,
            tribute_payers: Vec::new(),
            play_buf: String::new(),
            turn_timeout_secs: settings.turn_timeout_secs as u32,
            settings,
            turn_deadline: None,
            reveal_until: None,
            reveal_seat: None,
        }
    }

    pub fn tick(&mut self) {
        if self.status_ticks > 0 {
            self.status_ticks -= 1;
            if self.status_ticks == 0 {
                self.status.clear();
            }
        }
        if let Some(until) = self.reveal_until {
            if Instant::now() >= until {
                self.reveal_until = None;
                self.reveal_seat = None;
            }
        }
    }

    /// Seconds left on the turn timer (None if not in a timed turn).
    pub fn turn_secs_left(&self) -> Option<u32> {
        let deadline = self.turn_deadline?;
        let left = deadline.saturating_duration_since(Instant::now());
        Some(left.as_secs() as u32)
    }

    /// Whether we are in the play-reveal hold window.
    pub fn revealing(&self) -> bool {
        self.reveal_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false)
    }

    fn start_turn_timer(&mut self, timeout_secs: u32) {
        let secs = if timeout_secs == 0 {
            self.settings.turn_timeout_secs as u32
        } else {
            timeout_secs
        };
        self.turn_timeout_secs = secs;
        self.turn_deadline = Some(Instant::now() + Duration::from_secs(secs as u64));
    }

    fn start_reveal(&mut self, seat: Seat, _server_reveal_secs: u32) {
        // Local settings are authoritative for how long we display others' plays.
        let hold = self.settings.play_reveal_secs.max(1);
        self.reveal_until = Some(Instant::now() + Duration::from_secs(hold));
        self.reveal_seat = Some(seat);
    }

    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
        self.status_ticks = 80;
    }

    pub fn on_server(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::Connected { session_id, .. } => {
                self.session_id = Some(session_id);
                self.set_status("已连接 · Connected");
            }
            ServerMessage::Pong => {}
            ServerMessage::Error { message } => self.set_status(message),
            ServerMessage::OnlineCount { count } => self.online = count,
            ServerMessage::RoomCreated { room_id } => {
                self.room_id = Some(room_id.clone());
                self.set_status(format!("房间已创建 {room_id}"));
            }
            ServerMessage::RoomJoined {
                room_id,
                seat,
                players,
            } => {
                self.room_id = Some(room_id);
                self.my_seat = seat;
                self.seats = players;
                self.screen = Screen::Room;
            }
            ServerMessage::PlayerJoined { info, .. } => {
                if let Some(s) = self.seats.iter_mut().find(|s| s.seat == info.seat) {
                    *s = info;
                } else {
                    self.seats.push(info);
                }
            }
            ServerMessage::PlayerLeft { seat } => {
                if let Some(s) = self.seats.iter_mut().find(|s| s.seat == seat) {
                    s.name = format!("空位{}", seat + 1);
                    s.ready = false;
                    s.is_bot = false;
                }
            }
            ServerMessage::PlayerReady { seat, ready } => {
                if let Some(s) = self.seats.iter_mut().find(|s| s.seat == seat) {
                    s.ready = ready;
                }
            }
            ServerMessage::MatchFound { room_id, seat } => {
                self.room_id = Some(room_id);
                self.my_seat = seat;
                self.set_status("匹配成功 · Match found");
            }
            ServerMessage::GameStart {
                seats,
                team_levels,
                hand_level,
                ..
            } => {
                self.seats = seats;
                self.team_levels = team_levels;
                self.hand_level = hand_level;
                self.screen = Screen::Game;
                self.finish_order.clear();
            }
            ServerMessage::Deal {
                hand,
                hand_level,
                lead,
                counts,
            } => {
                self.hand = hand;
                self.hand_level = hand_level;
                self.counts = counts;
                self.selected = vec![false; self.hand.len()];
                self.cursor = 0;
                self.play_buf.clear();
                self.current = Some(lead);
                self.must_lead = true;
                self.last_play = None;
                self.turn_deadline = None;
                self.reveal_until = None;
                self.reveal_seat = None;
                self.screen = Screen::Game;
                self.tribute_mode = false;
                self.set_status(format!(
                    "发牌 · 级牌 {} · 先手 座位{}",
                    hand_level.label(),
                    lead + 1
                ));
            }
            ServerMessage::TributePaid { from, card, to } => {
                self.set_status(format!(
                    "进贡: 座位{} → 座位{} {}",
                    from + 1,
                    to + 1,
                    card.display()
                ));
            }
            ServerMessage::AntiTribute { .. } => {
                self.set_status("抗贡！双大王 · Anti-tribute");
            }
            ServerMessage::TributeReturnTurn { seat, payers } => {
                self.current = Some(seat);
                self.tribute_mode = seat == self.my_seat;
                self.tribute_payers = payers;
                if seat == self.my_seat {
                    self.set_status("请选择回贡牌 (≤10) 后按 Enter · Return tribute");
                }
            }
            ServerMessage::TributeReturned { from, card, to } => {
                self.tribute_mode = false;
                self.set_status(format!(
                    "回贡: 座位{} → 座位{} {}",
                    from + 1,
                    to + 1,
                    card.display()
                ));
            }
            ServerMessage::PlayTurn {
                seat,
                must_lead,
                last_play,
                timeout_secs,
            } => {
                self.current = Some(seat);
                self.must_lead = must_lead;
                // Keep showing held last play during reveal; otherwise take server snapshot
                if !self.revealing() && last_play.is_some() {
                    self.last_play = last_play;
                }
                self.start_turn_timer(timeout_secs);
                if seat == self.my_seat {
                    self.set_status(if must_lead {
                        format!("轮到你 · {timeout_secs}s")
                    } else {
                        format!("请出牌 · {timeout_secs}s")
                    });
                }
            }
            ServerMessage::CardPlayed {
                seat,
                cards,
                hand_type,
                counts,
                reveal_secs,
            } => {
                self.counts = counts;
                self.last_play = Some(PublicPlay {
                    seat,
                    cards: cards.clone(),
                    hand_type,
                    key: cards.first().map(|c| c.rank).unwrap_or(Rank::R2),
                });
                if seat == self.my_seat {
                    let ids: std::collections::HashSet<_> = cards.iter().map(|c| c.id).collect();
                    self.hand.retain(|c| !ids.contains(&c.id));
                    self.selected = vec![false; self.hand.len()];
                    self.cursor = self.cursor.min(self.hand.len().saturating_sub(1));
                    self.clear_play_input();
                } else {
                    // Hold other players' / bots' plays on screen
                    self.start_reveal(seat, reveal_secs);
                    let name = self.seat_name(seat);
                    self.set_status(format!(
                        "{} 出了 {} · 展示 {}s",
                        name,
                        hand_type_cn(hand_type),
                        self.settings.play_reveal_secs
                    ));
                }
            }
            ServerMessage::PlayerPass { seat, reveal_secs } => {
                if seat == self.my_seat {
                    self.set_status("不出 · Pass");
                    self.clear_play_input();
                } else {
                    self.start_reveal(seat, reveal_secs);
                    self.set_status(format!("{} 不出", self.seat_name(seat)));
                }
            }
            ServerMessage::TurnTimeout { seat } => {
                self.set_status(format!("座位{} 超时 · Turn timeout", seat + 1));
            }
            ServerMessage::PlayerOut { seat, rank } => {
                self.finish_order.push((seat, rank));
                let name = match rank {
                    FinishRank::Banker => "上游",
                    FinishRank::Follower => "二游",
                    FinishRank::Third => "三游",
                    FinishRank::Dweller => "下游",
                };
                self.set_status(format!("座位{} → {name}", seat + 1));
            }
            ServerMessage::HandResult {
                level_gain,
                new_levels,
                winning_team,
                match_over,
                winner_team,
                ..
            } => {
                self.team_levels = new_levels;
                let team = match winning_team {
                    TeamId::A => "队A",
                    TeamId::B => "队B",
                };
                self.last_hand_result = format!("{team} +{level_gain} 级");
                if match_over {
                    self.winner_team = winner_team;
                    self.screen = Screen::MatchOver;
                } else {
                    self.screen = Screen::HandResult;
                }
            }
            ServerMessage::MatchOver {
                winner_team,
                levels,
            } => {
                self.winner_team = Some(winner_team);
                self.team_levels = levels;
                self.screen = Screen::MatchOver;
            }
            ServerMessage::RoomList { rooms } => {
                let s = rooms
                    .iter()
                    .map(|r| format!("{} ({}/4)", r.room_id, r.players))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.set_status(if s.is_empty() {
                    "暂无房间 · No rooms".into()
                } else {
                    s
                });
            }
            ServerMessage::Chat { name, content, .. } => {
                self.set_status(format!("{name}: {content}"));
            }
        }
    }

    /// Returns true if should quit.
    pub fn on_key(&mut self, code: KeyCode) -> bool {
        match self.screen {
            Screen::Help => {
                if matches!(code, KeyCode::Esc | KeyCode::Char('h') | KeyCode::Char('H')) {
                    self.screen = self.prev_screen;
                }
                false
            }
            Screen::HandResult | Screen::MatchOver => {
                if matches!(code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ')) {
                    if self.screen == Screen::MatchOver {
                        self.screen = Screen::Lobby;
                        self.net.send(ClientMessage::LeaveRoom);
                    } else {
                        self.screen = Screen::Game;
                    }
                }
                false
            }
            Screen::Lobby => self.on_lobby_key(code),
            Screen::Room => self.on_room_key(code),
            Screen::Game => self.on_game_key(code),
        }
    }

    fn on_lobby_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => return true,
            KeyCode::Up | KeyCode::Char('k') => {
                self.lobby_focus = match self.lobby_focus {
                    LobbyFocus::Practice => LobbyFocus::Help,
                    LobbyFocus::Create => LobbyFocus::Practice,
                    LobbyFocus::Quick => LobbyFocus::Create,
                    LobbyFocus::Join => LobbyFocus::Quick,
                    LobbyFocus::Help => LobbyFocus::Join,
                };
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.lobby_focus = match self.lobby_focus {
                    LobbyFocus::Practice => LobbyFocus::Create,
                    LobbyFocus::Create => LobbyFocus::Quick,
                    LobbyFocus::Quick => LobbyFocus::Join,
                    LobbyFocus::Join => LobbyFocus::Help,
                    LobbyFocus::Help => LobbyFocus::Practice,
                };
            }
            KeyCode::Char(c) if self.lobby_focus == LobbyFocus::Join => {
                if c.is_ascii_alphanumeric() || c == '-' {
                    self.input_buf.push(c);
                }
            }
            KeyCode::Backspace if self.lobby_focus == LobbyFocus::Join => {
                self.input_buf.pop();
            }
            KeyCode::Enter => match self.lobby_focus {
                LobbyFocus::Practice => {
                    self.net.send(ClientMessage::PracticeMatch);
                    self.set_status("开始人机练习…");
                }
                LobbyFocus::Create => {
                    self.net.send(ClientMessage::CreateRoom {
                        name: String::new(),
                    });
                }
                LobbyFocus::Quick => {
                    self.net.send(ClientMessage::QuickMatch);
                    self.set_status("快速匹配中…");
                }
                LobbyFocus::Join => {
                    if !self.input_buf.is_empty() {
                        self.net.send(ClientMessage::JoinRoom {
                            room_id: self.input_buf.clone(),
                        });
                    } else {
                        self.net.send(ClientMessage::ListRooms);
                    }
                }
                LobbyFocus::Help => {
                    self.prev_screen = Screen::Lobby;
                    self.screen = Screen::Help;
                }
            },
            KeyCode::Char('h') | KeyCode::Char('H') => {
                self.prev_screen = Screen::Lobby;
                self.screen = Screen::Help;
            }
            _ => {}
        }
        false
    }

    fn on_room_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Esc => {
                self.net.send(ClientMessage::LeaveRoom);
                self.screen = Screen::Lobby;
            }
            KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Enter => {
                self.net.send(ClientMessage::Ready);
                self.set_status("已准备 · Ready");
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                self.prev_screen = Screen::Room;
                self.screen = Screen::Help;
            }
            _ => {}
        }
        false
    }

    fn on_game_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Esc => {
                // Clear typed buffer + selection (stay in game)
                if !self.play_buf.is_empty() || self.selected.iter().any(|s| *s) {
                    self.clear_play_input();
                    self.set_status("已清空输入 · Cleared");
                }
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                self.prev_screen = Screen::Game;
                self.screen = Screen::Help;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.show_counter = !self.show_counter;
            }
            KeyCode::Left | KeyCode::Char('[') => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Right | KeyCode::Char(']') => {
                if self.cursor + 1 < self.hand.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Char(' ') => {
                // Visual toggle: leave typed buffer mode
                self.play_buf.clear();
                if let Some(sel) = self.selected.get_mut(self.cursor) {
                    *sel = !*sel;
                }
            }
            KeyCode::Backspace | KeyCode::Delete => {
                if !self.play_buf.is_empty() {
                    self.play_buf.pop();
                    let _ = self.sync_selection_from_buf();
                } else if let Some(sel) = self.selected.get_mut(self.cursor) {
                    // no typed input → deselect cursor card
                    *sel = false;
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                // If buffer is "P" only, still pass; if buffer has cards, 'P' would be weird.
                // Pass always wins when not leading.
                if self.current == Some(self.my_seat) && !self.must_lead && !self.tribute_mode {
                    self.net.send(ClientMessage::Pass);
                    self.clear_play_input();
                    self.set_status("不出 · Pass");
                }
            }
            KeyCode::Enter => {
                self.submit_play();
            }
            KeyCode::Char(ch) => {
                // ddz-style: append rank chars to play buffer
                self.type_rank_char(ch);
            }
            _ => {}
        }
        false
    }

    /// Append a rank key into the typed play buffer and highlight matching cards.
    fn type_rank_char(&mut self, ch: char) {
        let upper = ch.to_ascii_uppercase();
        // Allow rank keys + '0' for 10
        if Rank::from_key_char(upper).is_none() && upper != '0' {
            return;
        }
        // Cap buffer length (hand is 27)
        if self.play_buf.len() >= 32 {
            return;
        }
        self.play_buf.push(upper);
        match self.sync_selection_from_buf() {
            Ok(()) => {
                let n = self.selected.iter().filter(|s| **s).count();
                self.set_status(format!(
                    "输入 {}  · 已匹配 {n} 张  Enter 出牌",
                    self.play_buf
                ));
            }
            Err(e) => {
                // Keep buffer so user can backspace; soft-fail status
                self.set_status(format!("{}  [{}]", e, self.play_buf));
                // Still try partial highlight of longest valid prefix? Keep last good selection.
            }
        }
    }

    /// Recompute `selected` from `play_buf`. Empty buffer clears selection.
    fn sync_selection_from_buf(&mut self) -> Result<(), String> {
        if self.selected.len() != self.hand.len() {
            self.selected = vec![false; self.hand.len()];
        }
        if self.play_buf.is_empty() {
            for s in &mut self.selected {
                *s = false;
            }
            return Ok(());
        }
        match find_card_indices_in_hand(&self.hand, &self.play_buf) {
            Ok(indices) => {
                for s in &mut self.selected {
                    *s = false;
                }
                for i in indices {
                    if let Some(s) = self.selected.get_mut(i) {
                        *s = true;
                    }
                    self.cursor = i;
                }
                Ok(())
            }
            Err(e) => {
                // Don't wipe previous selection on incomplete "10" mid-type
                if self.play_buf.ends_with('1') {
                    return Ok(());
                }
                Err(e)
            }
        }
    }

    fn clear_selection(&mut self) {
        for s in &mut self.selected {
            *s = false;
        }
    }

    fn clear_play_input(&mut self) {
        self.play_buf.clear();
        self.clear_selection();
    }

    fn submit_play(&mut self) {
        if self.current != Some(self.my_seat) {
            self.set_status("还没轮到你 · Not your turn");
            return;
        }

        // Prefer typed buffer (ddz-style), else visual selection
        let ids: Vec<u8> = if !self.play_buf.is_empty() {
            // Allow PASS typed in buffer
            let upper = self.play_buf.to_ascii_uppercase();
            if upper == "P" || upper == "PASS" {
                if !self.must_lead && !self.tribute_mode {
                    self.net.send(ClientMessage::Pass);
                    self.clear_play_input();
                    self.set_status("不出 · Pass");
                } else {
                    self.set_status("必须出牌 · Must play");
                }
                return;
            }
            match find_cards_in_hand(&self.hand, &self.play_buf) {
                Ok(cards) => cards.iter().map(|c| c.id).collect(),
                Err(e) => {
                    self.set_status(e);
                    return;
                }
            }
        } else {
            self.hand
                .iter()
                .zip(self.selected.iter())
                .filter(|(_, sel)| **sel)
                .map(|(c, _)| c.id)
                .collect()
        };

        if ids.is_empty() {
            self.set_status("请选牌或输入点数 · Type ranks e.g. 34567 / KK");
            return;
        }
        if self.tribute_mode {
            let to = self.tribute_payers.first().copied().unwrap_or(0);
            self.net.send(ClientMessage::ReturnTribute {
                card_id: ids[0],
                to_seat: to,
            });
            self.clear_play_input();
            return;
        }
        self.net.send(ClientMessage::PlayCards { card_ids: ids });
        self.clear_play_input();
    }

    pub fn relative_seat(&self, offset: usize) -> Seat {
        (self.my_seat + offset) % 4
    }

    pub fn seat_name(&self, seat: Seat) -> String {
        self.seats
            .iter()
            .find(|s| s.seat == seat)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| format!("座位{}", seat + 1))
    }
}

pub fn hand_type_cn(t: HandType) -> &'static str {
    t.chinese_name()
}
