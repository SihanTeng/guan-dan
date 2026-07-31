//! Client ↔ server message types (JSON over WebSocket).

use guandan_core::{Card, FinishRank, HandType, Rank, Seat, TeamId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    Reconnect { session_id: Uuid },
    CreateRoom { name: String },
    JoinRoom { room_id: String },
    LeaveRoom,
    QuickMatch,
    PracticeMatch,
    Ready,
    CancelReady,
    ListRooms,
    PlayCards { card_ids: Vec<u8> },
    Pass,
    ReturnTribute { card_id: u8, to_seat: Seat },
    Chat { content: String },
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
    },
    CardPlayed {
        seat: Seat,
        cards: Vec<Card>,
        hand_type: HandType,
        counts: [usize; 4],
    },
    PlayerPass {
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
    MatchOver {
        winner_team: TeamId,
        levels: [Rank; 2],
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
