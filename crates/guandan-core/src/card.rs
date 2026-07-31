//! Card, suit, rank, and dual-deck construction for Guandan.

use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique id for a physical card instance (two decks → duplicates of same face).
pub type CardId = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Suit {
    Spade,
    Heart,
    Club,
    Diamond,
    Joker,
}

impl Suit {
    pub fn symbol(self) -> &'static str {
        match self {
            Suit::Spade => "♠",
            Suit::Heart => "♥",
            Suit::Club => "♣",
            Suit::Diamond => "♦",
            Suit::Joker => "",
        }
    }

    pub fn is_red(self) -> bool {
        matches!(self, Suit::Heart | Suit::Diamond)
    }

    /// Compact wire/code letter: S H C D J
    pub fn code(self) -> char {
        match self {
            Suit::Spade => 'S',
            Suit::Heart => 'H',
            Suit::Club => 'C',
            Suit::Diamond => 'D',
            Suit::Joker => 'J',
        }
    }

    pub fn from_code(c: char) -> Option<Self> {
        match c.to_ascii_uppercase() {
            'S' => Some(Suit::Spade),
            'H' => Some(Suit::Heart),
            'C' => Some(Suit::Club),
            'D' => Some(Suit::Diamond),
            'J' => Some(Suit::Joker),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    Black,
    Red,
}

/// Face rank. Numeric ranks use the card face value; jokers are separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rank {
    R2 = 2,
    R3 = 3,
    R4 = 4,
    R5 = 5,
    R6 = 6,
    R7 = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    RJ = 11,
    RQ = 12,
    RK = 13,
    RA = 14,
    BlackJoker = 15,
    RedJoker = 16,
}

impl Rank {
    /// Non-joker ranks in ascending face order (2..A).
    pub const FACES: [Rank; 13] = [
        Rank::R2,
        Rank::R3,
        Rank::R4,
        Rank::R5,
        Rank::R6,
        Rank::R7,
        Rank::R8,
        Rank::R9,
        Rank::R10,
        Rank::RJ,
        Rank::RQ,
        Rank::RK,
        Rank::RA,
    ];

    /// Valid hand levels: 2 through Ace.
    pub const LEVELS: [Rank; 13] = Self::FACES;

    pub fn is_joker(self) -> bool {
        matches!(self, Rank::BlackJoker | Rank::RedJoker)
    }

    pub fn is_face(self) -> bool {
        !self.is_joker()
    }

    /// Display label for UI.
    pub fn label(self) -> &'static str {
        match self {
            Rank::R2 => "2",
            Rank::R3 => "3",
            Rank::R4 => "4",
            Rank::R5 => "5",
            Rank::R6 => "6",
            Rank::R7 => "7",
            Rank::R8 => "8",
            Rank::R9 => "9",
            Rank::R10 => "10",
            Rank::RJ => "J",
            Rank::RQ => "Q",
            Rank::RK => "K",
            Rank::RA => "A",
            Rank::BlackJoker => "小王",
            Rank::RedJoker => "大王",
        }
    }

    /// Compact single-char (or T for 10) used in rank-key input.
    pub fn key_char(self) -> char {
        match self {
            Rank::R2 => '2',
            Rank::R3 => '3',
            Rank::R4 => '4',
            Rank::R5 => '5',
            Rank::R6 => '6',
            Rank::R7 => '7',
            Rank::R8 => '8',
            Rank::R9 => '9',
            Rank::R10 => 'T',
            Rank::RJ => 'J',
            Rank::RQ => 'Q',
            Rank::RK => 'K',
            Rank::RA => 'A',
            Rank::BlackJoker => 'B',
            Rank::RedJoker => 'R',
        }
    }

    pub fn from_key_char(c: char) -> Option<Self> {
        match c.to_ascii_uppercase() {
            '2' => Some(Rank::R2),
            '3' => Some(Rank::R3),
            '4' => Some(Rank::R4),
            '5' => Some(Rank::R5),
            '6' => Some(Rank::R6),
            '7' => Some(Rank::R7),
            '8' => Some(Rank::R8),
            '9' => Some(Rank::R9),
            '0' | 'T' => Some(Rank::R10),
            'J' => Some(Rank::RJ),
            'Q' => Some(Rank::RQ),
            'K' => Some(Rank::RK),
            'A' => Some(Rank::RA),
            'B' => Some(Rank::BlackJoker),
            'R' => Some(Rank::RedJoker),
            _ => None,
        }
    }

    /// Next level after winning (+n levels). Caps at Ace.
    pub fn advance(self, steps: u8) -> Rank {
        debug_assert!(self.is_face());
        let idx = Self::LEVELS.iter().position(|&r| r == self).unwrap_or(0);
        let new_idx = (idx + steps as usize).min(Self::LEVELS.len() - 1);
        Self::LEVELS[new_idx]
    }

    /// Index 0..12 for faces only (2=0 … A=12). Used for straight continuity.
    pub fn face_index(self) -> Option<u8> {
        if self.is_joker() {
            return None;
        }
        Some(match self {
            Rank::R2 => 0,
            Rank::R3 => 1,
            Rank::R4 => 2,
            Rank::R5 => 3,
            Rank::R6 => 4,
            Rank::R7 => 5,
            Rank::R8 => 6,
            Rank::R9 => 7,
            Rank::R10 => 8,
            Rank::RJ => 9,
            Rank::RQ => 10,
            Rank::RK => 11,
            Rank::RA => 12,
            _ => unreachable!(),
        })
    }

    pub fn from_face_index(i: u8) -> Option<Rank> {
        Self::LEVELS.get(i as usize).copied()
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub suit: Suit,
    pub rank: Rank,
}

