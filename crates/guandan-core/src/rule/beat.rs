//! Hand comparison and move search.

use super::parse::{key_beats, parse_hand, HandType, ParsedHand};
use super::BombTier;
use crate::card::{play_strength, Card, Rank};

/// Whether `new_hand` beats `last_hand` under the current level.
pub fn can_beat(new_hand: &ParsedHand, last_hand: &ParsedHand, level: Rank) -> bool {
    // Joker bomb beats everything
    if new_hand.ty == HandType::JokerBomb {
        return last_hand.ty != HandType::JokerBomb;
    }
    if last_hand.ty == HandType::JokerBomb {
        return false;
    }

    let new_bomb = new_hand.ty.is_bomb();
    let last_bomb = last_hand.ty.is_bomb();

    if new_bomb && !last_bomb {
        return true;
    }
    if !new_bomb && last_bomb {
        return false;
    }

    if new_bomb && last_bomb {
        return bomb_beats(new_hand, last_hand, level);
    }

    // Same non-bomb type required
    if new_hand.ty != last_hand.ty {
        return false;
    }
    // Structural length must match (straight/tube/plate fixed sizes already equal types)
    if new_hand.length != last_hand.length {
        return false;
    }
    key_beats(new_hand.key, last_hand.key, level)
}

fn bomb_beats(new_hand: &ParsedHand, last_hand: &ParsedHand, level: Rank) -> bool {
    let nt = new_hand.ty.bomb_tier(new_hand.length).expect("bomb type");
    let lt = last_hand.ty.bomb_tier(last_hand.length).expect("bomb type");

    if nt != lt {
        return nt > lt;
    }
    match nt {
        BombTier::SixPlus => {
            if new_hand.length != last_hand.length {
                return new_hand.length > last_hand.length;
            }
            key_beats(new_hand.key, last_hand.key, level)
        }
        BombTier::StraightFlush => key_beats(new_hand.key, last_hand.key, level),
        BombTier::Four | BombTier::Five => key_beats(new_hand.key, last_hand.key, level),
        BombTier::JokerBomb => false,
    }
}

/// Find the smallest legal play from `hand` that beats `last`.
/// Returns the cards to play, or None if pass.
pub fn find_smallest_beater(hand: &[Card], last: &ParsedHand, level: Rank) -> Option<Vec<Card>> {
    let mut candidates: Vec<(u8, Vec<Card>, ParsedHand)> = Vec::new();

    // Generate combinations of appropriate sizes
    let sizes = relevant_sizes(last);
    for size in sizes {
        for combo in combinations(hand, size) {
            if let Ok(parsed) = parse_hand(&combo, level) {
                if can_beat(&parsed, last, level) {
                    let score = combo_score(&parsed, level);
                    candidates.push((score, combo, parsed));
                }
            }
        }
    }

    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.len().cmp(&b.1.len())));
    // Prefer non-bomb beaters when available
    if let Some((_, cards, _)) = candidates.iter().find(|(_, _, p)| !p.ty.is_bomb()) {
        return Some(cards.clone());
    }
    candidates.into_iter().next().map(|(_, c, _)| c)
}

/// Smallest reasonable lead from a hand.
pub fn find_smallest_lead(hand: &[Card], level: Rank) -> Option<Vec<Card>> {
    if hand.is_empty() {
        return None;
    }
    // Prefer single lowest card
    let mut sorted = hand.to_vec();
    sorted.sort_by_key(|c| play_strength(c.rank, level));
    // Try single, then pair, then triple of the lowest ranks
    for size in [1usize, 2, 3] {
        if sorted.len() < size {
            continue;
        }
        // group by rank
        let mut by_rank: std::collections::BTreeMap<u8, Vec<Card>> =
            std::collections::BTreeMap::new();
        for &c in &sorted {
            by_rank
                .entry(play_strength(c.rank, level))
                .or_default()
                .push(c);
        }
        for (_str, group) in by_rank {
            if group.len() >= size {
                let combo: Vec<Card> = group.iter().take(size).copied().collect();
                if parse_hand(&combo, level).is_ok() {
                    return Some(combo);
                }
            }
        }
    }
    // Fallback: any single
    Some(vec![sorted[0]])
}

fn combo_score(parsed: &ParsedHand, level: Rank) -> u8 {
    let mut s = play_strength(parsed.key, level);
    if parsed.ty.is_bomb() {
        s = s.saturating_add(50);
    }
    s
}

fn relevant_sizes(last: &ParsedHand) -> Vec<usize> {
    let mut sizes = Vec::new();
    // Same size as last for non-bomb follow
    sizes.push(last.cards.len());
    // Bombs of various sizes
    for n in 4..=10 {
        if !sizes.contains(&n) {
            sizes.push(n);
        }
    }
    // Straight flush is 5
    if !sizes.contains(&5) {
        sizes.push(5);
    }
    sizes
}

/// Generate all combinations of `k` cards from `hand`.
fn combinations(hand: &[Card], k: usize) -> Vec<Vec<Card>> {
    if k == 0 || k > hand.len() {
        return Vec::new();
    }
    // Large hands: rank-grouped + limited raw (no recursion into this path)
    if k >= 4 || hand.len() > 18 {
        return smart_combinations(hand, k);
    }
    raw_combinations(hand, k)
}

