//! Match finite-state machine: multi-hand Guandan with levels and tribute.

use crate::card::{deal_four, sort_hand, Card, Rank};
use crate::rule::{can_beat, parse_hand, ParsedHand, RuleError};
use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Seat index 0..3. Teams: 0+2 vs 1+3.
pub type Seat = usize;

pub fn team_of(seat: Seat) -> TeamId {
    if seat.is_multiple_of(2) {
        TeamId::A
    } else {
        TeamId::B
    }
}

pub fn partner(seat: Seat) -> Seat {
    (seat + 2) % 4
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamId {
    A = 0,
    B = 1,
}

impl TeamId {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn opponent(self) -> TeamId {
        match self {
            TeamId::A => TeamId::B,
            TeamId::B => TeamId::A,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchPhase {
    /// Between hands / before first deal
    Idle,
    /// Waiting for tribute return (Banker picks card)
    Tribute,
    /// Playing tricks
    Playing,
    /// Hand finished; levels updated
    HandOver,
    /// Match finished
    MatchOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishRank {
    Banker = 1,   // 上游
    Follower = 2, // 二游
    Third = 3,    // 三游
    Dweller = 4,  // 下游
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub hand: Vec<Card>,
    pub finished: Option<FinishRank>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TributeState {
    /// Dweller seat(s) who must pay / have paid.
    pub payers: Vec<Seat>,
    /// Cards paid by each payer (parallel to payers after payment applied).
    pub paid: Vec<(Seat, Card)>,
    /// Who must return tribute: Banker (and Follower if double).
    pub returners: Vec<Seat>,
    /// Pending returns left.
    pub pending_returns: Vec<Seat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
    pub phase: MatchPhase,
    pub players: [PlayerState; 4],
    /// Team levels (face rank 2..A), start at 2.
    pub team_levels: [Rank; 2],
    /// Current hand's level cards rank.
    pub hand_level: Rank,
    /// Whose team sets the hand level (Banker team of previous hand); seat of previous banker or None first hand.
    pub hand_number: u32,
    pub current: Seat,
    pub last_play: Option<ParsedHand>,
    pub last_player: Option<Seat>,
    pub passes: u8,
    /// Finish order as seats go out.
    pub finish_order: Vec<Seat>,
    pub tribute: Option<TributeState>,
    /// Previous hand dweller seats (for Ace win restriction and tribute).
    pub prev_dwellers: Vec<Seat>,
    pub prev_banker: Option<Seat>,
    pub winner_team: Option<TeamId>,
    /// Lead seat for this hand.
    pub lead_seat: Seat,
    /// After HandOver / MatchOver: who confirmed the result (all 4 required before next deal).
    pub confirmed: [bool; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Start / deal next hand.
    Deal,
    /// Banker (or returner) returns a card to a payer.
    ReturnTribute {
        card_id: u8,
        to_seat: Seat,
    },
    Play {
        card_ids: Vec<u8>,
    },
    Pass,
    /// Acknowledge hand/match result (rank board). All 4 must confirm before Deal.
    ConfirmResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Dealt {
        /// Hands only filled for the seat itself when serializing privately; engine keeps all.
        hands_len: [usize; 4],
        hand_level: Rank,
        lead: Seat,
    },
    TributePaid {
        from: Seat,
        card: Card,
        to: Seat,
    },
    AntiTribute {
        dwellers: Vec<Seat>,
    },
    TributeReturned {
        from: Seat,
        card: Card,
        to: Seat,
    },
    Turn {
        seat: Seat,
        must_lead: bool,
    },
    Played {
        seat: Seat,
        cards: Vec<Card>,
        hand: ParsedHand,
    },
    Passed {
        seat: Seat,
    },
    PlayerOut {
        seat: Seat,
        rank: FinishRank,
    },
    HandResult {
        finish_order: Vec<Seat>,
        winning_team: TeamId,
        level_gain: u8,
        new_levels: [Rank; 2],
        match_over: bool,
        winner_team: Option<TeamId>,
    },
    /// Seat acknowledged the result board (上游…下游).
    ResultConfirmed {
        seat: Seat,
        confirmed: [bool; 4],
    },
    /// All four confirmed — server may deal next hand.
    AllConfirmed {
        match_over: bool,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MatchError {
    #[error("非法阶段")]
    BadPhase,
    #[error("不是你的回合")]
    NotYourTurn,
    #[error("牌不在手中")]
    CardsNotInHand,
    #[error("牌型无效: {0}")]
    InvalidHand(#[from] RuleError),
    #[error("压不过上家")]
    CannotBeat,
    #[error("必须出牌")]
    MustPlay,
    #[error("不能空过")]
    CannotPass,
    #[error("进贡回牌非法")]
    BadTributeReturn,
    #[error("游戏已结束")]
    MatchOver,
    #[error("已确认过")]
    AlreadyConfirmed,
    #[error("需要先确认本局结果")]
    NeedConfirm,
}

impl Match {
    pub fn new() -> Self {
        Self {
            phase: MatchPhase::Idle,
            players: std::array::from_fn(|_| PlayerState {
                hand: Vec::new(),
                finished: None,
            }),
            team_levels: [Rank::R2, Rank::R2],
            hand_level: Rank::R2,
            hand_number: 0,
            current: 0,
            last_play: None,
            last_player: None,
            passes: 0,
            finish_order: Vec::new(),
            tribute: None,
            prev_dwellers: Vec::new(),
            prev_banker: None,
            winner_team: None,
            lead_seat: 0,
            confirmed: [false; 4],
        }
    }

    pub fn team_level(&self, team: TeamId) -> Rank {
        self.team_levels[team.index()]
    }

    /// Anti-tribute: the dweller(s) collectively hold both red jokers
    /// (a single dweller holding both, or each double-dweller holding one).
    fn anti_tribute(&self, dwellers: &[Seat]) -> bool {
        let red_jokers: usize = dwellers
            .iter()
            .map(|&d| {
                self.players[d]
                    .hand
                    .iter()
                    .filter(|c| c.rank == Rank::RedJoker)
                    .count()
            })
            .sum();
        red_jokers >= 2
    }

    pub fn apply<R: Rng + ?Sized>(
        &mut self,
        seat: Seat,
        action: Action,
        rng: &mut R,
    ) -> Result<Vec<Event>, MatchError> {
        match action {
            Action::ConfirmResult => return self.confirm_result(seat),
            Action::Deal => {}
            _ if self.phase == MatchPhase::MatchOver => return Err(MatchError::MatchOver),
            _ => {}
        }
        match action {
            Action::Deal => self.deal(rng),
            Action::ReturnTribute { card_id, to_seat } => {
                self.return_tribute(seat, card_id, to_seat)
            }
            Action::Play { card_ids } => self.play(seat, &card_ids),
            Action::Pass => self.pass(seat),
            Action::ConfirmResult => unreachable!(),
        }
    }

    /// Acknowledge hand result ranks. When all four seats confirm, emits AllConfirmed.
    pub fn confirm_result(&mut self, seat: Seat) -> Result<Vec<Event>, MatchError> {
        if self.phase != MatchPhase::HandOver && self.phase != MatchPhase::MatchOver {
            return Err(MatchError::BadPhase);
        }
        if seat >= 4 {
            return Err(MatchError::NotYourTurn);
        }
        if self.confirmed[seat] {
            return Err(MatchError::AlreadyConfirmed);
        }
        self.confirmed[seat] = true;
        let mut events = vec![Event::ResultConfirmed {
            seat,
            confirmed: self.confirmed,
        }];
        if self.confirmed.iter().all(|&c| c) {
            let match_over = self.phase == MatchPhase::MatchOver;
            events.push(Event::AllConfirmed { match_over });
            if !match_over {
                // Ready for next deal
                self.phase = MatchPhase::Idle;
            }
        }
        Ok(events)
    }

    fn deal<R: Rng + ?Sized>(&mut self, rng: &mut R) -> Result<Vec<Event>, MatchError> {
        // Only after Idle (includes post-AllConfirmed). Never skip confirm board.
        if self.phase == MatchPhase::HandOver {
            return Err(MatchError::NeedConfirm);
        }
        if self.phase != MatchPhase::Idle {
            return Err(MatchError::BadPhase);
        }
        self.confirmed = [false; 4];
        self.hand_number += 1;
        // Hand level = level of team that has Banker from previous hand
        if let Some(banker) = self.prev_banker {
            self.hand_level = self.team_levels[team_of(banker).index()];
        } else {
            self.hand_level = Rank::R2;
        }

        let hands = deal_four(rng);
        for (i, mut h) in hands.into_iter().enumerate() {
            sort_hand(&mut h, self.hand_level);
            self.players[i] = PlayerState {
                hand: h,
                finished: None,
            };
        }
        self.finish_order.clear();
        self.last_play = None;
        self.last_player = None;
        self.passes = 0;
        self.tribute = None;

        let mut events = Vec::new();

        // Determine lead seat
        if self.hand_number == 1 {
            self.lead_seat = rng.random_range(0..4);
            self.current = self.lead_seat;
            self.phase = MatchPhase::Playing;
            events.push(Event::Dealt {
                hands_len: [27; 4],
                hand_level: self.hand_level,
                lead: self.lead_seat,
            });
            events.push(Event::Turn {
                seat: self.current,
                must_lead: true,
            });
            return Ok(events);
        }

        // Tribute phase from hand 2+
        let dwellers = self.prev_dwellers.clone();
        let banker = self.prev_banker.expect("banker after first hand");

        // Anti-tribute: the dweller(s) collectively hold both red jokers
        // (a single dweller holding both, or each double-dweller holding one).
        let anti = self.anti_tribute(&dwellers);

        if anti {
            self.lead_seat = banker;
            self.current = banker;
            self.phase = MatchPhase::Playing;
            events.push(Event::Dealt {
                hands_len: [27; 4],
                hand_level: self.hand_level,
                lead: banker,
            });
            events.push(Event::AntiTribute {
                dwellers: dwellers.clone(),
            });
            events.push(Event::Turn {
                seat: self.current,
                must_lead: true,
            });
            return Ok(events);
        }

        // Auto-pay tribute: each dweller gives highest non-heart-level card.
        // Payments are applied now but reported after Dealt.
        let mut paid: Vec<(Seat, Card)> = Vec::new();
        let mut returners = Vec::new();
        let mut paid_events = Vec::new();
        if dwellers.len() == 1 {
            let d = dwellers[0];
            let card = take_tribute_card(&mut self.players[d].hand, self.hand_level)
                .expect("dweller has cards");
            self.players[banker].hand.push(card);
            sort_hand(&mut self.players[banker].hand, self.hand_level);
            paid.push((d, card));
            returners.push(banker);
            paid_events.push(Event::TributePaid {
                from: d,
                card,
                to: banker,
            });
            // Single dweller leads once the return is done.
            self.lead_seat = d;
        } else if dwellers.len() == 2 {
            // Double dweller: both pay to winning team (banker + follower)
            let follower = self.finish_order.get(1).copied().unwrap_or(partner(banker));
            // Pay highest first — banker gets higher tribute
            let mut tributes: Vec<(Seat, Card)> = dwellers
                .iter()
                .map(|&d| {
                    let c = peek_tribute_card(&self.players[d].hand, self.hand_level).unwrap();
                    (d, c)
                })
                .collect();
            tributes.sort_by(|a, b| {
                crate::card::play_strength(b.1.rank, self.hand_level)
                    .cmp(&crate::card::play_strength(a.1.rank, self.hand_level))
            });
            // Higher → banker, lower → follower
            let receivers = [banker, follower];
            for (i, (d, _)) in tributes.iter().enumerate() {
                let card = take_tribute_card(&mut self.players[*d].hand, self.hand_level).unwrap();
                let to = receivers[i.min(1)];
                self.players[to].hand.push(card);
                sort_hand(&mut self.players[to].hand, self.hand_level);
                paid.push((*d, card));
                paid_events.push(Event::TributePaid { from: *d, card, to });
            }
            returners.push(banker);
            returners.push(follower);
            // Lead = who paid higher tribute
            self.lead_seat = tributes[0].0;
        }

        events.push(Event::Dealt {
            hands_len: [27; 4],
            hand_level: self.hand_level,
            lead: self.lead_seat,
        });
        events.extend(paid_events);

        self.tribute = Some(TributeState {
            payers: dwellers,
            paid: paid.clone(),
            returners: returners.clone(),
            pending_returns: returners,
        });
        self.phase = MatchPhase::Tribute;
        // Wait for returns; current = first returner. Emit a Turn so clients
        // (and the server deadline) know a tribute return is expected.
        self.current = self.tribute.as_ref().unwrap().pending_returns[0];
        events.push(Event::Turn {
            seat: self.current,
            must_lead: false,
        });
        Ok(events)
    }

    fn return_tribute(
        &mut self,
        seat: Seat,
        card_id: u8,
        to_seat: Seat,
    ) -> Result<Vec<Event>, MatchError> {
        if self.phase != MatchPhase::Tribute {
            return Err(MatchError::BadPhase);
        }
        let trib = self.tribute.as_mut().ok_or(MatchError::BadPhase)?;
        if trib.pending_returns.first().copied() != Some(seat) {
            return Err(MatchError::NotYourTurn);
        }
        // Return card must be rank ≤ 10 (face 2..10). If the returner holds no
        // such card at all (pathological), accept any card so the hand can
        // proceed instead of wedging the state machine.
        let has_small = self.players[seat].hand.iter().any(|c| {
            matches!(
                c.rank,
                Rank::R2
                    | Rank::R3
                    | Rank::R4
                    | Rank::R5
                    | Rank::R6
                    | Rank::R7
                    | Rank::R8
                    | Rank::R9
                    | Rank::R10
            )
        });
        let pos = self.players[seat]
            .hand
            .iter()
            .position(|c| c.id == card_id)
            .ok_or(MatchError::CardsNotInHand)?;
        let card = self.players[seat].hand[pos];
        let ok_rank = matches!(
            card.rank,
            Rank::R2
                | Rank::R3
                | Rank::R4
                | Rank::R5
                | Rank::R6
                | Rank::R7
                | Rank::R8
                | Rank::R9
                | Rank::R10
        );
        if !ok_rank && has_small {
            return Err(MatchError::BadTributeReturn);
        }
        // Must return to a payer who paid this returner — simple: to_seat must be a payer
        if !trib.payers.contains(&to_seat) {
            return Err(MatchError::BadTributeReturn);
        }

        self.players[seat].hand.remove(pos);
        self.players[to_seat].hand.push(card);
        sort_hand(&mut self.players[to_seat].hand, self.hand_level);
        sort_hand(&mut self.players[seat].hand, self.hand_level);

        trib.pending_returns.remove(0);
        let mut events = vec![Event::TributeReturned {
            from: seat,
            card,
            to: to_seat,
        }];

        if trib.pending_returns.is_empty() {
            // Lead seat was fixed at deal time (highest tribute payer).
            self.current = self.lead_seat;
            self.phase = MatchPhase::Playing;
            self.tribute = None;
            events.push(Event::Turn {
                seat: self.current,
                must_lead: true,
            });
        } else {
            self.current = trib.pending_returns[0];
            events.push(Event::Turn {
                seat: self.current,
                must_lead: false,
            });
        }
        Ok(events)
    }

    fn play(&mut self, seat: Seat, card_ids: &[u8]) -> Result<Vec<Event>, MatchError> {
        if self.phase != MatchPhase::Playing {
            return Err(MatchError::BadPhase);
        }
        if seat != self.current {
            return Err(MatchError::NotYourTurn);
        }
        if self.players[seat].finished.is_some() {
            return Err(MatchError::NotYourTurn);
        }

        // Validate everything before mutating: ids must be unique and present,
        // the hand must parse, and it must beat the last play. Only then
        // remove the cards — a rejected play must never change state.
        let cards = collect_cards(&self.players[seat].hand, card_ids)?;
        let parsed = parse_hand(&cards, self.hand_level)?;

        if let Some(ref last) = self.last_play {
            if !can_beat(&parsed, last, self.hand_level) {
                return Err(MatchError::CannotBeat);
            }
        }

        remove_cards(&mut self.players[seat].hand, card_ids);

        let mut events = vec![Event::Played {
            seat,
            cards: cards.clone(),
            hand: parsed.clone(),
        }];

        self.last_play = Some(parsed);
        self.last_player = Some(seat);
        self.passes = 0;

        // Check if player went out
        if self.players[seat].hand.is_empty() {
            let rank = finish_rank(self.finish_order.len());
            self.players[seat].finished = Some(rank);
            self.finish_order.push(seat);
            events.push(Event::PlayerOut { seat, rank });

            // Hand ends when 3 players out (4th is automatic dweller) or both of one team?
            // Actually play continues until we know finish order for leveling:
            // stop when 3 are out (last is dweller) OR when partner of banker is determined enough.
            // Always continue until 3 outs for simplicity.
            if self.finish_order.len() >= 3 {
                let last = (0..4).find(|s| !self.finish_order.contains(s)).unwrap();
                self.players[last].finished = Some(FinishRank::Dweller);
                self.finish_order.push(last);
                events.push(Event::PlayerOut {
                    seat: last,
                    rank: FinishRank::Dweller,
                });
                events.extend(self.finish_hand());
                return Ok(events);
            }
        }

        self.current = self.next_active_seat(seat);
        // If current player is out — should not happen
        let must_lead = self.last_play.is_none();
        events.push(Event::Turn {
            seat: self.current,
            must_lead,
        });
        Ok(events)
    }

    fn pass(&mut self, seat: Seat) -> Result<Vec<Event>, MatchError> {
        if self.phase != MatchPhase::Playing {
            return Err(MatchError::BadPhase);
        }
        if seat != self.current {
            return Err(MatchError::NotYourTurn);
        }
        if self.last_play.is_none() {
            return Err(MatchError::MustPlay);
        }

        let mut events = vec![Event::Passed { seat }];
        self.passes += 1;

        // Count active players remaining in trick
        let active = (0..4)
            .filter(|&s| self.players[s].finished.is_none())
            .count();
        // Trick ends when everyone but the last player has passed. If the last
        // player already went out, every remaining active player must pass.
        let leader_finished = self
            .last_player
            .is_some_and(|lp| self.players[lp].finished.is_some());
        let need = (if leader_finished {
            active
        } else {
            active.saturating_sub(1)
        })
        .max(1) as u8;

        if self.passes >= need {
            // Trick ends; last player leads (or partner if out)
            let leader = self.last_player.unwrap();
            self.last_play = None;
            self.passes = 0;
            if self.players[leader].finished.is_some() {
                self.current = partner(leader);
                if self.players[self.current].finished.is_some() {
                    self.current = self.next_active_seat(leader);
                }
            } else {
                self.current = leader;
            }
            self.last_player = None;
            events.push(Event::Turn {
                seat: self.current,
                must_lead: true,
            });
        } else {
            self.current = self.next_active_seat(seat);
            events.push(Event::Turn {
                seat: self.current,
                must_lead: false,
            });
        }
        Ok(events)
    }

    fn next_active_seat(&self, from: Seat) -> Seat {
        let mut s = (from + 1) % 4;
        for _ in 0..4 {
            if self.players[s].finished.is_none() {
                return s;
            }
            s = (s + 1) % 4;
        }
        from
    }

    fn finish_hand(&mut self) -> Vec<Event> {
        let banker = self.finish_order[0];
        let winning_team = team_of(banker);
        let partner_seat = partner(banker);
        let partner_place = self
            .finish_order
            .iter()
            .position(|&s| s == partner_seat)
            .unwrap_or(3);

        let level_gain: u8 = match partner_place {
            1 => 3, // Banker + Follower
            2 => 2, // Banker + Third
            _ => 1, // Banker + Dweller
        };

        let ti = winning_team.index();
        let old = self.team_levels[ti];
        // Ace win rule: at level A the team must take 头游 with the partner not
        // 下游 — i.e. finish 1st&2nd or 1st&3rd. 1st&4th keeps them at A
        // (advance saturates at A) and the hand passes to the next deal.
        let was_at_ace = old == Rank::RA;
        let ace_converted = was_at_ace && partner_place <= 2;

        self.team_levels[ti] = old.advance(level_gain);

        let mut match_over = false;
        let mut winner_team = None;
        if ace_converted {
            match_over = true;
            winner_team = Some(winning_team);
            self.winner_team = winner_team;
            self.phase = MatchPhase::MatchOver;
        } else {
            self.phase = MatchPhase::HandOver;
        }
        self.confirmed = [false; 4];

        self.prev_banker = Some(banker);
        // Dwellers for next hand's tribute: both losers on a double-down
        // (banker's partner also 2nd), otherwise just the last-place seat.
        if partner_place == 1 {
            self.prev_dwellers = self
                .finish_order
                .iter()
                .copied()
                .filter(|&s| team_of(s) != winning_team)
                .collect();
        } else {
            self.prev_dwellers = vec![*self.finish_order.last().unwrap()];
        }

        vec![Event::HandResult {
            finish_order: self.finish_order.clone(),
            winning_team,
            level_gain,
            new_levels: self.team_levels,
            match_over,
            winner_team,
        }]
    }
}

impl Default for Match {
    fn default() -> Self {
        Self::new()
    }
}

fn finish_rank(outs_before: usize) -> FinishRank {
    match outs_before {
        0 => FinishRank::Banker,
        1 => FinishRank::Follower,
        2 => FinishRank::Third,
        _ => FinishRank::Dweller,
    }
}

/// Collect cards by id without mutating the hand. Fails if any id is
/// duplicated or absent.
fn collect_cards(hand: &[Card], ids: &[u8]) -> Result<Vec<Card>, MatchError> {
    let mut seen = std::collections::HashSet::with_capacity(ids.len());
    let mut cards = Vec::with_capacity(ids.len());
    for &id in ids {
        if !seen.insert(id) {
            return Err(MatchError::CardsNotInHand);
        }
        let card = hand
            .iter()
            .copied()
            .find(|c| c.id == id)
            .ok_or(MatchError::CardsNotInHand)?;
        cards.push(card);
    }
    Ok(cards)
}

/// Remove cards by id. Call only after `collect_cards` has validated the ids.
fn remove_cards(hand: &mut Vec<Card>, ids: &[u8]) {
    for &id in ids {
        if let Some(pos) = hand.iter().position(|c| c.id == id) {
            hand.remove(pos);
        }
    }
}

fn peek_tribute_card(hand: &[Card], level: Rank) -> Option<Card> {
    hand.iter()
        .copied()
        .filter(|c| !(c.suit == crate::card::Suit::Heart && c.rank == level))
        .max_by_key(|c| crate::card::play_strength(c.rank, level))
}

fn take_tribute_card(hand: &mut Vec<Card>, level: Rank) -> Option<Card> {
    let card = peek_tribute_card(hand, level)?;
    let pos = hand.iter().position(|c| c.id == card.id)?;
    Some(hand.remove(pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn deal_first_hand() {
        let mut m = Match::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let events = m.apply(0, Action::Deal, &mut rng).unwrap();
        assert!(matches!(m.phase, MatchPhase::Playing));
        assert!(events.iter().any(|e| matches!(e, Event::Dealt { .. })));
        for p in &m.players {
            assert_eq!(p.hand.len(), 27);
        }
    }

    #[test]
    fn play_and_pass_trick() {
        let mut m = Match::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        m.apply(0, Action::Deal, &mut rng).unwrap();
        let seat = m.current;
        let card_id = m.players[seat].hand[0].id;
        m.apply(
            seat,
            Action::Play {
                card_ids: vec![card_id],
            },
            &mut rng,
        )
        .unwrap();
        assert!(m.last_play.is_some());
        // others pass
        for _ in 0..3 {
            let s = m.current;
            if m.last_play.is_none() {
                break;
            }
            let _ = m.apply(s, Action::Pass, &mut rng);
        }
    }

    #[test]
    fn teams() {
        assert_eq!(team_of(0), TeamId::A);
        assert_eq!(team_of(2), TeamId::A);
        assert_eq!(team_of(1), TeamId::B);
        assert_eq!(partner(0), 2);
    }

    /// A rejected play must never change the hand or the turn.
    #[test]
    fn failed_play_keeps_cards() {
        let mut m = Match::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(11);
        m.apply(0, Action::Deal, &mut rng).unwrap();
        let seat = m.current;

        // Duplicate id → error, hand untouched
        let id = m.players[seat].hand[0].id;
        let err = m
            .apply(
                seat,
                Action::Play {
                    card_ids: vec![id, id],
                },
                &mut rng,
            )
            .unwrap_err();
        assert_eq!(err, MatchError::CardsNotInHand);
        assert_eq!(m.players[seat].hand.len(), 27);

        // Bogus id after a valid one → error, valid card NOT removed
        let err = m
            .apply(
                seat,
                Action::Play {
                    card_ids: vec![id, 250],
                },
                &mut rng,
            )
            .unwrap_err();
        assert_eq!(err, MatchError::CardsNotInHand);
        assert_eq!(m.players[seat].hand.len(), 27);
        assert!(m.players[seat].hand.iter().any(|c| c.id == id));

        // Invalid combo (two different ranks) → parse error, cards kept
        let c1 = m.players[seat].hand[0];
        let c2 = m.players[seat]
            .hand
            .iter()
            .find(|c| c.rank != c1.rank)
            .copied()
            .unwrap();
        let err = m
            .apply(
                seat,
                Action::Play {
                    card_ids: vec![c1.id, c2.id],
                },
                &mut rng,
            )
            .unwrap_err();
        assert!(matches!(err, MatchError::InvalidHand(_)));
        assert_eq!(m.players[seat].hand.len(), 27);
        assert!(m.last_play.is_none());
        assert_eq!(m.current, seat);

        // CannotBeat → cards returned, still the same turn
        let joker_seat = (0..4)
            .find(|&s| m.players[s].hand.iter().any(|c| c.rank == Rank::RedJoker))
            .unwrap();
        m.current = joker_seat;
        let joker = m.players[joker_seat]
            .hand
            .iter()
            .find(|c| c.rank == Rank::RedJoker)
            .copied()
            .unwrap();
        m.apply(
            joker_seat,
            Action::Play {
                card_ids: vec![joker.id],
            },
            &mut rng,
        )
        .unwrap();
        let next = m.current;
        let weak = m.players[next].hand[0]; // sorted low → high
        let err = m
            .apply(
                next,
                Action::Play {
                    card_ids: vec![weak.id],
                },
                &mut rng,
            )
            .unwrap_err();
        assert_eq!(err, MatchError::CannotBeat);
        assert_eq!(m.players[next].hand.len(), 27);
        assert_eq!(m.current, next);
    }

    /// Tribute (single dweller): Dealt carries the real lead, and Turn events
    /// drive the returner — without them human clients soft-lock.
    #[test]
    fn tribute_emits_turn_events() {
        let mut m = Match::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(5);
        m.apply(0, Action::Deal, &mut rng).unwrap();
        // Simulate hand 1 result: banker 0, single dweller 3
        m.prev_banker = Some(0);
        m.prev_dwellers = vec![3];
        m.phase = MatchPhase::Idle;

        let events = m.apply(0, Action::Deal, &mut rng).unwrap();
        if matches!(m.phase, MatchPhase::Playing) {
            // Anti-tribute rolled by this seed — not what this test covers
            panic!("seed produced anti-tribute");
        }
        assert!(matches!(m.phase, MatchPhase::Tribute));
        let lead = events
            .iter()
            .find_map(|e| match e {
                Event::Dealt { lead, .. } => Some(*lead),
                _ => None,
            })
            .unwrap();
        assert_eq!(lead, 3, "single dweller leads after tribute");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Turn { seat: 0, .. })),
            "returner must be told it is their turn"
        );
        assert_eq!(m.current, 0);

        // Return the lowest ≤10 card to the payer
        let card = m.players[0]
            .hand
            .iter()
            .filter(|c| (c.rank as u8) >= 2 && (c.rank as u8) <= 10)
            .min_by_key(|c| c.rank as u8)
            .copied()
            .expect("banker has a small card");
        let events = m
            .apply(
                0,
                Action::ReturnTribute {
                    card_id: card.id,
                    to_seat: 3,
                },
                &mut rng,
            )
            .unwrap();
        assert!(matches!(m.phase, MatchPhase::Playing));
        assert_eq!(m.current, 3);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Turn {
                seat: 3,
                must_lead: true
            }
        )));
    }

    /// Double dweller: both returners get Turn events in order.
    #[test]
    fn double_dweller_tribute_turns() {
        let mut m = Match::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(6);
        m.apply(0, Action::Deal, &mut rng).unwrap();
        m.prev_banker = Some(0);
        m.prev_dwellers = vec![1, 3]; // double-down: team B both dwell
        m.phase = MatchPhase::Idle;

        let events = m.apply(0, Action::Deal, &mut rng).unwrap();
        if matches!(m.phase, MatchPhase::Playing) {
            panic!("seed produced anti-tribute");
        }
        assert!(matches!(m.phase, MatchPhase::Tribute));
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::Turn { seat: 0, .. })));

        // Banker returns first, then follower (partner of banker, seat 2)
        let card0 = m.players[0]
            .hand
            .iter()
            .filter(|c| (c.rank as u8) >= 2 && (c.rank as u8) <= 10)
            .min_by_key(|c| c.rank as u8)
            .copied()
            .unwrap();
        let events = m
            .apply(
                0,
                Action::ReturnTribute {
                    card_id: card0.id,
                    to_seat: 1,
                },
                &mut rng,
            )
            .unwrap();
        assert!(matches!(m.phase, MatchPhase::Tribute));
        assert_eq!(m.current, 2);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Turn { seat: 2, .. })),
            "second returner must be told"
        );

        let card2 = m.players[2]
            .hand
            .iter()
            .filter(|c| (c.rank as u8) >= 2 && (c.rank as u8) <= 10)
            .min_by_key(|c| c.rank as u8)
            .copied()
            .unwrap();
        let events = m
            .apply(
                2,
                Action::ReturnTribute {
                    card_id: card2.id,
                    to_seat: 3,
                },
                &mut rng,
            )
            .unwrap();
        assert!(matches!(m.phase, MatchPhase::Playing));
        // Lead = the payer of the higher tribute
        assert_eq!(m.current, m.lead_seat);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Turn {
                must_lead: true,
                ..
            }
        )));
    }

    #[test]
    fn anti_tribute_collective_jokers() {
        let mut m = Match::new();
        // Single dweller holding both red jokers
        m.players[3].hand = crate::card::cards_from_codes(&["RJ", "RJ", "S5"]);
        assert!(m.anti_tribute(&[3]));
        // Single dweller holding only one
        m.players[3].hand = crate::card::cards_from_codes(&["RJ", "S5", "C6"]);
        assert!(!m.anti_tribute(&[3]));
        // Double dwellers with one red joker each
        m.players[1].hand = crate::card::cards_from_codes(&["RJ", "S5"]);
        m.players[3].hand = crate::card::cards_from_codes(&["RJ", "C6"]);
        assert!(m.anti_tribute(&[1, 3]));
        // Double dwellers with only one red joker between them
        m.players[1].hand = crate::card::cards_from_codes(&["RJ", "S5"]);
        m.players[3].hand = crate::card::cards_from_codes(&["S6", "C6"]);
        assert!(!m.anti_tribute(&[1, 3]));
    }

    /// When the trick winner goes out on their play, every remaining active
    /// player must pass before the trick closes.
    #[test]
    fn pass_count_when_leader_went_out() {
        let mut m = Match::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(9);
        m.apply(0, Action::Deal, &mut rng).unwrap();

        // Reduce current seat to one card and play it → goes out
        let a = m.current;
        let card = m.players[a].hand[0];
        m.players[a].hand = vec![card];
        m.apply(
            a,
            Action::Play {
                card_ids: vec![card.id],
            },
            &mut rng,
        )
        .unwrap();
        assert!(m.players[a].finished.is_some());

        // Two passes are NOT enough — the third active player must get a turn
        let b = m.current;
        m.apply(b, Action::Pass, &mut rng).unwrap();
        let c = m.current;
        m.apply(c, Action::Pass, &mut rng).unwrap();
        assert!(
            m.last_play.is_some(),
            "trick closed early; a player was skipped"
        );

        let d = m.current;
        m.apply(d, Action::Pass, &mut rng).unwrap();
        assert!(m.last_play.is_none(), "trick should close after all pass");
        // Lead passes to the out player's partner (接风)
        assert_eq!(m.current, partner(a));
    }

    /// At level A the match is only won when the partner is not the dweller
    /// of this hand (1st&2nd or 1st&3rd); 1st&4th stays at A.
    #[test]
    fn ace_win_condition() {
        // 1st & 3rd at level A → match won
        let mut m = Match::new();
        m.team_levels = [Rank::RA, Rank::R2];
        m.finish_order = vec![0, 1, 2, 3];
        let events = m.finish_hand();
        assert!(matches!(m.phase, MatchPhase::MatchOver));
        let (over, winner) = events
            .iter()
            .find_map(|e| match e {
                Event::HandResult {
                    match_over,
                    winner_team,
                    ..
                } => Some((*match_over, *winner_team)),
                _ => None,
            })
            .unwrap();
        assert!(over);
        assert_eq!(winner, Some(TeamId::A));

        // 1st & 4th at level A → no win, stay at A
        let mut m = Match::new();
        m.team_levels = [Rank::RA, Rank::R2];
        m.finish_order = vec![0, 1, 3, 2]; // partner seat 2 is dweller
        let events = m.finish_hand();
        assert!(matches!(m.phase, MatchPhase::HandOver));
        assert_eq!(m.team_levels[0], Rank::RA);
        let over = events
            .iter()
            .find_map(|e| match e {
                Event::HandResult { match_over, .. } => Some(*match_over),
                _ => None,
            })
            .unwrap();
        assert!(!over);

        // Below A: 1st & 2nd → +3 levels (K → A) but no match win yet
        let mut m = Match::new();
        m.team_levels = [Rank::RK, Rank::R2];
        m.finish_order = vec![0, 2, 1, 3];
        let events = m.finish_hand();
        assert!(matches!(m.phase, MatchPhase::HandOver));
        assert_eq!(m.team_levels[0], Rank::RA);
        let over = events
            .iter()
            .find_map(|e| match e {
                Event::HandResult { match_over, .. } => Some(*match_over),
                _ => None,
            })
            .unwrap();
        assert!(!over);
    }
}