impl Card {
    pub fn color(self) -> Color {
        if self.suit == Suit::Joker {
            if self.rank == Rank::RedJoker {
                Color::Red
            } else {
                Color::Black
            }
        } else if self.suit.is_red() {
            Color::Red
        } else {
            Color::Black
        }
    }

    /// Heart level cards are wild (逢人配) when forming combinations.
    pub fn is_wild(self, level: Rank) -> bool {
        self.suit == Suit::Heart && self.rank == level && level.is_face()
    }

    /// Compact encoding: `H7`, `ST` (10), `BJ`, `RJ`.
    pub fn encode(self) -> String {
        match self.rank {
            Rank::BlackJoker => "BJ".to_string(),
            Rank::RedJoker => "RJ".to_string(),
            r => format!("{}{}", self.suit.code(), r.key_char()),
        }
    }

    pub fn display(self) -> String {
        match self.rank {
            Rank::BlackJoker => "小王".to_string(),
            Rank::RedJoker => "大王".to_string(),
            r => format!("{}{}", self.suit.symbol(), r.label()),
        }
    }

    /// Display with wild marker when applicable.
    pub fn display_with_level(self, level: Rank) -> String {
        let mut s = self.display();
        if self.is_wild(level) {
            s.push('★');
        }
        s
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display())
    }
}

/// Comparison strength for playing order under a given hand level.
/// Higher value beats lower for same-type singles/pairs/etc.
pub fn play_strength(rank: Rank, level: Rank) -> u8 {
    if rank == Rank::RedJoker {
        return 17;
    }
    if rank == Rank::BlackJoker {
        return 16;
    }
    if rank == level {
        return 15; // level card above Ace
    }
    // 2..A map to 2..14 but A is 14; we need 2 lowest among faces when not level.
    // Natural face order for Guandan non-level: 2 < 3 < … < A
    rank as u8
}

/// Build two full decks (108 cards) with stable unique ids 0..107.
pub fn new_double_deck() -> Vec<Card> {
    let mut cards = Vec::with_capacity(108);
    let mut id: CardId = 0;
    for _deck in 0..2 {
        for &suit in &[Suit::Spade, Suit::Heart, Suit::Club, Suit::Diamond] {
            for &rank in &Rank::FACES {
                cards.push(Card { id, suit, rank });
                id += 1;
            }
        }
        cards.push(Card {
            id,
            suit: Suit::Joker,
            rank: Rank::BlackJoker,
        });
        id += 1;
        cards.push(Card {
            id,
            suit: Suit::Joker,
            rank: Rank::RedJoker,
        });
        id += 1;
    }
    debug_assert_eq!(cards.len(), 108);
    cards
}

