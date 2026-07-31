//! Card faces — denser normal cards + decorated jokers (clown / big / little).

use guandan_core::{Card, Rank, Suit};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::theme::{self, INK, INK_RED, PAPER, PAPER_SEL, WILD};

/// Compact normal card.
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

    pub fn ink(self) -> Color {
        if self.card.rank == Rank::RedJoker || matches!(self.card.suit, Suit::Heart | Suit::Diamond)
        {
            INK_RED
        } else {
            INK
        }
    }

    pub fn rank_label(self) -> String {
        match self.card.rank {
            Rank::BlackJoker => "小王".into(),
            Rank::RedJoker => "大王".into(),
            Rank::R10 => "10".into(),
            r => r.label().to_string(),
        }
    }

    pub fn suit_label(self) -> &'static str {
        match self.card.rank {
            Rank::BlackJoker | Rank::RedJoker => "🤡",
            _ => self.card.suit.symbol(),
        }
    }

    pub fn is_wild(self) -> bool {
        self.card.is_wild(self.level)
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

fn put(buf: &mut Buffer, x: u16, y: u16, s: &str, style: Style) {
    // Write multi-width carefully: one cell at a time for ASCII frames;
    // for emoji/CJK, set_symbol on first cell.
    buf[(x, y)].set_symbol(s).set_style(style);
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

    // Top
    let top = if face.selected {
        "╔═══╗"
    } else {
        "┌───┐"
    };
    for (i, ch) in top.chars().take(w as usize).enumerate() {
        buf[(x + i as u16, y)]
            .set_symbol(&ch.to_string())
            .set_style(st_b);
    }
    // Rank row — corner style
    if h >= 2 {
        let rank = face.rank_label();
        let line = if face.selected {
            if rank == "10" {
                "║10 ║".to_string()
            } else {
                format!("║{rank:<2} ║")
            }
        } else if rank == "10" {
            "│10 │".to_string()
        } else {
            format!("│{rank:<2} │")
        };
        for (i, ch) in line.chars().take(w as usize).enumerate() {
            let edge = i == 0 || i + 1 >= w as usize;
            buf[(x + i as u16, y + 1)]
                .set_symbol(&ch.to_string())
                .set_style(if edge { st_b } else { st_i });
        }
    }
    // Suit row
    if h >= 3 {
        let suit = face.suit_label();
        let mid = if face.is_wild() {
            if face.selected {
                format!("║ {suit}*║")
            } else {
                format!("│ {suit}*│")
            }
        } else if face.selected {
            format!("║ {suit} ║")
        } else {
            format!("│ {suit} │")
        };
        // suit symbols are single-width in most terminals
        for (i, ch) in mid.chars().take(w as usize).enumerate() {
            let edge = i == 0 || i + 1 >= w as usize;
            buf[(x + i as u16, y + 2)]
                .set_symbol(&ch.to_string())
                .set_style(if edge { st_b } else { st_i });
        }
    }
    if h >= 4 {
        let bot = if face.selected {
            "╚═══╝"
        } else {
            "└───┘"
        };
        for (i, ch) in bot.chars().take(w as usize).enumerate() {
            buf[(x + i as u16, y + 3)]
                .set_symbol(&ch.to_string())
                .set_style(st_b);
        }
    }
    let _ = put;
}

/// Joker: wider art with clown + 小王/大王.
fn render_joker(face: CardFace, area: Rect, buf: &mut Buffer) {
    if area.width < 5 || area.height < 4 {
        render_normal(face, area, buf);
        return;
    }
    let is_big = face.card.rank == Rank::RedJoker;
    let bg = if face.selected {
        if is_big {
            Color::Rgb(255, 235, 235)
        } else {
            Color::Rgb(235, 235, 245)
        }
    } else if is_big {
        Color::Rgb(255, 245, 245)
    } else {
        Color::Rgb(245, 245, 255)
    };
    let border = if face.cursor {
        theme::BORDER_FOCUS
    } else if face.selected {
        theme::ACCENT
    } else if is_big {
        INK_RED
    } else {
        Color::Rgb(70, 70, 100)
    };
    let ink = if is_big { INK_RED } else { INK };
    let x = area.x;
    let y = area.y;
    let st_b = Style::default()
        .fg(border)
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    let st_i = Style::default().fg(ink).bg(bg).add_modifier(Modifier::BOLD);

    // 5-col frame
    for (i, ch) in "┌───┐".chars().enumerate() {
        buf[(x + i as u16, y)]
            .set_symbol(&ch.to_string())
            .set_style(st_b);
    }
    // Row1: clown
    buf[(x, y + 1)].set_symbol("│").set_style(st_b);
    buf[(x + 1, y + 1)].set_symbol("🤡").set_style(st_i);
    // clear following cells that emoji may cover
    buf[(x + 2, y + 1)]
        .set_symbol(" ")
        .set_style(Style::default().bg(bg));
    buf[(x + 3, y + 1)]
        .set_symbol(" ")
        .set_style(Style::default().bg(bg));
    buf[(x + 4, y + 1)].set_symbol("│").set_style(st_b);

    // Row2: 大/小
    let label = if is_big { "大" } else { "小" };
    buf[(x, y + 2)].set_symbol("│").set_style(st_b);
    buf[(x + 1, y + 2)].set_symbol(label).set_style(st_i);
    buf[(x + 2, y + 2)].set_symbol("王").set_style(st_i);
    buf[(x + 3, y + 2)]
        .set_symbol(" ")
        .set_style(Style::default().bg(bg));
    buf[(x + 4, y + 2)].set_symbol("│").set_style(st_b);

    for (i, ch) in "└───┘".chars().enumerate() {
        buf[(x + i as u16, y + 3)]
            .set_symbol(&ch.to_string())
            .set_style(st_b);
    }
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
            "—",
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
        let label = if face.is_joker() {
            format!("{}{} ", face.suit_label(), face.rank_label())
        } else {
            format!("{}{} ", face.rank_label(), face.suit_label())
        };
        spans.push(Span::styled(
            label,
            Style::default()
                .fg(face.ink())
                .bg(PAPER)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }
    vec![Line::from(spans)]
}
