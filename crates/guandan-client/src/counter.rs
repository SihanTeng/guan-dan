//! 记牌器 — remaining cards held by opponents.
//!
//! Dual-deck 掼蛋 (108 cards): each face rank ×8, each joker ×2.
//! remaining[r] = deck_total[r] − my_hand[r] − played[r]

use guandan_core::{Card, Rank};

/// Display order: high → low (王 first, then 2, A…3).
pub const COUNTER_RANKS: [Rank; 15] = [
    Rank::RedJoker,
    Rank::BlackJoker,
    Rank::R2,
    Rank::RA,
    Rank::RK,
    Rank::RQ,
    Rank::RJ,
    Rank::R10,
    Rank::R9,
    Rank::R8,
    Rank::R7,
    Rank::R6,
    Rank::R5,
    Rank::R4,
    Rank::R3,
];

/// How many copies of `rank` exist in two full decks.
pub fn deck_total(rank: Rank) -> u8 {
    if rank.is_joker() {
        2
    } else {
        8
    }
}

fn rank_idx(rank: Rank) -> usize {
    rank as u8 as usize
}

/// Tracks cards publicly played this hand. Hand is supplied at query time so
/// tribute / play mutations stay accurate without double-counting.
#[derive(Debug, Clone)]
pub struct CardCounter {
    /// Count of each rank played to the table this hand (index = Rank as u8).
    played: [u8; 17],
}

impl Default for CardCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl CardCounter {
    pub fn new() -> Self {
        Self { played: [0; 17] }
    }

    /// New hand: clear played history.
    pub fn reset(&mut self) {
        self.played = [0; 17];
    }

    /// Record cards that left play (public CardPlayed).
    pub fn note_played(&mut self, cards: &[Card]) {
        for c in cards {
            let i = rank_idx(c.rank);
            if i < self.played.len() {
                self.played[i] = self.played[i].saturating_add(1);
            }
        }
    }

    /// Cards of `rank` still held by the other three seats.
    pub fn remaining_of(&self, rank: Rank, hand: &[Card]) -> u8 {
        let mine = hand.iter().filter(|c| c.rank == rank).count() as u8;
        let played = self.played.get(rank_idx(rank)).copied().unwrap_or(0);
        deck_total(rank).saturating_sub(mine).saturating_sub(played)
    }

    /// Remaining counts in display order.
    pub fn remaining_row(&self, hand: &[Card]) -> [(Rank, u8); 15] {
        let mut out = [(Rank::R3, 0u8); 15];
        for (i, &rank) in COUNTER_RANKS.iter().enumerate() {
            out[i] = (rank, self.remaining_of(rank, hand));
        }
        out
    }

    /// Short single-column label (ASCII, width 1) so the TUI grid stays aligned.
    /// Ten is `T` (same as typed input), not `10`.
    pub fn rank_header(rank: Rank) -> &'static str {
        match rank {
            Rank::RedJoker => "R",
            Rank::BlackJoker => "B",
            Rank::R10 => "T",
            Rank::RJ => "J",
            Rank::RQ => "Q",
            Rank::RK => "K",
            Rank::RA => "A",
            Rank::R2 => "2",
            Rank::R3 => "3",
            Rank::R4 => "4",
            Rank::R5 => "5",
            Rank::R6 => "6",
            Rank::R7 => "7",
            Rank::R8 => "8",
            Rank::R9 => "9",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guandan_core::card::cards_from_codes;

    #[test]
    fn deal_then_play_tracks_opponents() {
        let mut cc = CardCounter::new();
        // Full dual deck minus 2 kings in hand → 6 kings left for others.
        let hand = cards_from_codes(&["SK", "HK"]);
        assert_eq!(cc.remaining_of(Rank::RK, &hand), 6);

        // Opponent plays one king.
        cc.note_played(&cards_from_codes(&["CK"]));
        assert_eq!(cc.remaining_of(Rank::RK, &hand), 5);

        // I play one of my kings: hand shrinks, played grows → remaining unchanged.
        let hand2 = cards_from_codes(&["SK"]);
        cc.note_played(&cards_from_codes(&["HK"]));
        assert_eq!(cc.remaining_of(Rank::RK, &hand2), 5);
    }

    #[test]
    fn jokers_are_two_each() {
        let cc = CardCounter::new();
        assert_eq!(cc.remaining_of(Rank::RedJoker, &[]), 2);
        assert_eq!(cc.remaining_of(Rank::BlackJoker, &[]), 2);
        let hand = cards_from_codes(&["RJ", "BJ"]);
        assert_eq!(cc.remaining_of(Rank::RedJoker, &hand), 1);
        assert_eq!(cc.remaining_of(Rank::BlackJoker, &hand), 1);
    }

    #[test]
    fn reset_clears_played() {
        let mut cc = CardCounter::new();
        cc.note_played(&cards_from_codes(&["S3", "H3", "C3"]));
        assert_eq!(cc.remaining_of(Rank::R3, &[]), 5);
        cc.reset();
        assert_eq!(cc.remaining_of(Rank::R3, &[]), 8);
    }
}