pub fn shuffle_deck<R: Rng + ?Sized>(deck: &mut [Card], rng: &mut R) {
    deck.shuffle(rng);
}

/// Sort hand for display: by play strength (low→high), then suit.
pub fn sort_hand(hand: &mut [Card], level: Rank) {
    hand.sort_by(|a, b| {
        play_strength(a.rank, level)
            .cmp(&play_strength(b.rank, level))
            .then_with(|| (a.suit as u8).cmp(&(b.suit as u8)))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Deal 108 cards into 4 hands of 27.
pub fn deal_four<R: Rng + ?Sized>(rng: &mut R) -> [Vec<Card>; 4] {
    let mut deck = new_double_deck();
    shuffle_deck(&mut deck, rng);
    let mut hands: [Vec<Card>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for (i, card) in deck.into_iter().enumerate() {
        hands[i % 4].push(card);
    }
    hands
}

/// Parse compact card codes like `H7`, `ST`, `BJ` (without id — for tests/protocol helpers).
/// Returns a card with id=0; prefer matching by suit+rank against a real hand in production.
pub fn parse_card_code(code: &str) -> Option<Card> {
    let code = code.trim().to_ascii_uppercase();
    if code == "BJ" {
        return Some(Card {
            id: 0,
            suit: Suit::Joker,
            rank: Rank::BlackJoker,
        });
    }
    if code == "RJ" {
        return Some(Card {
            id: 0,
            suit: Suit::Joker,
            rank: Rank::RedJoker,
        });
    }
    let mut chars = code.chars();
    let s = chars.next()?;
    let rest: String = chars.collect();
    let suit = Suit::from_code(s)?;
    if suit == Suit::Joker {
        return None;
    }
    let rank_ch = if rest == "10" {
        'T'
    } else {
        rest.chars().next()?
    };
    let rank = Rank::from_key_char(rank_ch)?;
    Some(Card { id: 0, suit, rank })
}

/// Build test cards from codes; assigns sequential ids.
pub fn cards_from_codes(codes: &[&str]) -> Vec<Card> {
    codes
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            parse_card_code(c).map(|mut card| {
                card.id = i as CardId;
                card
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_deck_has_108() {
        assert_eq!(new_double_deck().len(), 108);
    }

    #[test]
    fn level_card_stronger_than_ace() {
        let level = Rank::RQ;
        assert!(play_strength(Rank::RQ, level) > play_strength(Rank::RA, level));
        assert!(play_strength(Rank::BlackJoker, level) > play_strength(Rank::RQ, level));
        assert!(play_strength(Rank::RedJoker, level) > play_strength(Rank::BlackJoker, level));
    }

    #[test]
    fn wild_is_heart_level() {
        let c = Card {
            id: 1,
            suit: Suit::Heart,
            rank: Rank::R7,
        };
        assert!(c.is_wild(Rank::R7));
        assert!(!c.is_wild(Rank::R8));
        let s = Card {
            id: 2,
            suit: Suit::Spade,
            rank: Rank::R7,
        };
        assert!(!s.is_wild(Rank::R7));
    }

    #[test]
    fn encode_roundtrip_faces() {
        let c = Card {
            id: 0,
            suit: Suit::Spade,
            rank: Rank::R10,
        };
        assert_eq!(c.encode(), "ST");
        let p = parse_card_code("ST").unwrap();
        assert_eq!(p.suit, Suit::Spade);
        assert_eq!(p.rank, Rank::R10);
    }

    #[test]
    fn advance_levels() {
        assert_eq!(Rank::R2.advance(3), Rank::R5);
        assert_eq!(Rank::RA.advance(1), Rank::RA);
        assert_eq!(Rank::RK.advance(5), Rank::RA);
    }

    #[test]
    fn deal_four_hands() {
        let mut rng = rand::rng();
        let hands = deal_four(&mut rng);
        for h in &hands {
            assert_eq!(h.len(), 27);
        }
        let mut ids: Vec<_> = hands.iter().flatten().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..108).collect::<Vec<_>>());
    }
}
