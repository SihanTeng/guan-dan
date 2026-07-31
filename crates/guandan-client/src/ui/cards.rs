//! Card faces — ASCII-only glyphs so every terminal can render them.
//!
//! Jokers never use CJK (王) or emoji: terminals often lack those fonts.
//! True font embedding is not available in a pure TTY UI (the emulator
//! draws glyphs); we use box-drawing + Latin labels instead.

use guandan_core::{Card, Rank, Suit};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::theme::{self, INK, INK_RED, PAPER, PAPER_SEL, WILD};

/// Normal + joker share the same cell size for grid alignment.
pub const CARD_W: u16 = 5;
pub const CARD_H: u16 = 4;

#[derive(Clone, Copy)]
pub struct CardFace {
    pub card: Card,
    pub level: Rank,
    pub selected: bool,
    pub cursor: bool,
}

impl CardFace {
    pub fn is_joker(self) -> bool {
        self.card.rank.is_joker()
    }

    pub fn is_big_joker(self) -> bool {
        self.card.rank == Rank::RedJoker
    }

    pub fn ink(self) -> Color {
        if self.card.rank == Rank::RedJoker || matches!(self.card.suit, Suit::Heart | Suit::Diamond)
        {
            INK_RED
        } else {
            INK
        }
    }

    /// Single-cell-friendly rank for normal cards (ASCII / digits).
    pub fn rank_label(self) -> String {
        match self.card.rank {
            Rank::BlackJoker => "sJ".into(), // small joker
            Rank::RedJoker => "bJ".into(),   // big joker
            Rank::R10 => "10".into(),
            r => r.label().to_string(),
        }
    }

    pub fn suit_label(self) -> &'static str {
        match self.card.rank {
            Rank::BlackJoker => "*", // geometric mark, not emoji
            Rank::RedJoker => "+",
            _ => self.card.suit.symbol(),
        }
    }

    pub fn is_wild(self) -> bool {
        self.card.is_wild(self.level)
    }

    /// Short label for last-play strips (ASCII only).
    pub fn strip_label(self) -> String {
        match self.card.rank {
            Rank::BlackJoker => "sJ*".into(),
            Rank::RedJoker => "bJ+".into(),
            Rank::R10 => format!("10{}", self.suit_label()),
            r => format!("{}{}", r.label(), self.suit_label()),
        }
    }
}

impl Widget for CardFace {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.is_joker() {
            render_joker(self, area, buf);
        } else {
            render_normal(self, area, buf);
        }
    }
}

fn cell(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    buf[(x, y)].set_symbol(&ch.to_string()).set_style(style);
}

fn row_chars(buf: &mut Buffer, x: u16, y: u16, s: &str, styles: &[Style]) {
    for (i, ch) in s.chars().enumerate() {
        let st = styles.get(i).copied().unwrap_or(styles[0]);
        cell(buf, x + i as u16, y, ch, st);
    }
}

fn render_normal(face: CardFace, area: Rect, buf: &mut Buffer) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    let bg = if face.selected { PAPER_SEL } else { PAPER };
    let border = if face.cursor {
        theme::BORDER_FOCUS
    } else if face.selected {
        theme::ACCENT
    } else if face.is_wild() {
        WILD
    } else {
        Color::Rgb(120, 120, 130)
    };
    let ink = face.ink();
    let x = area.x;
    let y = area.y;
    let w = CARD_W.min(area.width);
    let h = CARD_H.min(area.height);
    let st_b = Style::default().fg(border).bg(bg);
    let st_i = Style::default().fg(ink).bg(bg).add_modifier(Modifier::BOLD);

    let (top, bot, vl, vr) = if face.selected {
        ("╔═══╗", "╚═══╝", '║', '║')
    } else {
        ("┌───┐", "└───┘", '│', '│')
    };

    for (i, ch) in top.chars().take(w as usize).enumerate() {
        cell(buf, x + i as u16, y, ch, st_b);
    }

    if h >= 2 {
        let rank = face.rank_label();
        // │A  │ or │10 │
        let inner = if rank == "10" {
            "10 ".to_string()
        } else {
            format!("{rank:<3}")
        };
        let mut line = String::new();
        line.push(vl);
        line.push_str(&inner.chars().take(3).collect::<String>());
        while line.chars().count() < 4 {
            line.push(' ');
        }
        line.push(vr);
        for (i, ch) in line.chars().take(w as usize).enumerate() {
            let edge = i == 0 || i + 1 >= w as usize;
            cell(buf, x + i as u16, y + 1, ch, if edge { st_b } else { st_i });
        }
    }

    if h >= 3 {
        let suit = face.suit_label().chars().next().unwrap_or('?');
        let mark = if face.is_wild() { '*' } else { ' ' };
        // │ ♠  │ — suit may be multi-width; write as single symbol in middle
        cell(buf, x, y + 2, vl, st_b);
        cell(buf, x + 1, y + 2, ' ', Style::default().bg(bg));
        buf[(x + 2, y + 2)]
            .set_symbol(&suit.to_string())
            .set_style(st_i);
        cell(
            buf,
            x + 3,
            y + 2,
            mark,
            if face.is_wild() {
                st_i
            } else {
                Style::default().bg(bg)
            },
        );
        cell(buf, x + 4, y + 2, vr, st_b);
    }

    if h >= 4 {
        for (i, ch) in bot.chars().take(w as usize).enumerate() {
            cell(buf, x + i as u16, y + 3, ch, st_b);
        }
    }
}

