//! Room and in-game match hosting (with turn timer + play reveal).

use std::time::{Duration, Instant};

use guandan_bot::decide_play;
use guandan_core::{can_follow, team_of, Action, Event, FinishRank, Match, MatchPhase, Seat};
use guandan_protocol::{
    PublicPlay, SeatInfo, ServerMessage, CONFIRM_TIMEOUT_SECS, REPARTY_TIMEOUT_SECS,
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use uuid::Uuid;

use crate::settings::GameSettings;

#[derive(Debug, Clone)]
pub struct PlayerSlot {
    pub session_id: Option<Uuid>,
    pub name: String,
    pub is_bot: bool,
    pub ready: bool,
}

pub struct Room {
    pub id: String,
    pub slots: [PlayerSlot; 4],
    pub game: Option<Match>,
    pub rng: StdRng,
    pub settings: GameSettings,
    /// When the current turn expires (server-side).
    pub turn_deadline: Option<Instant>,
    /// Bots (and next acts) wait until this so players can see the last play.
    pub reveal_until: Option<Instant>,
    /// When HandOver/MatchOver confirm window ends (auto-confirm remaining).
    pub confirm_deadline: Option<Instant>,
    /// When vacant seats get bot-filled if no human joins (re-party).
    pub reparty_deadline: Option<Instant>,
    /// After all ranks confirmed, deal exactly once on next tick.
    pub pending_next_deal: bool,
}

impl Room {
    pub fn new(id: String, _practice: bool, settings: GameSettings) -> Self {
        Self {
            id,
            slots: std::array::from_fn(|i| PlayerSlot {
                session_id: None,
                name: format!("空位{}", i + 1),
                is_bot: false,
                ready: false,
            }),
            game: None,
            rng: StdRng::from_os_rng(),
            settings,
            turn_deadline: None,
            reveal_until: None,
            confirm_deadline: None,
            reparty_deadline: None,
            pending_next_deal: false,
        }
    }

    pub fn player_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| s.session_id.is_some() || s.is_bot)
            .count()
    }

    pub fn seat_of(&self, session: Uuid) -> Option<Seat> {
        self.slots
            .iter()
            .position(|s| s.session_id == Some(session))
    }

    pub fn join(&mut self, session: Uuid, name: String) -> Option<Seat> {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.session_id.is_none() && !slot.is_bot {
                slot.session_id = Some(session);
                slot.name = name;
                slot.ready = false;
                return Some(i);
            }
        }
        None
    }

    pub fn fill_bots(&mut self) {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.session_id.is_none() && !slot.is_bot {
                slot.is_bot = true;
                slot.name = format!("机器人{}", i + 1);
                slot.ready = true;
            }
        }
    }

    pub fn all_ready(&self) -> bool {
        self.slots
            .iter()
            .all(|s| (s.session_id.is_some() || s.is_bot) && s.ready)
            && self.player_count() == 4
    }

    pub fn seat_infos(&self) -> Vec<SeatInfo> {
        self.slots
            .iter()
            .enumerate()
            .map(|(i, s)| SeatInfo {
                seat: i,
                name: s.name.clone(),
                is_bot: s.is_bot,
                ready: s.ready,
                team: team_of(i),
            })
            .collect()
    }

    pub fn start_game(&mut self) -> Vec<(Option<Uuid>, ServerMessage)> {
        let mut game = Match::new();
        let events = game.apply(0, Action::Deal, &mut self.rng).expect("deal");
        self.game = Some(game);
        let msgs = self.broadcast_events(&events);
        self.note_timing_from_events(&events);
        msgs
    }

    pub fn apply_action(
        &mut self,
        seat: Seat,
        action: Action,
    ) -> Result<Vec<(Option<Uuid>, ServerMessage)>, String> {
        let game = self.game.as_mut().ok_or("游戏未开始")?;
        let events = game
            .apply(seat, action, &mut self.rng)
            .map_err(|e| e.to_string())?;
        let msgs = self.broadcast_events(&events);
        self.note_timing_from_events(&events);
        Ok(msgs)
    }

    /// Update turn deadline / reveal window from engine events.
    fn note_timing_from_events(&mut self, events: &[Event]) {
        let now = Instant::now();
        for ev in events {
            match ev {
                Event::Turn { .. } => {
                    self.turn_deadline = Some(now + self.settings.turn_timeout());
                    self.confirm_deadline = None;
                }
                Event::Played { .. } | Event::Passed { .. } => {
                    self.reveal_until = Some(now + self.settings.play_reveal());
                }
                Event::HandResult { .. } => {
                    self.turn_deadline = None;
                    self.confirm_deadline =
                        Some(now + Duration::from_secs(CONFIRM_TIMEOUT_SECS as u64));
                    self.reparty_deadline =
                        Some(now + Duration::from_secs(REPARTY_TIMEOUT_SECS as u64));
                }
                Event::Dealt { .. } => {
                    self.turn_deadline = None;
                    self.confirm_deadline = None;
                    self.reparty_deadline = None;
                    self.reveal_until = Some(now + Duration::from_millis(400));
                }
                Event::AllConfirmed { match_over: false } => {
                    self.confirm_deadline = None;
                    self.pending_next_deal = true;
                }
                Event::AllConfirmed { match_over: true } => {
                    self.confirm_deadline = None;
                    self.pending_next_deal = false;
                    self.reparty_deadline =
                        Some(now + Duration::from_secs(REPARTY_TIMEOUT_SECS as u64));
                }
                _ => {}
            }
        }
    }

    /// Messages to send: (Some(session) for private, None for all human seats).
    pub fn broadcast_events(&self, events: &[Event]) -> Vec<(Option<Uuid>, ServerMessage)> {
        let mut out = Vec::new();
        let game = match &self.game {
            Some(g) => g,
            None => return out,
        };
        let timeout_secs = self.settings.turn_secs();
        let reveal_secs = self.settings.reveal_secs();

        for ev in events {
            match ev {
                Event::Dealt {
                    hand_level, lead, ..
                } => {
                    for (seat, slot) in self.slots.iter().enumerate() {
                        if let Some(sid) = slot.session_id {
                            let hand = game.players[seat].hand.clone();
                            out.push((
                                Some(sid),
                                ServerMessage::Deal {
                                    hand,
                                    hand_level: *hand_level,
                                    lead: *lead,
                                    counts: std::array::from_fn(|i| game.players[i].hand.len()),
                                },
                            ));
                        }
                    }
                    out.push((
                        None,
                        ServerMessage::GameStart {
                            seats: self.seat_infos(),
                            team_levels: game.team_levels,
                            hand_level: *hand_level,
                            hand_number: game.hand_number,
                        },
                    ));
                }
                Event::TributePaid { from, card, to } => {
                    out.push((
                        None,
                        ServerMessage::TributePaid {
                            from: *from,
                            card: *card,
                            to: *to,
                        },
                    ));
                }
                Event::AntiTribute { dwellers } => {
                    out.push((
                        None,
                        ServerMessage::AntiTribute {
                            dwellers: dwellers.clone(),
                        },
                    ));
                }
                Event::TributeReturned { from, card, to } => {
                    out.push((
                        None,
                        ServerMessage::TributeReturned {
                            from: *from,
                            card: *card,
                            to: *to,
                        },
                    ));
                }
                Event::Turn { seat, must_lead } => {
                    if game.phase == MatchPhase::Tribute {
                        if let Some(ref t) = game.tribute {
                            out.push((
                                None,
                                ServerMessage::TributeReturnTurn {
                                    seat: *seat,
                                    payers: t.payers.clone(),
                                },
                            ));
                        }
                    }
                    let last_play = game.last_play.as_ref().map(|p| PublicPlay {
                        seat: game.last_player.unwrap_or(0),
                        cards: p.cards.clone(),
                        hand_type: p.ty,
                        key: p.key,
                    });
                    let hand = &game.players[*seat].hand;
                    let follow_ok = can_follow(hand, game.last_play.as_ref(), game.hand_level);
                    out.push((
                        None,
                        ServerMessage::PlayTurn {
                            seat: *seat,
                            must_lead: *must_lead,
                            last_play,
                            timeout_secs,
                            can_follow: *must_lead || follow_ok,
                        },
                    ));
                    if !*must_lead && !follow_ok {
                        out.push((None, ServerMessage::NoLegalPlay { seat: *seat }));
                    }
                }
                Event::Played { seat, cards, hand } => {
                    out.push((
                        None,
                        ServerMessage::CardPlayed {
                            seat: *seat,
                            cards: cards.clone(),
                            hand_type: hand.ty,
                            counts: std::array::from_fn(|i| game.players[i].hand.len()),
                            reveal_secs,
                        },
                    ));
                }
                Event::Passed { seat } => {
                    out.push((
                        None,
                        ServerMessage::PlayerPass {
                            seat: *seat,
                            reveal_secs,
                        },
                    ));
                }
                Event::PlayerOut { seat, rank } => {
                    out.push((
                        None,
                        ServerMessage::PlayerOut {
                            seat: *seat,
                            rank: *rank,
                        },
                    ));
                }
                Event::HandResult {
                    finish_order,
                    winning_team,
                    level_gain,
                    new_levels,
                    match_over,
                    winner_team,
                } => {
                    let ranks: Vec<FinishRank> = (0..finish_order.len())
                        .map(|i| match i {
                            0 => FinishRank::Banker,
                            1 => FinishRank::Follower,
                            2 => FinishRank::Third,
                            _ => FinishRank::Dweller,
                        })
                        .collect();
                    out.push((
                        None,
                        ServerMessage::HandResult {
                            finish_order: finish_order.clone(),
                            ranks,
                            winning_team: *winning_team,
                            level_gain: *level_gain,
                            new_levels: *new_levels,
                            match_over: *match_over,
                            winner_team: *winner_team,
                            confirm_timeout_secs: CONFIRM_TIMEOUT_SECS,
                            confirmed: [false; 4],
                        },
                    ));
                    if *match_over {
                        if let Some(wt) = winner_team {
                            out.push((
                                None,
                                ServerMessage::MatchOver {
                                    winner_team: *wt,
                                    levels: *new_levels,
                                },
                            ));
                        }
                    }
                }
                Event::ResultConfirmed { seat, confirmed } => {
                    out.push((
                        None,
                        ServerMessage::ResultConfirmed {
                            seat: *seat,
                            confirmed: *confirmed,
                        },
                    ));
                }
                Event::AllConfirmed { match_over } => {
                    out.push((
                        None,
                        ServerMessage::AllConfirmed {
                            match_over: *match_over,
                        },
                    ));
                }
            }
        }
        out
    }

    fn revealing(&self) -> bool {
        self.reveal_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false)
    }

    /// One bot step if current seat is a bot and reveal window has elapsed.
    pub fn bot_actions(&mut self) -> Vec<(Option<Uuid>, ServerMessage)> {
        if self.revealing() {
            return Vec::new();
        }
        let mut all = Vec::new();
        // At most one bot action per tick so reveal delay applies between plays.
        let (phase, current, is_bot) = {
            let Some(game) = &self.game else {
                return all;
            };
            if game.phase != MatchPhase::Playing && game.phase != MatchPhase::Tribute {
                return all;
            }
            let cur = game.current;
            let bot = self.slots[cur].is_bot;
            (game.phase, cur, bot)
        };
        if !is_bot {
            return all;
        }
        let decision = {
            let game = self.game.as_ref().unwrap();
            decide_play(game, current)
        };
        let action = if phase == MatchPhase::Tribute {
            if let Some((card_id, to_seat)) = decision.return_tribute {
                Action::ReturnTribute { card_id, to_seat }
            } else {
                return all;
            }
        } else if decision.pass {
            Action::Pass
        } else if let Some(cards) = decision.play {
            Action::Play {
                card_ids: cards.iter().map(|c| c.id).collect(),
            }
        } else {
            return all;
        };
        match self.apply_action(current, action) {
            Ok(msgs) => all.extend(msgs),
            Err(e) => {
                tracing::warn!("bot action failed seat={current}: {e}");
                if phase == MatchPhase::Playing {
                    if let Ok(msgs) = self.apply_action(current, Action::Pass) {
                        all.extend(msgs);
                    }
                }
            }
        }
        all
    }

    /// Force pass (or lead) when the standard turn timer expires.
    pub fn check_turn_timeout(&mut self) -> Vec<(Option<Uuid>, ServerMessage)> {
        if self.revealing() {
            return Vec::new();
        }
        let Some(deadline) = self.turn_deadline else {
            return Vec::new();
        };
        if Instant::now() < deadline {
            return Vec::new();
        }
        let Some(game) = &self.game else {
            return Vec::new();
        };
        if game.phase != MatchPhase::Playing && game.phase != MatchPhase::Tribute {
            return Vec::new();
        }
        let seat = game.current;
        // Don't timeout bots here — bot tick handles them (but still enforce if stuck).
        let mut out = vec![(None, ServerMessage::TurnTimeout { seat })];

        let action = if game.phase == MatchPhase::Tribute {
            let decision = decide_play(game, seat);
            if let Some((card_id, to_seat)) = decision.return_tribute {
                Action::ReturnTribute { card_id, to_seat }
            } else {
                return out;
            }
        } else if game.last_play.is_none() {
            // Must lead: play smallest legal set
            let decision = decide_play(game, seat);
            if let Some(cards) = decision.play {
                Action::Play {
                    card_ids: cards.iter().map(|c| c.id).collect(),
                }
            } else {
                return out;
            }
        } else {
            Action::Pass
        };

        match self.apply_action(seat, action) {
            Ok(msgs) => {
                out.extend(msgs);
            }
            Err(e) => {
                tracing::warn!("turn timeout action failed seat={seat}: {e}");
                // Clear deadline so we don't loop-spam
                self.turn_deadline = Some(Instant::now() + self.settings.turn_timeout());
            }
        }
        out
    }

    /// Auto-confirm remaining seats when confirm timer expires; deal when all confirmed.
    pub fn check_confirm_timeout(&mut self) -> Vec<(Option<Uuid>, ServerMessage)> {
        let Some(deadline) = self.confirm_deadline else {
            return Vec::new();
        };
        if Instant::now() < deadline {
            // Still waiting — bots auto-confirm immediately so only humans block.
            return self.auto_confirm_bots();
        }
        let mut all = Vec::new();
        // Force-confirm anyone still pending
        for seat in 0..4 {
            let need = {
                let Some(g) = &self.game else {
                    return all;
                };
                matches!(g.phase, MatchPhase::HandOver | MatchPhase::MatchOver)
                    && !g.confirmed[seat]
            };
            if need {
                if let Ok(msgs) = self.apply_action(seat, Action::ConfirmResult) {
                    all.extend(msgs);
                }
            }
        }
        all.extend(self.try_deal_after_confirm());
        all
    }

    fn auto_confirm_bots(&mut self) -> Vec<(Option<Uuid>, ServerMessage)> {
        let mut all = Vec::new();
        for seat in 0..4 {
            let (is_bot, need) = {
                let Some(g) = &self.game else {
                    return all;
                };
                let need = matches!(g.phase, MatchPhase::HandOver | MatchPhase::MatchOver)
                    && !g.confirmed[seat];
                (self.slots[seat].is_bot, need)
            };
            if is_bot && need {
                if let Ok(msgs) = self.apply_action(seat, Action::ConfirmResult) {
                    all.extend(msgs);
                }
            }
        }
        all.extend(self.try_deal_after_confirm());
        all
    }

    /// After AllConfirmed, deal the next hand exactly once.
    pub fn try_deal_after_confirm(&mut self) -> Vec<(Option<Uuid>, ServerMessage)> {
        if !self.pending_next_deal {
            return Vec::new();
        }
        let ok = self
            .game
            .as_ref()
            .map(|g| g.phase == MatchPhase::Idle && g.winner_team.is_none())
            .unwrap_or(false);
        if !ok {
            return Vec::new();
        }
        self.pending_next_deal = false;
        match self.apply_action(0, Action::Deal) {
            Ok(msgs) => msgs,
            Err(e) => {
                tracing::warn!("deal after confirm failed: {e}");
                Vec::new()
            }
        }
    }

    /// Re-party: fill empty human seats with bots after timeout; open seats for join.
    pub fn check_reparty(&mut self) -> Vec<(Option<Uuid>, ServerMessage)> {
        let mut out = Vec::new();
        let Some(deadline) = self.reparty_deadline else {
            return out;
        };
        // Always advertise vacant seats (session gone, not bot)
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.session_id.is_none() && !slot.is_bot {
                // already vacant — ensure reparty window
                let _ = i;
            }
        }
        if Instant::now() < deadline {
            return out;
        }
        // Timeout: bots substitute empty seats so game can continue
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.session_id.is_none() && !slot.is_bot {
                slot.is_bot = true;
                slot.name = format!("机器人{}", i + 1);
                slot.ready = true;
                out.push((
                    None,
                    ServerMessage::SeatTaken {
                        seat: i,
                        info: SeatInfo {
                            seat: i,
                            name: slot.name.clone(),
                            is_bot: true,
                            ready: true,
                            team: team_of(i),
                        },
                    },
                ));
            }
        }
        self.reparty_deadline = None;
        // Auto-confirm bots after fill
        out.extend(self.auto_confirm_bots());
        out
    }

    /// Human left mid-game / between hands → seat vacant for re-party.
    pub fn vacate_seat(&mut self, seat: Seat, reason: &str) -> Vec<(Option<Uuid>, ServerMessage)> {
        if seat >= 4 {
            return Vec::new();
        }
        self.slots[seat].session_id = None;
        self.slots[seat].is_bot = false;
        self.slots[seat].ready = false;
        self.slots[seat].name = format!("空位{}", seat + 1);
        self.reparty_deadline =
            Some(Instant::now() + Duration::from_secs(REPARTY_TIMEOUT_SECS as u64));
        vec![(
            None,
            ServerMessage::SeatOpened {
                seat,
                reason: reason.into(),
            },
        )]
    }

    /// New player takes vacant seat (substitute).
    pub fn take_seat(
        &mut self,
        session: Uuid,
        name: String,
        seat: Seat,
    ) -> Result<Vec<(Option<Uuid>, ServerMessage)>, String> {
        if seat >= 4 {
            return Err("非法座位".into());
        }
        let slot = &mut self.slots[seat];
        if slot.session_id.is_some() {
            return Err("座位已有人".into());
        }
        // Prefer vacant human slots; also allow taking bot seat between hands
        let phase_ok = self.game.as_ref().map(|g| {
            matches!(
                g.phase,
                MatchPhase::Idle | MatchPhase::HandOver | MatchPhase::MatchOver
            )
        });
        if self.game.is_some() && phase_ok == Some(false) {
            return Err("对局中不可换人，请等待本局结束".into());
        }
        slot.session_id = Some(session);
        slot.is_bot = false;
        slot.name = name;
        slot.ready = false;
        let info = SeatInfo {
            seat,
            name: slot.name.clone(),
            is_bot: false,
            ready: false,
            team: team_of(seat),
        };
        Ok(vec![(None, ServerMessage::SeatTaken { seat, info })])
    }
}
