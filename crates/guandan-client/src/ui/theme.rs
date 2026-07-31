//! Minimal low-glare palette — charcoal surfaces, one soft accent.
//!
//! Hallmark · genre: modern-minimal · tone: austere

use ratatui::style::{Color, Style};

/// App canvas (near-black, low blue light).
pub const BG: Color = Color::Rgb(22, 23, 26);
/// Slightly raised panel.
pub const SURFACE: Color = Color::Rgb(30, 31, 36);
/// Hairline / quiet border.
pub const BORDER: Color = Color::Rgb(55, 56, 62);
/// Active / focus border.
pub const BORDER_FOCUS: Color = Color::Rgb(100, 120, 150);

/// Primary text.
pub const TEXT: Color = Color::Rgb(228, 228, 231);
/// Secondary text.
pub const MUTED: Color = Color::Rgb(140, 142, 150);
/// Single accent (steel blue — used sparingly).
pub const ACCENT: Color = Color::Rgb(130, 155, 190);
/// Soft turn indicator (not yellow flood).
pub const TURN: Color = Color::Rgb(150, 175, 210);

/// Card face.
pub const PAPER: Color = Color::Rgb(245, 245, 247);
pub const PAPER_SEL: Color = Color::Rgb(232, 238, 248);
pub const INK: Color = Color::Rgb(24, 24, 27);
/// Muted rose — readable red without glare.
pub const INK_RED: Color = Color::Rgb(180, 95, 100);
pub const WILD: Color = Color::Rgb(150, 130, 90);

pub fn surface() -> Style {
    Style::default().fg(TEXT).bg(SURFACE)
}

pub fn panel_border() -> Style {
    Style::default().fg(BORDER).bg(SURFACE)
}

pub fn panel_title() -> Style {
    Style::default().fg(MUTED).bg(SURFACE)
}

pub fn active_border() -> Style {
    Style::default().fg(BORDER_FOCUS).bg(SURFACE)
}
