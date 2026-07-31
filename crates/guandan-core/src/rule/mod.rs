//! Hand parsing, comparison, and move generation for Guandan.

mod beat;
mod parse;

pub use beat::{can_beat, find_smallest_beater, find_smallest_lead};
pub use parse::{parse_hand, HandType, ParsedHand, RuleError};

use crate::card::{Card, Rank};

/// Bomb ranking ladder (higher beats lower regardless of key rank).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BombTier {
    /// 4 of a kind
    Four = 1,
    /// 5 of a kind
    Five = 2,
    /// Straight flush
    StraightFlush = 3,
    /// 6+ of a kind (length breaks ties before rank)
    SixPlus = 4,
    /// Four jokers
    JokerBomb = 5,
}

impl HandType {
    pub fn is_bomb(self) -> bool {
        matches!(
            self,
            HandType::Bomb | HandType::StraightFlush | HandType::JokerBomb
        )
    }

    pub fn bomb_tier(self, length: usize) -> Option<BombTier> {
        match self {
            HandType::JokerBomb => Some(BombTier::JokerBomb),
            HandType::StraightFlush => Some(BombTier::StraightFlush),
            HandType::Bomb if length == 4 => Some(BombTier::Four),
            HandType::Bomb if length == 5 => Some(BombTier::Five),
            HandType::Bomb if length >= 6 => Some(BombTier::SixPlus),
            _ => None,
        }
    }

    pub fn chinese_name(self) -> &'static str {
        match self {
            HandType::Single => "单张",
            HandType::Pair => "对子",
            HandType::Triple => "三同张",
            HandType::FullHouse => "三带二",
            HandType::Straight => "顺子",
            HandType::Tube => "三连对",
            HandType::Plate => "钢板",
            HandType::Bomb => "炸弹",
            HandType::StraightFlush => "同花顺",
            HandType::JokerBomb => "天王炸",
        }
    }
}

/// Count non-wild cards by rank; return wild count separately.
pub(crate) fn split_wilds(cards: &[Card], level: Rank) -> (Vec<Card>, Vec<Card>) {
    let mut fixed = Vec::new();
    let mut wilds = Vec::new();
    for &c in cards {
        if c.is_wild(level) {
            wilds.push(c);
        } else {
            fixed.push(c);
        }
    }
    (fixed, wilds)
}

/// Face ranks only (2..A), for wild substitution targets.
pub(crate) fn face_ranks() -> &'static [Rank] {
    &Rank::FACES
}
