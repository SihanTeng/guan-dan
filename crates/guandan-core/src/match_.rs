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
        }
    }

    pub fn team_level(&self, team: TeamId) -> Rank {
        self.team_levels[team.index()]
    }

    pub fn apply<R: Rng + ?Sized>(
        &mut self,
        seat: Seat,
        action: Action,
        rng: &mut R,
    ) -> Result<Vec<Event>, MatchError> {
        if self.phase == MatchPhase::MatchOver {
            return Err(MatchError::MatchOver);
        }
        match action {
            Action::Deal => self.deal(rng),
            Action::ReturnTribute { card_id, to_seat } => {
                self.return_tribute(seat, card_id, to_seat)
            }
            Action::Play { card_ids } => self.play(seat, &card_ids),
            Action::Pass => self.pass(seat),
        }
    }

    fn deal<R: Rng + ?Sized>(&mut self, rng: &mut R) -> Result<Vec<Event>, MatchError> {
        if self.phase != MatchPhase::Idle && self.phase != MatchPhase::HandOver {
            return Err(MatchError::BadPhase);
        }
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

        // Anti-tribute: any dweller holds both red jokers
        let anti = dwellers.iter().any(|&d| {
            self.players[d]
                .hand
                .iter()
                .filter(|c| c.rank == Rank::RedJoker)
                .count()
                >= 2
        });

        events.push(Event::Dealt {
            hands_len: [27; 4],
            hand_level: self.hand_level,
            lead: 0, // updated below
        });

        if anti {
            events.push(Event::AntiTribute {
                dwellers: dwellers.clone(),
            });
            self.lead_seat = banker;
            self.current = banker;
            self.phase = MatchPhase::Playing;
            events.push(Event::Turn {
                seat: self.current,
                must_lead: true,
            });
            return Ok(events);
        }

        // Auto-pay tribute: each dweller gives highest non-heart-level card
        let mut paid: Vec<(Seat, Card)> = Vec::new();
        let mut returners = Vec::new();
        if dwellers.len() == 1 {
            let d = dwellers[0];
            let card = take_tribute_card(&mut self.players[d].hand, self.hand_level)
                .expect("dweller has cards");
            self.players[banker].hand.push(card);
            sort_hand(&mut self.players[banker].hand, self.hand_level);
            paid.push((d, card));
            returners.push(banker);
            events.push(Event::TributePaid {
                from: d,
                card,
                to: banker,
            });
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
                events.push(Event::TributePaid { from: *d, card, to });
            }
            returners.push(banker);
            returners.push(follower);
            // Lead = who paid higher tribute
            self.lead_seat = tributes[0].0;
        }

        self.tribute = Some(TributeState {
            payers: dwellers,
            paid: paid.clone(),
            returners: returners.clone(),
            pending_returns: returners,
        });
        self.phase = MatchPhase::Tribute;
        // Wait for returns; current = first returner
        self.current = self.tribute.as_ref().unwrap().pending_returns[0];
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
        // Return card must be rank ≤ 10 (face 2..10)
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
        if !ok_rank {
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
            // Start play: lead is highest tribute payer (or single dweller)
            if trib.paid.len() == 1 {
                self.lead_seat = trib.paid[0].0;
            }
            // else lead_seat already set in double case
            self.current = self.lead_seat;
            self.phase = MatchPhase::Playing;
            self.tribute = None;
            events.push(Event::Turn {
                seat: self.current,
                must_lead: true,
            });
        } else {
            self.current = trib.pending_returns[0];
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

        let cards = take_cards(&mut self.players[seat].hand, card_ids)?;
        let parsed = parse_hand(&cards, self.hand_level)?;

        if let Some(ref last) = self.last_play {
            if !can_beat(&parsed, last, self.hand_level) {
                // put cards back
                self.players[seat].hand.extend(cards);
                sort_hand(&mut self.players[seat].hand, self.hand_level);
                return Err(MatchError::CannotBeat);
            }
        }

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
        // Need (active - 1) passes to end trick, or 3 passes classic
        let need = (active.saturating_sub(1)).max(1) as u8;

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
        // Ace win restriction
        let was_at_ace = old == Rank::RA;
        let partner_was_prev_dweller =
            self.prev_dwellers.contains(&partner_seat) || self.prev_dwellers.contains(&banker);
        // Wikipedia: cannot win on a hand where one of the partners is the Dweller from previous hand
        let blocked = was_at_ace && partner_was_prev_dweller;

        self.team_levels[ti] = old.advance(level_gain);

        let mut match_over = false;
        let mut winner_team = None;
        if was_at_ace && !blocked {
            // Winning a hand while already at Ace wins the match
            match_over = true;
            winner_team = Some(winning_team);
            self.winner_team = winner_team;
            self.phase = MatchPhase::MatchOver;
        } else if self.team_levels[ti] == Rank::RA && old == Rank::RA {
            // stayed at ace but blocked — continue
            self.phase = MatchPhase::HandOver;
        } else {
            self.phase = MatchPhase::HandOver;
        }

        // If we advanced TO ace and won this hand from below ace, need another hand to claim
        // (only win when already at ace before the hand). Correct per Wikipedia.

        self.prev_banker = Some(banker);
        self.prev_dwellers = self
            .finish_order
            .iter()
            .copied()
            .filter(|&s| team_of(s) != winning_team)
            .collect();
        // Actually dwellers are the losing team members still not out first — standard:
        // Dweller = last place; Double-Dweller = last two if both losing?
        // Losing team seats that finished 3rd/4th or just last:
        // For tribute: the Dweller(s) — if Double-Dweller, both of last two when partner of banker is follower?
        // Simpler: prev_dwellers = seats with FinishRank::Dweller, and if partner of banker is Follower
        // then losing team both are "dwellers" for double? Wikipedia: Double-Dweller when both of last two.
        if partner_place == 1 {
            // double down — both losers are dwellers
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

fn take_cards(hand: &mut Vec<Card>, ids: &[u8]) -> Result<Vec<Card>, MatchError> {
    let mut taken = Vec::with_capacity(ids.len());
    for &id in ids {
        let pos = hand
            .iter()
            .position(|c| c.id == id)
            .ok_or(MatchError::CardsNotInHand)?;
        taken.push(hand.remove(pos));
    }
    Ok(taken)
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
}
