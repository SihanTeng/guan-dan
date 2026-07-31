//! Fixed game timing (not configurable).

use std::time::Duration;

use guandan_protocol::{PLAY_REVEAL_SECS, TURN_TIMEOUT_SECS};

/// Standard Guandan multiplayer timing — fixed values only.
#[derive(Debug, Clone, Copy, Default)]
pub struct GameSettings;

impl GameSettings {
    pub fn turn_timeout(self) -> Duration {
        Duration::from_secs(TURN_TIMEOUT_SECS as u64)
    }

    pub fn play_reveal(self) -> Duration {
        Duration::from_secs(PLAY_REVEAL_SECS as u64)
    }

    pub fn turn_secs(self) -> u32 {
        TURN_TIMEOUT_SECS
    }

    pub fn reveal_secs(self) -> u32 {
        PLAY_REVEAL_SECS
    }
}
