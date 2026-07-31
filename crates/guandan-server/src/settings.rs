//! Server game timing settings (turn timer + play reveal).

use std::time::Duration;

/// Standard Guandan multiplayer timing.
#[derive(Debug, Clone, Copy)]
pub struct GameSettings {
    /// Max seconds per turn before auto-pass / auto-lead (default 30).
    pub turn_timeout: Duration,
    /// Hold after a play/pass so others can see it before the next act (default 3s).
    pub play_reveal: Duration,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            turn_timeout: Duration::from_secs(30),
            play_reveal: Duration::from_secs(3),
        }
    }
}

impl GameSettings {
    pub fn turn_secs(self) -> u32 {
        self.turn_timeout.as_secs() as u32
    }

    pub fn reveal_secs(self) -> u32 {
        self.play_reveal.as_secs() as u32
    }
}
