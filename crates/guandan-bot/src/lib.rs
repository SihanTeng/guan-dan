//! Rule-based heuristic bot — always produces legal moves.

use guandan_core::{find_smallest_beater, find_smallest_lead, Card, Match, ParsedHand, Rank, Seat};

#[derive(Debug, Clone)]
pub struct BotDecision {
    pub play: Option<Vec<Card>>,
    pub pass: bool,
    pub return_tribute: Option<(u8, Seat)>,
}

/// Decide a play for `seat` given current match state.
pub fn decide_play(m: &Match, seat: Seat) -> BotDecision {
    if m.phase == guandan_core::MatchPhase::Tribute {
        return decide_tribute(m, seat);
    }
    if m.phase != guandan_core::MatchPhase::Playing {
        return BotDecision {
            play: None,
            pass: false,
            return_tribute: None,
        };
    }
    if m.current != seat {
        return BotDecision {
            play: None,
            pass: false,
            return_tribute: None,
        };
    }

    let hand = &m.players[seat].hand;
    let level = m.hand_level;

    if let Some(ref last) = m.last_play {
        // Partner is winning the trick — pass if partner is last player
        if let Some(lp) = m.last_player {
            if is_partner(seat, lp) && !last.ty.is_bomb() {
                // Soft pass when partner leads non-bomb
                if should_pass_to_partner(hand, last, level) {
                    return BotDecision {
                        play: None,
                        pass: true,
                        return_tribute: None,
                    };
                }
            }
        }
        match find_smallest_beater(hand, last, level) {
            Some(cards) => BotDecision {
                play: Some(cards),
                pass: false,
                return_tribute: None,
            },
            None => BotDecision {
                play: None,
                pass: true,
                return_tribute: None,
            },
        }
    } else {
        let cards = find_smallest_lead(hand, level);
        BotDecision {
            play: cards,
            pass: false,
            return_tribute: None,
        }
    }
}

fn decide_tribute(m: &Match, seat: Seat) -> BotDecision {
    let Some(ref trib) = m.tribute else {
        return BotDecision {
            play: None,
            pass: false,
            return_tribute: None,
        };
    };
    if trib.pending_returns.first().copied() != Some(seat) {
        return BotDecision {
            play: None,
            pass: false,
            return_tribute: None,
        };
    }
    // Return lowest card rank ≤ 10 to first payer that paid us
    let hand = &m.players[seat].hand;
    let mut candidates: Vec<&Card> = hand
        .iter()
        .filter(|c| (c.rank as u8) >= 2 && (c.rank as u8) <= 10)
        .collect();
    candidates.sort_by_key(|c| c.rank as u8);
    let Some(card) = candidates.first() else {
        // fallback any lowest
        let card = hand.iter().min_by_key(|c| c.rank as u8).unwrap();
        let to = trib.payers[0];
        return BotDecision {
            play: None,
            pass: false,
            return_tribute: Some((card.id, to)),
        };
    };
    // Prefer payer who paid this seat
    let to = trib
        .paid
        .iter()
        .find(|(_, _c)| {
            // who paid to us: paid.to was seat — we stored (from, card) and events had to
            // Our TributeState.paid is (from_dweller, card); receiver is banker/follower
            true
        })
        .map(|(d, _)| *d)
        .unwrap_or(trib.payers[0]);
    // Map: if we are banker/follower, return to corresponding payer
    let to = if trib.paid.len() == 1 {
        trib.paid[0].0
    } else {
        // match returner order to payer order roughly
        let idx = trib.returners.iter().position(|&s| s == seat).unwrap_or(0);
        trib.paid.get(idx).map(|(d, _)| *d).unwrap_or(to)
    };
    let _ = card;
    BotDecision {
        play: None,
        pass: false,
        return_tribute: Some((candidates[0].id, to)),
    }
}

fn is_partner(a: Seat, b: Seat) -> bool {
    a.is_multiple_of(2) == b.is_multiple_of(2)
}

fn should_pass_to_partner(hand: &[Card], last: &ParsedHand, level: Rank) -> bool {
    // Pass if we would need a bomb or very high card
    match find_smallest_beater(hand, last, level) {
        None => true,
        Some(cards) => {
            if let Ok(p) = guandan_core::parse_hand(&cards, level) {
                p.ty.is_bomb()
            } else {
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guandan_core::{Action, MatchPhase};
    use rand::SeedableRng;

    #[test]
    fn bot_leads_legally() {
        let mut m = Match::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        m.apply(0, Action::Deal, &mut rng).unwrap();
        assert_eq!(m.phase, MatchPhase::Playing);
        let seat = m.current;
        let d = decide_play(&m, seat);
        assert!(d.play.is_some());
        let cards = d.play.unwrap();
        assert!(guandan_core::parse_hand(&cards, m.hand_level).is_ok());
    }
}
