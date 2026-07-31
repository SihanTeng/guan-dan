//! Client application state.

use crossterm::event::KeyCode;
use guandan_core::{Card, FinishRank, HandType, Rank, Seat, TeamId};
use guandan_protocol::{ClientMessage, PublicPlay, SeatInfo, ServerMessage};
use uuid::Uuid;

use crate::net::NetHandle;

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
    pub fn new(net: NetHandle) -> Self {
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
        }
    }

    pub fn tick(&mut self) {
        if self.status_ticks > 0 {
            self.status_ticks -= 1;
            if self.status_ticks == 0 {
                self.status.clear();
            }
        }
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
                self.current = Some(lead);
                self.must_lead = true;
                self.last_play = None;
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
            } => {
                self.current = Some(seat);
                self.must_lead = must_lead;
                self.last_play = last_play;
                if seat == self.my_seat {
                    self.set_status(if must_lead {
                        "轮到你出牌 · Your lead".to_string()
                    } else {
                        "轮到你压牌 · Your turn".to_string()
                    });
                }
            }
            ServerMessage::CardPlayed {
                seat,
                cards,
                hand_type,
                counts,
            } => {
                self.counts = counts;
                self.last_play = Some(PublicPlay {
                    seat,
                    cards: cards.clone(),
                    hand_type,
                    key: cards.first().map(|c| c.rank).unwrap_or(Rank::R2),
                });
                if seat == self.my_seat {
                    // remove played from local hand
                    let ids: std::collections::HashSet<_> = cards.iter().map(|c| c.id).collect();
                    self.hand.retain(|c| !ids.contains(&c.id));
                    self.selected = vec![false; self.hand.len()];
                    self.cursor = self.cursor.min(self.hand.len().saturating_sub(1));
                }
            }
            ServerMessage::PlayerPass { seat } => {
                if seat == self.my_seat {
                    self.set_status("你选择不出 · Pass");
                }
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
                // stay in game
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                self.prev_screen = Screen::Game;
                self.screen = Screen::Help;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.show_counter = !self.show_counter;
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor + 1 < self.hand.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Char(' ') => {
                if let Some(sel) = self.selected.get_mut(self.cursor) {
                    *sel = !*sel;
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                if self.current == Some(self.my_seat) && !self.must_lead && !self.tribute_mode {
                    self.net.send(ClientMessage::Pass);
                    self.clear_selection();
                }
            }
            KeyCode::Enter => {
                self.submit_play();
            }
            KeyCode::Char(ch) => {
                self.rank_key(ch);
            }
            _ => {}
        }
        false
    }

    fn rank_key(&mut self, ch: char) {
        let rank = match Rank::from_key_char(ch) {
            Some(r) => r,
            None => return,
        };
        // Toggle all cards of this rank; if none selected of rank, select one more unselected
        let indices: Vec<usize> = self
            .hand
            .iter()
            .enumerate()
            .filter(|(_, c)| c.rank == rank)
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            return;
        }
        let any_selected = indices.iter().any(|&i| self.selected[i]);
        if any_selected {
            for i in indices {
                self.selected[i] = false;
            }
        } else {
            for i in indices {
                self.selected[i] = true;
            }
            self.cursor = self
                .hand
                .iter()
                .position(|c| c.rank == rank)
                .unwrap_or(self.cursor);
        }
    }

    fn clear_selection(&mut self) {
        for s in &mut self.selected {
            *s = false;
        }
    }

    fn submit_play(&mut self) {
        if self.current != Some(self.my_seat) {
            self.set_status("还没轮到你 · Not your turn");
            return;
        }
        let ids: Vec<u8> = self
            .hand
            .iter()
            .zip(self.selected.iter())
            .filter(|(_, sel)| **sel)
            .map(|(c, _)| c.id)
            .collect();
        if ids.is_empty() {
            self.set_status("请先选牌 · Select cards");
            return;
        }
        if self.tribute_mode {
            let to = self.tribute_payers.first().copied().unwrap_or(0);
            self.net.send(ClientMessage::ReturnTribute {
                card_id: ids[0],
                to_seat: to,
            });
            self.clear_selection();
            return;
        }
        self.net.send(ClientMessage::PlayCards { card_ids: ids });
        self.clear_selection();
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
