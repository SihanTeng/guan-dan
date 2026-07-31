//! Pure Guandan (掼蛋) game engine.
//!
//! No network or UI dependencies. Server and client share this crate for
//! authoritative validation and local preview.

pub mod card;
pub mod match_;
pub mod rule;

pub use card::{
    find_card_indices_in_hand, find_cards_in_hand, parse_rank_input, Card, CardId, Color, Rank,
    Suit,
};
pub use match_::{
    partner, team_of, Action, Event, FinishRank, Match, MatchError, MatchPhase, PlayerState, Seat,
    TeamId,
};
pub use rule::{
    can_beat, can_follow, find_smallest_beater, find_smallest_lead, parse_hand, HandType,
    ParsedHand, RuleError,
};
