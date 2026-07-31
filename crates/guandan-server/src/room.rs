//! Room and in-game match hosting.

use guandan_bot::decide_play;
use guandan_core::{team_of, Action, Event, Match, MatchPhase, Seat};
use guandan_protocol::{PublicPlay, SeatInfo, ServerMessage};
use rand::rngs::StdRng;
use rand::SeedableRng;
use uuid::Uuid;

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
}

impl Room {
    pub fn new(id: String, _practice: bool) -> Self {
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
        self.broadcast_events(&events)
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
        Ok(self.broadcast_events(&events))
    }

    /// Messages to send: (Some(session) for private, None for all human seats).
    pub fn broadcast_events(&self, events: &[Event]) -> Vec<(Option<Uuid>, ServerMessage)> {
        let mut out = Vec::new();
        let game = match &self.game {
            Some(g) => g,
            None => return out,
        };

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
                    // Also game start snapshot once
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
                    out.push((
                        None,
                        ServerMessage::PlayTurn {
                            seat: *seat,
                            must_lead: *must_lead,
                            last_play,
                        },
                    ));
                }
                Event::Played { seat, cards, hand } => {
                    out.push((
                        None,
                        ServerMessage::CardPlayed {
                            seat: *seat,
                            cards: cards.clone(),
                            hand_type: hand.ty,
                            counts: std::array::from_fn(|i| game.players[i].hand.len()),
                        },
                    ));
                }
                Event::Passed { seat } => {
                    out.push((None, ServerMessage::PlayerPass { seat: *seat }));
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
                    out.push((
                        None,
                        ServerMessage::HandResult {
                            finish_order: finish_order.clone(),
                            winning_team: *winning_team,
                            level_gain: *level_gain,
                            new_levels: *new_levels,
                            match_over: *match_over,
                            winner_team: *winner_team,
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
            }
        }
        out
    }

    pub fn bot_actions(&mut self) -> Vec<(Option<Uuid>, ServerMessage)> {
        let mut all = Vec::new();
        // Cap steps so one tick cannot burn the stack / hang the server.
        for _ in 0..32 {
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
                break;
            }
            let decision = {
                let game = self.game.as_ref().unwrap();
                decide_play(game, current)
            };
            let action = if phase == MatchPhase::Tribute {
                if let Some((card_id, to_seat)) = decision.return_tribute {
                    Action::ReturnTribute { card_id, to_seat }
                } else {
                    break;
                }
            } else if decision.pass {
                Action::Pass
            } else if let Some(cards) = decision.play {
                Action::Play {
                    card_ids: cards.iter().map(|c| c.id).collect(),
                }
            } else {
                break;
            };
            match self.apply_action(current, action) {
                Ok(msgs) => {
                    all.extend(msgs);
                    // continue if next is also bot
                }
                Err(e) => {
                    tracing::warn!("bot action failed seat={current}: {e}");
                    // force pass if possible
                    if phase == MatchPhase::Playing {
                        if let Ok(msgs) = self.apply_action(current, Action::Pass) {
                            all.extend(msgs);
                        }
                    }
                    break;
                }
            }
        }
        all
    }

    /// After hand over, auto-deal next hand for continuous play.
    pub fn maybe_continue(&mut self) -> Vec<(Option<Uuid>, ServerMessage)> {
        let need = matches!(
            self.game.as_ref().map(|g| g.phase),
            Some(MatchPhase::HandOver)
        );
        if !need {
            return Vec::new();
        }
        let game = self.game.as_mut().unwrap();
        match game.apply(0, Action::Deal, &mut self.rng) {
            Ok(events) => self.broadcast_events(&events),
            Err(e) => {
                tracing::warn!("auto deal failed: {e}");
                Vec::new()
            }
        }
    }
}
