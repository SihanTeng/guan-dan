//! Shared palette — felt table + card paper.

use ratatui::style::{Color, Modifier, Style};

/// Deep green felt (table).
pub const FELT: Color = Color::Rgb(18, 72, 42);
pub const FELT_DARK: Color = Color::Rgb(10, 42, 24);
pub const FELT_LIGHT: Color = Color::Rgb(28, 98, 56);

/// Card face paper.
pub const PAPER: Color = Color::Rgb(255, 252, 245);
pub const PAPER_DIM: Color = Color::Rgb(220, 216, 208);

/// Ink on cards.
pub const INK: Color = Color::Rgb(20, 20, 24);
pub const INK_RED: Color = Color::Rgb(190, 30, 40);
pub const INK_GOLD: Color = Color::Rgb(180, 140, 40);

/// Chrome / accents.
pub const ACCENT: Color = Color::Rgb(255, 210, 80);
pub const CYAN: Color = Color::Rgb(90, 210, 230);
pub const MUTED: Color = Color::Rgb(160, 190, 170);
pub const DANGER: Color = Color::Rgb(255, 100, 90);
pub const TURN_GLOW: Color = Color::Rgb(255, 230, 120);

pub fn panel() -> Style {
    Style::default().fg(MUTED).bg(FELT_DARK)
}

pub fn panel_border() -> Style {
    Style::default().fg(FELT_LIGHT).bg(FELT_DARK)
}

pub fn panel_title() -> Style {
    Style::default()
        .fg(ACCENT)
        .bg(FELT_DARK)
        .add_modifier(Modifier::BOLD)
}

pub fn active_border() -> Style {
    Style::default().fg(TURN_GLOW).bg(FELT_DARK)
}

pub fn muted_on_felt() -> Style {
    Style::default().fg(MUTED).bg(FELT)
}
