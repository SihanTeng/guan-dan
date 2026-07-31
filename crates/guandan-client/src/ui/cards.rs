//! Compact card faces — density without visual noise.

use guandan_core::{Card, Rank, Suit};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::theme::{self, INK, INK_RED, PAPER, PAPER_SEL, WILD};

/// Compact card: 3 wide × 3 tall (lighter than 5×4 boxes).
pub const CARD_W: u16 = 4;
pub const CARD_H: u16 = 3;

#[derive(Clone, Copy)]
pub struct CardFace {
    pub card: Card,
    pub level: Rank,
    pub selected: bool,
    pub cursor: bool,
}

impl CardFace {
    pub fn ink(self) -> ratatui::style::Color {
        if self.card.rank == Rank::RedJoker || matches!(self.card.suit, Suit::Heart | Suit::Diamond)
        {
            INK_RED
        } else {
            INK
        }
    }

    pub fn rank_label(self) -> String {
        match self.card.rank {
            Rank::BlackJoker => "Bj".into(),
            Rank::RedJoker => "Rj".into(),
            Rank::R10 => "10".into(),
            r => r.label().to_string(),
        }
    }

    pub fn suit_label(self) -> &'static str {
        match self.card.rank {
            Rank::BlackJoker | Rank::RedJoker => "★",
            _ => self.card.suit.symbol(),
        }
    }

    pub fn is_wild(self) -> bool {
        self.card.is_wild(self.level)
    }
}

impl Widget for CardFace {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 3 || area.height < 2 {
            return;
        }

        let bg = if self.selected { PAPER_SEL } else { PAPER };
        let border = if self.cursor {
            theme::BORDER_FOCUS
        } else if self.selected {
            theme::ACCENT
        } else if self.is_wild() {
            WILD
        } else {
            theme::BORDER
        };
        let ink = self.ink();
        let x = area.x;
        let y = area.y;
        let w = CARD_W.min(area.width);
        let h = CARD_H.min(area.height);

        // Simple light frame — no double-line chrome
        let top = match w {
            4 => "┌──┐",
            3 => "┌─┐",
            _ => "┌┐",
        };
        let bot = match w {
            4 => "└──┘",
            3 => "└─┘",
            _ => "└┘",
        };

        for (i, ch) in top.chars().take(w as usize).enumerate() {
            buf[(x + i as u16, y)]
                .set_symbol(&ch.to_string())
                .set_style(Style::default().fg(border).bg(bg));
        }

        if h >= 2 {
            let rank = self.rank_label();
            let body = if w >= 4 {
                if rank == "10" {
                    "│10│".to_string()
                } else if rank.chars().count() == 1 {
                    let s = self.suit_label();
                    // rank + suit on one line when possible
                    format!("│{rank}{s}│")
                } else {
                    format!("│{rank:<2}│")
                }
            } else {
                format!("│{rank}│")
            };
            for (i, ch) in body.chars().take(w as usize).enumerate() {
                let edge = i == 0 || i + 1 == w as usize;
                let style = if edge {
                    Style::default().fg(border).bg(bg)
                } else {
                    Style::default().fg(ink).bg(bg).add_modifier(Modifier::BOLD)
                };
                buf[(x + i as u16, y + 1)]
                    .set_symbol(&ch.to_string())
                    .set_style(style);
            }
        }

        if h >= 3 {
            for (i, ch) in bot.chars().take(w as usize).enumerate() {
                buf[(x + i as u16, y + 2)]
                    .set_symbol(&ch.to_string())
                    .set_style(Style::default().fg(border).bg(bg));
            }
            // Wild marker in bottom-left inner if room — already on rank line for suits
            if self.is_wild() && w >= 4 {
                // soft mark on bottom border center
                buf[(x + 1, y + 2)]
                    .set_symbol("*")
                    .set_style(Style::default().fg(WILD).bg(bg));
            }
            if self.cursor && w >= 4 {
                buf[(x + 2, y + 2)]
                    .set_symbol("·")
                    .set_style(Style::default().fg(theme::BORDER_FOCUS).bg(bg));
            }
        }
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

/// Compact strip for last play — rank+suit pairs, no heavy chrome.
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
        let label = format!("{}{} ", face.rank_label(), face.suit_label());
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