/// Rank-aware candidate generation for bombs / larger sets.
fn smart_combinations(hand: &[Card], k: usize) -> Vec<Vec<Card>> {
    let mut out = Vec::new();
    let mut groups: std::collections::HashMap<Rank, Vec<Card>> = std::collections::HashMap::new();
    for &c in hand {
        groups.entry(c.rank).or_default().push(c);
    }

    // Same-rank k-sets (bombs) — use raw only (never recurse via combinations)
    for cards in groups.values() {
        if cards.len() >= k {
            out.extend(raw_combinations(cards, k));
        }
    }

    // Wild + rank bombs: include heart level cards mixed in by raw when hand small enough
    if (4..=8).contains(&k) && hand.len() <= 16 {
        // already covered partially; also try mixtures of wilds with a rank
        let wilds: Vec<Card> = hand
            .iter()
            .copied()
            .filter(|c| c.suit == crate::card::Suit::Heart)
            .collect();
        for (rank, group) in &groups {
            if rank.is_joker() {
                continue;
            }
            let mut pool = group.clone();
            for w in &wilds {
                if !pool.iter().any(|c| c.id == w.id) {
                    pool.push(*w);
                }
            }
            if pool.len() >= k {
                out.extend(raw_combinations(&pool, k));
            }
        }
    }

    if k == 5 && hand.len() <= 18 {
        out.extend(raw_combinations(hand, 5));
    }
    if k == 6 && hand.len() <= 14 {
        out.extend(raw_combinations(hand, 6));
    }

    if k == 4 {
        let jokers: Vec<Card> = hand.iter().copied().filter(|c| c.rank.is_joker()).collect();
        if jokers.len() >= 4 {
            out.extend(raw_combinations(&jokers, 4));
        }
    }

    out
}

fn raw_combinations(hand: &[Card], k: usize) -> Vec<Vec<Card>> {
    let mut out = Vec::new();
    let n = hand.len();
    if k == 0 || k > n {
        return out;
    }
    let mut idx: Vec<usize> = (0..k).collect();
    loop {
        out.push(idx.iter().map(|&i| hand[i]).collect());
        // Advance to next combination; return when exhausted
        let mut i = k;
        loop {
            if i == 0 {
                return out;
            }
            i -= 1;
            if idx[i] < n - k + i {
                idx[i] += 1;
                for j in i + 1..k {
                    idx[j] = idx[j - 1] + 1;
                }
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::cards_from_codes;
    use crate::rule::parse_hand;

    #[test]
    fn higher_pair_beats() {
        let level = Rank::R2;
        let a = parse_hand(&cards_from_codes(&["S3", "H3"]), level).unwrap();
        let b = parse_hand(&cards_from_codes(&["S5", "H5"]), level).unwrap();
        assert!(can_beat(&b, &a, level));
        assert!(!can_beat(&a, &b, level));
    }

    #[test]
    fn bomb_beats_pair() {
        let level = Rank::R2;
        let pair = parse_hand(&cards_from_codes(&["SA", "HA"]), level).unwrap();
        let bomb = parse_hand(&cards_from_codes(&["S4", "H4", "C4", "D4"]), level).unwrap();
        assert!(can_beat(&bomb, &pair, level));
    }

    #[test]
    fn joker_bomb_beats_big_bomb() {
        let level = Rank::R2;
        let bomb = parse_hand(
            &cards_from_codes(&["S4", "H4", "C4", "D4", "S4", "H4"]),
            level,
        );
        // only 4 of each rank max from 2 decks for non-level... 8 of a rank possible
        let mut eight = cards_from_codes(&["S4", "H4", "C4", "D4"]);
        eight.extend(cards_from_codes(&["S4", "H4", "C4", "D4"]));
        for (i, c) in eight.iter_mut().enumerate() {
            c.id = i as u8;
        }
        let big = parse_hand(&eight, level).unwrap();
        let jokers = vec![
            Card {
                id: 100,
                suit: crate::card::Suit::Joker,
                rank: Rank::BlackJoker,
            },
            Card {
                id: 101,
                suit: crate::card::Suit::Joker,
                rank: Rank::BlackJoker,
            },
            Card {
                id: 102,
                suit: crate::card::Suit::Joker,
                rank: Rank::RedJoker,
            },
            Card {
                id: 103,
                suit: crate::card::Suit::Joker,
                rank: Rank::RedJoker,
            },
        ];
        let jb = parse_hand(&jokers, level).unwrap();
        assert!(can_beat(&jb, &big, level));
        let _ = bomb;
    }

    #[test]
    fn find_beater_simple() {
        let level = Rank::R7;
        let last = parse_hand(&cards_from_codes(&["S3"]), level).unwrap();
        let hand = cards_from_codes(&["S2", "H5", "C5", "SA"]);
        let beat = find_smallest_beater(&hand, &last, level).unwrap();
        assert_eq!(beat.len(), 1);
    }
}
