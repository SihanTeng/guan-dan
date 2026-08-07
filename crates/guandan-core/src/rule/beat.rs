//! Hand comparison and move search.

use super::parse::{
    key_beats, key_beats_natural, parse_hand, straight_shapes, HandType, ParsedHand,
};
use super::BombTier;
use crate::card::{play_strength, Card, Rank, Suit};

/// Whether the player has any legal follow/bomb against `last` (or can always lead).
pub fn can_follow(hand: &[Card], last: Option<&ParsedHand>, level: Rank) -> bool {
    match last {
        None => !hand.is_empty(),
        Some(lp) => find_smallest_beater(hand, lp, level).is_some(),
    }
}

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
    // Sequences compare by natural face order; the level card participates
    // at its natural value.
    if matches!(new_hand.ty, HandType::Straight | HandType::StraightFlush) {
        return key_beats_natural(new_hand.key, last_hand.key);
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
        BombTier::StraightFlush => key_beats_natural(new_hand.key, last_hand.key),
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
        for combo in combinations(hand, size, level) {
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
    // Sequences score by natural face order, matching can_beat.
    let mut s = match parsed.ty {
        HandType::Straight | HandType::StraightFlush => parsed.key as u8,
        _ => play_strength(parsed.key, level),
    };
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
fn combinations(hand: &[Card], k: usize, level: Rank) -> Vec<Vec<Card>> {
    if k == 0 || k > hand.len() {
        return Vec::new();
    }
    // Small sets are cheap to enumerate fully (C(27,3) = 2925 max).
    if k <= 3 {
        return raw_combinations(hand, k);
    }
    smart_combinations(hand, k, level)
}

/// Rank-aware candidate generation for bombs and structured combos. Stays
/// complete at any hand size by generating shapes directly instead of
/// brute-force enumeration.
fn smart_combinations(hand: &[Card], k: usize, level: Rank) -> Vec<Vec<Card>> {
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

    // Wild + rank bombs: mix heart level cards into each rank group
    let wilds: Vec<Card> = hand.iter().copied().filter(|c| c.is_wild(level)).collect();
    if (4..=8).contains(&k) && !wilds.is_empty() {
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

    if k == 4 {
        let jokers: Vec<Card> = hand.iter().copied().filter(|c| c.rank.is_joker()).collect();
        if jokers.len() >= 4 {
            out.extend(raw_combinations(&jokers, 4));
        }
    }

    if k == 5 {
        // Full house shapes
        for &t in &Rank::FACES {
            for &p in &Rank::FACES {
                if t != p {
                    if let Some(c) = shaped_combo(hand, level, &[(t, 3), (p, 2)], None) {
                        out.push(c);
                    }
                }
            }
        }
        // Straight and straight-flush shapes
        for shape in straight_shapes() {
            let need: Vec<(Rank, usize)> = shape.iter().map(|&r| (r, 1)).collect();
            if let Some(c) = shaped_combo(hand, level, &need, None) {
                out.push(c);
            }
            for suit in [Suit::Spade, Suit::Heart, Suit::Club, Suit::Diamond] {
                if let Some(c) = shaped_combo(hand, level, &need, Some(suit)) {
                    out.push(c);
                }
            }
        }
    }

    if k == 6 {
        // Tubes (3 consecutive pairs) and plates (2 consecutive triples)
        for start in 0..=10u8 {
            if let [Some(r0), Some(r1), Some(r2)] =
                [start, start + 1, start + 2].map(Rank::from_face_index)
            {
                if let Some(c) = shaped_combo(hand, level, &[(r0, 2), (r1, 2), (r2, 2)], None) {
                    out.push(c);
                }
            }
        }
        for start in 0..=11u8 {
            if let [Some(r0), Some(r1)] = [start, start + 1].map(Rank::from_face_index) {
                if let Some(c) = shaped_combo(hand, level, &[(r0, 3), (r1, 3)], None) {
                    out.push(c);
                }
            }
        }
    }

    out
}

/// Build one combo covering `need` (rank -> count) from `hand`: natural
/// (non-wild) cards of each rank first, wilds filling the gaps. When `suit`
/// is given, natural cards must share it (straight flush search).
fn shaped_combo(
    hand: &[Card],
    level: Rank,
    need: &[(Rank, usize)],
    suit: Option<Suit>,
) -> Option<Vec<Card>> {
    let mut wilds: Vec<Card> = hand.iter().copied().filter(|c| c.is_wild(level)).collect();
    let mut combo = Vec::new();
    for &(rank, count) in need {
        let fixed: Vec<Card> = hand
            .iter()
            .copied()
            .filter(|c| c.rank == rank && !c.is_wild(level))
            .filter(|c| match suit {
                Some(s) => c.suit == s,
                None => true,
            })
            .take(count)
            .collect();
        let missing = count - fixed.len();
        if wilds.len() < missing {
            return None;
        }
        combo.extend(fixed);
        combo.extend(wilds.drain(..missing));
    }
    Some(combo)
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

    /// Straights compare by natural face order: at level 6 the straight
    /// 2-3-4-5-6 must NOT beat 10-J-Q-K-A (the level card plays at its
    /// natural value inside sequences).
    #[test]
    fn straight_compares_by_natural_rank() {
        let level = Rank::R6;
        let low = parse_hand(&cards_from_codes(&["S2", "H3", "C4", "D5", "S6"]), level).unwrap();
        let high = parse_hand(&cards_from_codes(&["ST", "HJ", "CQ", "DK", "SA"]), level).unwrap();
        assert_eq!(low.ty, HandType::Straight);
        assert_eq!(high.ty, HandType::Straight);
        assert!(!can_beat(&low, &high, level));
        assert!(can_beat(&high, &low, level));

        // …while the level card still outranks A in plain sets
        let sixes = parse_hand(&cards_from_codes(&["S6", "C6"]), level).unwrap();
        let aces = parse_hand(&cards_from_codes(&["SA", "HA"]), level).unwrap();
        assert!(can_beat(&sixes, &aces, level));
    }

    #[test]
    fn straight_flush_compares_by_natural_rank() {
        let level = Rank::R6;
        let low = parse_hand(&cards_from_codes(&["S2", "S3", "S4", "S5", "S6"]), level).unwrap();
        let high = parse_hand(&cards_from_codes(&["HT", "HJ", "HQ", "HK", "HA"]), level).unwrap();
        assert_eq!(low.ty, HandType::StraightFlush);
        assert_eq!(high.ty, HandType::StraightFlush);
        assert!(!can_beat(&low, &high, level));
        assert!(can_beat(&high, &low, level));
    }

    /// With 19+ cards, straights/fulls/tubes used to be invisible to the
    /// beater search. The shape-directed generator must find them.
    #[test]
    fn finds_straight_beater_in_large_hand() {
        let level = Rank::R2;
        let last = parse_hand(&cards_from_codes(&["H3", "D4", "C5", "H6", "D7"]), level).unwrap();
        assert_eq!(last.ty, HandType::Straight);
        // 21-card hand: junk pairs plus the 4-5-6-7-8 straight
        let hand = cards_from_codes(&[
            "S2", "C2", "S3", "C3", "S9", "C9", "SJ", "CJ", "SQ", "CQ", "SK", "CK", "SA", "CA",
            "ST", "CT", "S4", "D5", "S6", "D7", "S8",
        ]);
        assert_eq!(hand.len(), 21);
        let beater = find_smallest_beater(&hand, &last, level).expect("a straight beater exists");
        let p = parse_hand(&beater, level).unwrap();
        assert_eq!(p.ty, HandType::Straight);
        assert_eq!(p.key, Rank::R8);
        assert!(can_beat(&p, &last, level));
        assert!(can_follow(&hand, Some(&last), level));
    }
}
