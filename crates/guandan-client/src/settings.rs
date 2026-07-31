//! Client display settings (configurable timers).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Local client settings. Override via CLI or `~/.config/guandan/settings.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// How long to hold another player's play on screen (seconds). Default 3.
    #[serde(default = "default_reveal")]
    pub play_reveal_secs: u64,
    /// Fallback turn timeout display if server omits it (seconds). Default 30.
    #[serde(default = "default_turn")]
    pub turn_timeout_secs: u64,
}

fn default_reveal() -> u64 {
    3
}
fn default_turn() -> u64 {
    30
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            play_reveal_secs: default_reveal(),
            turn_timeout_secs: default_turn(),
        }
    }
}

impl Settings {
    pub fn config_path() -> PathBuf {
        if let Some(dir) = dirs_config() {
            dir.join("guandan").join("settings.toml")
        } else {
            PathBuf::from("guandan-settings.toml")
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(s) = toml::from_str(&text) {
                return s;
            }
        }
        Self::default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, text)
    }
}

fn dirs_config() -> Option<PathBuf> {
    // XDG or home .config without extra crate
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
}
