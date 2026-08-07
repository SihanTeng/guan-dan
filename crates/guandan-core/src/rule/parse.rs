//! Parse a multiset of cards into a Guandan hand type (with wild support).

use super::{face_ranks, split_wilds};
use crate::card::{play_strength, Card, Rank, Suit};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandType {
    Single,
    Pair,
    Triple,
    FullHouse,
    Straight,
    Tube,
    Plate,
    Bomb,
    StraightFlush,
    JokerBomb,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedHand {
    pub ty: HandType,
    /// Key rank used for same-type comparison (uses actual rank; compare via play_strength).
    pub key: Rank,
    /// For bombs: card count; for straight/tube/plate: structural length; else 1.
    pub length: usize,
    pub cards: Vec<Card>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuleError {
    #[error("不能出空牌")]
    Empty,
    #[error("不支持的牌型")]
    InvalidHand,
    #[error("牌不在手牌中")]
    CardsNotInHand,
}

/// Parse cards into a hand under the current level.
///
/// Wild (heart level) cards may substitute any non-joker rank when forming
/// combinations. Priority when multiple interpretations exist:
/// joker bomb → longer n-bombs → straight flush → shorter n-bombs → other types.
pub fn parse_hand(cards: &[Card], level: Rank) -> Result<ParsedHand, RuleError> {
    if cards.is_empty() {
        return Err(RuleError::Empty);
    }

    let mut cards = cards.to_vec();
    // Stable order for determinism
    cards.sort_by_key(|c| c.id);

    // Try bomb interpretations first (including wilds), then non-bombs.
    if let Some(h) = try_joker_bomb(&cards) {
        return Ok(h);
    }
    if let Some(h) = try_n_bomb(&cards, level) {
        return Ok(h);
    }
    if let Some(h) = try_straight_flush(&cards, level) {
        return Ok(h);
    }

    // Non-bomb types
    match cards.len() {
        1 => parse_single(&cards, level),
        2 => parse_pair(&cards, level),
        3 => parse_triple(&cards, level),
        5 => parse_five(&cards, level),
        6 => parse_six(&cards, level),
        _ => Err(RuleError::InvalidHand),
    }
}

fn make(ty: HandType, key: Rank, length: usize, cards: &[Card]) -> ParsedHand {
    ParsedHand {
        ty,
        key,
        length,
        cards: cards.to_vec(),
    }
}

fn try_joker_bomb(cards: &[Card]) -> Option<ParsedHand> {
    if cards.len() == 4
        && cards
            .iter()
            .all(|c| c.rank == Rank::BlackJoker || c.rank == Rank::RedJoker)
    {
        let black = cards.iter().filter(|c| c.rank == Rank::BlackJoker).count();
        let red = cards.iter().filter(|c| c.rank == Rank::RedJoker).count();
        if black == 2 && red == 2 {
            return Some(make(HandType::JokerBomb, Rank::RedJoker, 4, cards));
        }
    }
    None
}

/// N-of-a-kind bomb (4+), possibly using wilds. Not jokers mixed with faces.
fn try_n_bomb(cards: &[Card], level: Rank) -> Option<ParsedHand> {
    if cards.len() < 4 {
        return None;
    }
    // All jokers handled elsewhere
    if cards.iter().all(|c| c.rank.is_joker()) {
        return None;
    }
    // Cannot include jokers in normal bombs
    if cards.iter().any(|c| c.rank.is_joker()) {
        return None;
    }

    let (fixed, wilds) = split_wilds(cards, level);
    let wild_n = wilds.len();

    // Count fixed by rank
    let mut counts = [0u8; 15]; // index by rank as u8, 2..14
    for c in &fixed {
        counts[c.rank as usize] += 1;
    }
    let distinct: Vec<Rank> = Rank::FACES
        .iter()
        .copied()
        .filter(|&r| counts[r as usize] > 0)
        .collect();

    if distinct.len() > 1 {
        // Wilds must collapse everything to one rank — only if all fixed are same OR empty
        return None;
    }

    let key = if distinct.is_empty() {
        // All wilds: they form a bomb of the level rank (as wilds of that rank)
        level
    } else {
        distinct[0]
    };

    let total = fixed.len() + wild_n;
    if total < 4 {
        return None;
    }
    // All cards must be interpretable as `key`
    if distinct.len() == 1 && distinct[0] != key {
        return None;
    }
    Some(make(HandType::Bomb, key, total, cards))
}

fn try_straight_flush(cards: &[Card], level: Rank) -> Option<ParsedHand> {
    if cards.len() != 5 {
        return None;
    }
    if cards.iter().any(|c| c.rank.is_joker()) {
        return None;
    }

    let (fixed, wilds) = split_wilds(cards, level);
    // Determine suit: all non-wild must share a suit; wilds (hearts) only match heart SF
    // unless we treat wild as any suit — official: wild can be any card except joker,
    // so wild can complete a straight flush of another suit.
    let suit = if fixed.is_empty() {
        Suit::Heart // pure wild SF as hearts
    } else {
        let s = fixed[0].suit;
        if fixed.iter().any(|c| c.suit != s) {
            return None;
        }
        s
    };

    // Try all straight shapes, strongest first; wilds fill missing ranks.
    for shape in straight_shapes().into_iter().rev() {
        if straight_shape_fits(&fixed, wilds.len(), &shape) {
            let key = shape_high_rank(&shape);
            return Some(make(HandType::StraightFlush, key, 5, cards));
        }
    }
    let _ = suit; // suit validated for non-wild consistency
    None
}

fn parse_single(cards: &[Card], _level: Rank) -> Result<ParsedHand, RuleError> {
    // As a single, wild is just a level card
    Ok(make(HandType::Single, cards[0].rank, 1, cards))
}

fn parse_pair(cards: &[Card], level: Rank) -> Result<ParsedHand, RuleError> {
    let (fixed, wilds) = split_wilds(cards, level);
    if wilds.len() == 2 {
        // Two wilds = pair of level
        return Ok(make(HandType::Pair, level, 1, cards));
    }
    if wilds.len() == 1 && fixed.len() == 1 {
        if fixed[0].rank.is_joker() {
            return Err(RuleError::InvalidHand);
        }
        return Ok(make(HandType::Pair, fixed[0].rank, 1, cards));
    }
    // No wilds
    if fixed.len() == 2 {
        let a = fixed[0].rank;
        let b = fixed[1].rank;
        if a == b {
            // Joker pair must be same color (same rank already means both black or both red)
            return Ok(make(HandType::Pair, a, 1, cards));
        }
    }
    Err(RuleError::InvalidHand)
}

fn parse_triple(cards: &[Card], level: Rank) -> Result<ParsedHand, RuleError> {
    if cards.iter().any(|c| c.rank.is_joker()) && !cards.iter().all(|c| c.is_wild(level)) {
        // Jokers cannot form triples with faces (except wild is heart face)
        if cards.iter().any(|c| c.rank.is_joker()) {
            return Err(RuleError::InvalidHand);
        }
    }
    let (fixed, wilds) = split_wilds(cards, level);
    if fixed.iter().any(|c| c.rank.is_joker()) {
        return Err(RuleError::InvalidHand);
    }
    if wilds.len() == 3 {
        return Ok(make(HandType::Triple, level, 1, cards));
    }
    let mut counts: std::collections::BTreeMap<Rank, usize> = std::collections::BTreeMap::new();
    for c in &fixed {
        *counts.entry(c.rank).or_default() += 1;
    }
    if counts.len() > 1 {
        return Err(RuleError::InvalidHand);
    }
    if counts.is_empty() {
        return Err(RuleError::InvalidHand);
    }
    let (rank, n) = counts.iter().next().unwrap();
    if n + wilds.len() == 3 {
        return Ok(make(HandType::Triple, *rank, 1, cards));
    }
    Err(RuleError::InvalidHand)
}

fn parse_five(cards: &[Card], level: Rank) -> Result<ParsedHand, RuleError> {
    // Full house or straight (SF already tried)
    if let Some(h) = try_full_house(cards, level) {
        return Ok(h);
    }
    if let Some(h) = try_straight(cards, level) {
        return Ok(h);
    }
    Err(RuleError::InvalidHand)
}

fn parse_six(cards: &[Card], level: Rank) -> Result<ParsedHand, RuleError> {
    // Plate preferred over tube if both somehow match (they rarely do)
    if let Some(h) = try_plate(cards, level) {
        return Ok(h);
    }
    if let Some(h) = try_tube(cards, level) {
        return Ok(h);
    }
    Err(RuleError::InvalidHand)
}

fn try_full_house(cards: &[Card], level: Rank) -> Option<ParsedHand> {
    if cards.len() != 5 {
        return None;
    }
    if cards.iter().any(|c| c.rank.is_joker() && !c.is_wild(level)) {
        // real jokers can't be in full house
        if cards
            .iter()
            .any(|c| matches!(c.rank, Rank::BlackJoker | Rank::RedJoker))
        {
            return None;
        }
    }
    // Enumerate which rank is the triple (key) and which is the pair;
    // strongest triple first so wild hands get their best interpretation.
    for &triple_rank in face_ranks().iter().rev() {
        for &pair_rank in face_ranks().iter().rev() {
            if triple_rank == pair_rank {
                continue;
            }
            if can_form_counts(cards, level, &[(triple_rank, 3), (pair_rank, 2)]) {
                return Some(make(HandType::FullHouse, triple_rank, 1, cards));
            }
        }
    }
    None
}

fn try_straight(cards: &[Card], level: Rank) -> Option<ParsedHand> {
    if cards.len() != 5 {
        return None;
    }
    if cards
        .iter()
        .any(|c| matches!(c.rank, Rank::BlackJoker | Rank::RedJoker))
    {
        return None;
    }
    let (fixed, wilds) = split_wilds(cards, level);
    // In straights, wilds are free; non-wild level cards count as their natural face.
    // Strongest shape first so wild hands get their best interpretation.
    for shape in straight_shapes().into_iter().rev() {
        if straight_shape_fits(&fixed, wilds.len(), &shape) {
            let key = shape_high_rank(&shape);
            return Some(make(HandType::Straight, key, 5, cards));
        }
    }
    None
}

fn try_tube(cards: &[Card], level: Rank) -> Option<ParsedHand> {
    // Three consecutive pairs; level rank cannot appear as its natural identity
    if cards.len() != 6 {
        return None;
    }
    if cards
        .iter()
        .any(|c| matches!(c.rank, Rank::BlackJoker | Rank::RedJoker))
    {
        return None;
    }
    for start in 0..=10u8 {
        // three consecutive face indices start, start+1, start+2
        let r0 = Rank::from_face_index(start)?;
        let r1 = Rank::from_face_index(start + 1)?;
        let r2 = Rank::from_face_index(start + 2)?;
        // Level cards break natural consecutive for tubes
        if r0 == level || r1 == level || r2 == level {
            continue;
        }
        if can_form_counts(cards, level, &[(r0, 2), (r1, 2), (r2, 2)]) {
            return Some(make(HandType::Tube, r2, 3, cards));
        }
    }
    None
}

fn try_plate(cards: &[Card], level: Rank) -> Option<ParsedHand> {
    // Two consecutive triples
    if cards.len() != 6 {
        return None;
    }
    if cards
        .iter()
        .any(|c| matches!(c.rank, Rank::BlackJoker | Rank::RedJoker))
    {
        return None;
    }
    for start in 0..=11u8 {
        let r0 = Rank::from_face_index(start)?;
        let r1 = Rank::from_face_index(start + 1)?;
        if r0 == level || r1 == level {
            continue;
        }
        if can_form_counts(cards, level, &[(r0, 3), (r1, 3)]) {
            return Some(make(HandType::Plate, r1, 2, cards));
        }
    }
    None
}

/// Whether cards (with wilds) can supply the required rank counts.
fn can_form_counts(cards: &[Card], level: Rank, need: &[(Rank, usize)]) -> bool {
    let (fixed, wilds) = split_wilds(cards, level);
    let mut have = std::collections::HashMap::<Rank, usize>::new();
    for c in &fixed {
        if matches!(c.rank, Rank::BlackJoker | Rank::RedJoker) {
            return false;
        }
        // Non-wild level cards count as their face rank for combination forming
        // when used as that rank in bombs/pairs/etc.
        *have.entry(c.rank).or_default() += 1;
    }
    let mut wild_left = wilds.len() as i32;
    for &(rank, n) in need {
        let h = *have.get(&rank).unwrap_or(&0);
        if h > n {
            return false; // extra cards of this rank not allowed
        }
        wild_left -= (n - h) as i32;
        if wild_left < 0 {
            return false;
        }
    }
    // All fixed cards must be used
    let mut needed_fixed = 0usize;
    for &(rank, n) in need {
        let h = *have.get(&rank).unwrap_or(&0);
        needed_fixed += h.min(n);
    }
    if needed_fixed != fixed.len() {
        return false;
    }
    // All wilds used
    wild_left == 0
}

/// Straight shapes as lists of 5 face ranks (A can be low), ascending by strength.
pub(crate) fn straight_shapes() -> Vec<Vec<Rank>> {
    let mut shapes = Vec::new();
    // A-2-3-4-5
    shapes.push(vec![Rank::RA, Rank::R2, Rank::R3, Rank::R4, Rank::R5]);
    // 2-3-4-5-6 through 10-J-Q-K-A
    for start in 0..=8u8 {
        let mut s = Vec::new();
        for k in 0..5u8 {
            s.push(Rank::from_face_index(start + k).unwrap());
        }
        shapes.push(s);
    }
    shapes
}

fn shape_high_rank(shape: &[Rank]) -> Rank {
    // For A2345, high is 5; for others max face in non-A-low sense
    if shape.contains(&Rank::RA)
        && shape.contains(&Rank::R2)
        && shape.contains(&Rank::R3)
        && shape.contains(&Rank::R4)
        && shape.contains(&Rank::R5)
    {
        return Rank::R5;
    }
    *shape.iter().max_by_key(|r| **r as u8).unwrap()
}

fn straight_shape_fits(fixed: &[Card], wild_n: usize, shape: &[Rank]) -> bool {
    // fixed cards must each match a unique shape slot by rank; suit already checked
    let mut need: std::collections::HashMap<Rank, usize> = std::collections::HashMap::new();
    for &r in shape {
        *need.entry(r).or_default() += 1;
    }
    for c in fixed {
        let e = need.get_mut(&c.rank);
        match e {
            Some(n) if *n > 0 => *n -= 1,
            _ => return false,
        }
    }
    let missing: usize = need.values().sum();
    missing == wild_n
}

/// Compare key ranks under level for same hand type.
pub fn key_beats(new_key: Rank, old_key: Rank, level: Rank) -> bool {
    play_strength(new_key, level) > play_strength(old_key, level)
}

/// Compare key ranks by natural face order. Used for straights and straight
/// flushes, where the level card participates at its natural value.
pub fn key_beats_natural(new_key: Rank, old_key: Rank) -> bool {
    (new_key as u8) > (old_key as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::cards_from_codes;

    fn level() -> Rank {
        Rank::R2
    }

    #[test]
    fn single_pair_triple() {
        let s = cards_from_codes(&["S3"]);
        let h = parse_hand(&s, level()).unwrap();
        assert_eq!(h.ty, HandType::Single);

        let p = cards_from_codes(&["S3", "H3"]);
        let h = parse_hand(&p, level()).unwrap();
        assert_eq!(h.ty, HandType::Pair);

        let t = cards_from_codes(&["S4", "H4", "C4"]);
        let h = parse_hand(&t, level()).unwrap();
        assert_eq!(h.ty, HandType::Triple);
    }

    #[test]
    fn full_house() {
        let cards = cards_from_codes(&["S5", "H5", "C5", "S6", "H6"]);
        let h = parse_hand(&cards, level()).unwrap();
        assert_eq!(h.ty, HandType::FullHouse);
        assert_eq!(h.key, Rank::R5);
    }

    #[test]
    fn straight_a2345_and_tjqka() {
        let low = cards_from_codes(&["SA", "S2", "S3", "S4", "H5"]);
        // level is 2 — 2 is level card; still OK in straight as natural 2
        let h = parse_hand(&low, Rank::R7).unwrap();
        assert_eq!(h.ty, HandType::Straight);

        let high = cards_from_codes(&["ST", "HJ", "CQ", "DK", "SA"]);
        let h = parse_hand(&high, Rank::R7).unwrap();
        assert_eq!(h.ty, HandType::Straight);
        assert_eq!(h.key, Rank::RA);
    }

    #[test]
    fn tube_and_plate() {
        let tube = cards_from_codes(&["S3", "H3", "S4", "H4", "S5", "H5"]);
        let h = parse_hand(&tube, Rank::R7).unwrap();
        assert_eq!(h.ty, HandType::Tube);

        let plate = cards_from_codes(&["S3", "H3", "C3", "S4", "H4", "C4"]);
        let h = parse_hand(&plate, Rank::R7).unwrap();
        assert_eq!(h.ty, HandType::Plate);
    }

    #[test]
    fn level_breaks_tube() {
        // level 2: 2-2-3-3-4-4 is NOT a tube
        let cards = cards_from_codes(&["S2", "C2", "S3", "H3", "S4", "H4"]);
        assert!(parse_hand(&cards, Rank::R2).is_err());
    }

    #[test]
    fn bomb4_and_joker_bomb() {
        let b = cards_from_codes(&["S8", "H8", "C8", "D8"]);
        let h = parse_hand(&b, Rank::R2).unwrap();
        assert_eq!(h.ty, HandType::Bomb);
        assert_eq!(h.length, 4);

        let j = cards_from_codes(&["BJ", "BJ", "RJ", "RJ"]);
        // cards_from_codes gives unique ids but same ranks - need actual jokers
        // parse_card_code only one BJ - fix by manual cards
        let jokers = vec![
            Card {
                id: 0,
                suit: Suit::Joker,
                rank: Rank::BlackJoker,
            },
            Card {
                id: 1,
                suit: Suit::Joker,
                rank: Rank::BlackJoker,
            },
            Card {
                id: 2,
                suit: Suit::Joker,
                rank: Rank::RedJoker,
            },
            Card {
                id: 3,
                suit: Suit::Joker,
                rank: Rank::RedJoker,
            },
        ];
        let h = parse_hand(&jokers, Rank::R2).unwrap();
        assert_eq!(h.ty, HandType::JokerBomb);
        let _ = j;
    }

    #[test]
    fn wild_completes_triple() {
        // level 7, heart 7 is wild + two 8s → triple 8
        let cards = cards_from_codes(&["S8", "C8", "H7"]);
        let h = parse_hand(&cards, Rank::R7).unwrap();
        assert_eq!(h.ty, HandType::Triple);
        assert_eq!(h.key, Rank::R8);
    }

    #[test]
    fn wild_bomb() {
        let cards = cards_from_codes(&["S9", "H9", "C9", "H2"]);
        // level 2, H2 wild → bomb of 9s length 4
        let h = parse_hand(&cards, Rank::R2).unwrap();
        assert_eq!(h.ty, HandType::Bomb);
        assert_eq!(h.key, Rank::R9);
        assert_eq!(h.length, 4);
    }

    #[test]
    fn straight_flush() {
        let cards = cards_from_codes(&["S3", "S4", "S5", "S6", "S7"]);
        let h = parse_hand(&cards, Rank::RA).unwrap();
        assert_eq!(h.ty, HandType::StraightFlush);
    }

    #[test]
    fn wild_straight_picks_strongest() {
        // level 2 wild + 4-5-6-7 (mixed suits) → strongest interpretation
        // 4-5-6-7-8 (key 8)
        let cards = cards_from_codes(&["H2", "S4", "H5", "S6", "C7"]);
        let h = parse_hand(&cards, Rank::R2).unwrap();
        assert_eq!(h.ty, HandType::Straight);
        assert_eq!(h.key, Rank::R8);
    }

    #[test]
    fn wild_full_house_picks_strongest() {
        // level 2 wild + 5-5-6-6 → 6s full of 5s, not the weaker 5s full
        let cards = cards_from_codes(&["S5", "C5", "S6", "C6", "H2"]);
        let h = parse_hand(&cards, Rank::R2).unwrap();
        assert_eq!(h.ty, HandType::FullHouse);
        assert_eq!(h.key, Rank::R6);
    }
}
