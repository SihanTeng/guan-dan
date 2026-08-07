//! Client ↔ server message types (JSON over WebSocket).

use guandan_core::{Card, FinishRank, HandType, Rank, Seat, TeamId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Fixed turn time limit (seconds). Not configurable.
pub const TURN_TIMEOUT_SECS: u32 = 30;
/// Fixed hold time after a play so others can see it (seconds). Not configurable.
pub const PLAY_REVEAL_SECS: u32 = 3;
/// Seconds for all seats to confirm hand ranks before next deal.
/// Unconfirmed seats (humans) are auto-confirmed when this elapses; bots
/// confirm immediately on the result board.
pub const CONFIRM_TIMEOUT_SECS: u32 = 10;
/// Seconds to re-party empty seats (bots fill / new humans can join).
pub const REPARTY_TIMEOUT_SECS: u32 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub ty: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

// ── Client → Server ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ty", rename_all = "snake_case")]
pub enum ClientMessage {
    Ping,
    Reconnect {
        session_id: Uuid,
    },
    CreateRoom {
        name: String,
    },
    JoinRoom {
        room_id: String,
    },
    LeaveRoom,
    QuickMatch,
    PracticeMatch,
    Ready,
    CancelReady,
    ListRooms,
    PlayCards {
        card_ids: Vec<u8>,
    },
    Pass,
    ReturnTribute {
        card_id: u8,
        to_seat: Seat,
    },
    /// Confirm hand/match result ranks (all 4 required before next round).
    ConfirmResult,
    /// Take a vacant seat as substitute (between hands or after timeout re-party).
    TakeSeat {
        seat: Seat,
    },
    Chat {
        content: String,
    },
}

// ── Server → Client ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ty", rename_all = "snake_case")]
pub enum ServerMessage {
    Connected {
        session_id: Uuid,
        player_id: Uuid,
    },
    Pong,
    Error {
        message: String,
    },
    OnlineCount {
        count: u32,
    },
    RoomCreated {
        room_id: String,
    },
    RoomJoined {
        room_id: String,
        seat: Seat,
        players: Vec<SeatInfo>,
    },
    PlayerJoined {
        seat: Seat,
        info: SeatInfo,
    },
    PlayerLeft {
        seat: Seat,
    },
    PlayerReady {
        seat: Seat,
        ready: bool,
    },
    RoomList {
        rooms: Vec<RoomSummary>,
    },
    MatchFound {
        room_id: String,
        seat: Seat,
    },
    GameStart {
        seats: Vec<SeatInfo>,
        team_levels: [Rank; 2],
        hand_level: Rank,
        hand_number: u32,
    },
    Deal {
        hand: Vec<Card>,
        hand_level: Rank,
        lead: Seat,
        counts: [usize; 4],
    },
    TributePaid {
        from: Seat,
        card: Card,
        to: Seat,
    },
    AntiTribute {
        dwellers: Vec<Seat>,
    },
    TributeReturnTurn {
        seat: Seat,
        payers: Vec<Seat>,
    },
    TributeReturned {
        from: Seat,
        card: Card,
        to: Seat,
    },
    PlayTurn {
        seat: Seat,
        must_lead: bool,
        last_play: Option<PublicPlay>,
        /// Always [`TURN_TIMEOUT_SECS`] (30); kept for older clients / UI.
        #[serde(default = "default_turn_timeout_secs")]
        timeout_secs: u32,
        /// Whether this seat has any legal follow (false → must pass / notify 无牌可出).
        #[serde(default = "default_true")]
        can_follow: bool,
    },
    CardPlayed {
        seat: Seat,
        cards: Vec<Card>,
        hand_type: HandType,
        counts: [usize; 4],
        /// Always [`PLAY_REVEAL_SECS`] (3).
        #[serde(default = "default_play_reveal_secs")]
        reveal_secs: u32,
    },
    PlayerPass {
        seat: Seat,
        #[serde(default = "default_play_reveal_secs")]
        reveal_secs: u32,
    },
    /// Server forced pass/play because the turn timer expired.
    TurnTimeout {
        seat: Seat,
    },
    PlayerOut {
        seat: Seat,
        rank: FinishRank,
    },
    HandResult {
        finish_order: Vec<Seat>,
        /// Parallel to finish_order — 上游/二游/三游/下游 for each seat.
        ranks: Vec<FinishRank>,
        winning_team: TeamId,
        level_gain: u8,
        new_levels: [Rank; 2],
        match_over: bool,
        winner_team: Option<TeamId>,
        #[serde(default = "default_confirm_timeout")]
        confirm_timeout_secs: u32,
        /// Who already confirmed (index = seat).
        #[serde(default)]
        confirmed: [bool; 4],
    },
    ResultConfirmed {
        seat: Seat,
        confirmed: [bool; 4],
    },
    /// All four confirmed ranks — next deal / match leave may proceed.
    AllConfirmed {
        match_over: bool,
    },
    MatchOver {
        winner_team: TeamId,
        levels: [Rank; 2],
    },
    /// Seat opened for re-party / substitute (bot or disconnected).
    SeatOpened {
        seat: Seat,
        reason: String,
    },
    SeatTaken {
        seat: Seat,
        info: SeatInfo,
    },
    /// Notify current player they have no legal play.
    NoLegalPlay {
        seat: Seat,
    },
    Chat {
        seat: Option<Seat>,
        name: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeatInfo {
    pub seat: Seat,
    pub name: String,
    pub is_bot: bool,
    pub ready: bool,
    pub team: TeamId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummary {
    pub room_id: String,
    pub players: usize,
    pub max_players: usize,
    pub in_game: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicPlay {
    pub seat: Seat,
    pub cards: Vec<Card>,
    pub hand_type: HandType,
    pub key: Rank,
}

fn default_turn_timeout_secs() -> u32 {
    TURN_TIMEOUT_SECS
}

fn default_play_reveal_secs() -> u32 {
    PLAY_REVEAL_SECS
}

fn default_confirm_timeout() -> u32 {
    CONFIRM_TIMEOUT_SECS
}

fn default_true() -> bool {
    true
}

pub fn encode_client(msg: &ClientMessage) -> Result<String, serde_json::Error> {
    serde_json::to_string(msg)
}

pub fn decode_client(s: &str) -> Result<ClientMessage, serde_json::Error> {
    serde_json::from_str(s)
}

pub fn encode_server(msg: &ServerMessage) -> Result<String, serde_json::Error> {
    serde_json::to_string(msg)
}

pub fn decode_server(s: &str) -> Result<ServerMessage, serde_json::Error> {
    serde_json::from_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ping() {
        let m = ClientMessage::Ping;
        let s = encode_client(&m).unwrap();
        let back = decode_client(&s).unwrap();
        assert!(matches!(back, ClientMessage::Ping));
    }

    #[test]
    fn roundtrip_connected() {
        let m = ServerMessage::Connected {
            session_id: Uuid::nil(),
            player_id: Uuid::nil(),
        };
        let s = encode_server(&m).unwrap();
        let back = decode_server(&s).unwrap();
        assert!(matches!(back, ServerMessage::Connected { .. }));
    }
}