/// Little / big joker — pure ASCII art, no CJK, no emoji.
///
/// ```text
/// ┌───┐   ┌───┐
/// │sJ │   │bJ │
/// │ * │   │ + │
/// └───┘   └───┘
/// little    big
/// ```
fn render_joker(face: CardFace, area: Rect, buf: &mut Buffer) {
    if area.width < 5 || area.height < 4 {
        // Fallback compact
        let bg = PAPER;
        let ink = face.ink();
        let label = if face.is_big_joker() { "bJ" } else { "sJ" };
        buf[(area.x, area.y)]
            .set_symbol(label)
            .set_style(Style::default().fg(ink).bg(bg).add_modifier(Modifier::BOLD));
        return;
    }

    let is_big = face.is_big_joker();
    let bg = if face.selected {
        if is_big {
            Color::Rgb(255, 236, 236)
        } else {
            Color::Rgb(236, 236, 248)
        }
    } else if is_big {
        Color::Rgb(255, 248, 248)
    } else {
        Color::Rgb(248, 248, 255)
    };
    let border = if face.cursor {
        theme::BORDER_FOCUS
    } else if face.selected {
        theme::ACCENT
    } else if is_big {
        INK_RED
    } else {
        Color::Rgb(60, 60, 90)
    };
    let ink = if is_big { INK_RED } else { INK };
    let x = area.x;
    let y = area.y;
    let st_b = Style::default()
        .fg(border)
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    let st_i = Style::default().fg(ink).bg(bg).add_modifier(Modifier::BOLD);
    let st_d = Style::default().fg(ink).bg(bg);
    let st_fill = Style::default().bg(bg);

    // Decorative double-line when selected
    let (t0, t1, t2, t3, t4) = if face.selected {
        ('╔', '═', '═', '═', '╗')
    } else {
        ('┌', '─', '─', '─', '┐')
    };
    let (b0, b1, b2, b3, b4) = if face.selected {
        ('╚', '═', '═', '═', '╝')
    } else {
        ('└', '─', '─', '─', '┘')
    };
    let vl = if face.selected { '║' } else { '│' };

    row_chars(buf, x, y, &format!("{t0}{t1}{t2}{t3}{t4}"), &[st_b; 5]);

    // Line 1: sJ / bJ
    let (a, b) = if is_big { ('b', 'J') } else { ('s', 'J') };
    cell(buf, x, y + 1, vl, st_b);
    cell(buf, x + 1, y + 1, a, st_i);
    cell(buf, x + 2, y + 1, b, st_i);
    cell(buf, x + 3, y + 1, ' ', st_fill);
    cell(buf, x + 4, y + 1, vl, st_b);

    // Line 2: mark + tiny diamond pattern (ASCII)
    let mark = if is_big { '+' } else { '*' };
    cell(buf, x, y + 2, vl, st_b);
    cell(buf, x + 1, y + 2, ' ', st_fill);
    cell(buf, x + 2, y + 2, mark, st_i);
    cell(buf, x + 3, y + 2, if is_big { '!' } else { '.' }, st_d);
    cell(buf, x + 4, y + 2, vl, st_b);

    row_chars(buf, x, y + 3, &format!("{b0}{b1}{b2}{b3}{b4}"), &[st_b; 5]);
}

pub fn render_card_row(
    buf: &mut Buffer,
    area: Rect,
    cards: &[(Card, bool, bool)],
    level: Rank,
    gap: u16,
) {
    let mut x = area.x;
    for &(card, selected, cursor) in cards {
        if x + CARD_W > area.x + area.width {
            break;
        }
        CardFace {
            card,
            level,
            selected,
            cursor,
        }
        .render(Rect::new(x, area.y, CARD_W, CARD_H.min(area.height)), buf);
        x = x.saturating_add(CARD_W + gap);
    }
}

pub fn render_card_grid(
    buf: &mut Buffer,
    area: Rect,
    cards: &[(Card, bool, bool)],
    level: Rank,
    gap: u16,
) {
    if area.height < CARD_H || area.width < CARD_W {
        return;
    }
    let per_row = ((area.width + gap) / (CARD_W + gap)).max(1) as usize;
    let mut idx = 0;
    let mut row = 0u16;
    while idx < cards.len() {
        let y = area.y + row * CARD_H;
        if y + CARD_H > area.y + area.height {
            break;
        }
        let end = (idx + per_row).min(cards.len());
        render_card_row(
            buf,
            Rect::new(area.x, y, area.width, CARD_H),
            &cards[idx..end],
            level,
            gap,
        );
        idx = end;
        row += 1;
    }
}

pub fn card_strip_lines(cards: &[Card], level: Rank) -> Vec<Line<'static>> {
    if cards.is_empty() {
        return vec![Line::from(Span::styled(
            "-",
            Style::default().fg(theme::MUTED),
        ))];
    }
    let mut spans = Vec::new();
    for c in cards {
        let face = CardFace {
            card: *c,
            level,
            selected: false,
            cursor: false,
        };
        spans.push(Span::styled(
            format!("{} ", face.strip_label()),
            Style::default()
                .fg(face.ink())
                .bg(PAPER)
                .add_modifier(Modifier::BOLD),
        ));
    }
    vec![Line::from(spans)]
}
