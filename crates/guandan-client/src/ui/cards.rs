//! Mini playing-card faces for the terminal.

use guandan_core::{Card, Rank, Suit};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::theme::{self, INK, INK_GOLD, INK_RED, PAPER, PAPER_DIM};

/// Outer size of one mini card (including borders).
pub const CARD_W: u16 = 5;
pub const CARD_H: u16 = 4;

#[derive(Clone, Copy)]
pub struct CardFace {
    pub card: Card,
    pub level: Rank,
    pub selected: bool,
    pub cursor: bool,
    pub dim: bool,
}

impl CardFace {
    pub fn ink(self) -> Color {
        if self.card.rank.is_joker() {
            if self.card.rank == Rank::RedJoker {
                INK_RED
            } else {
                INK
            }
        } else if matches!(self.card.suit, Suit::Heart | Suit::Diamond) {
            INK_RED
        } else {
            INK
        }
    }

    /// Single-width-friendly rank for fixed 5-col cards.
    pub fn rank_label(self) -> String {
        match self.card.rank {
            Rank::BlackJoker => "BJ".into(),
            Rank::RedJoker => "RJ".into(),
            Rank::R10 => "10".into(),
            r => r.label().to_string(),
        }
    }

    /// Suit / joker mark (prefer single-cell symbols).
    pub fn suit_label(self) -> &'static str {
        match self.card.rank {
            Rank::BlackJoker => "b",
            Rank::RedJoker => "R",
            _ => self.card.suit.symbol(),
        }
    }

    pub fn is_wild(self) -> bool {
        self.card.is_wild(self.level)
    }
}

impl Widget for CardFace {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < CARD_W || area.height < CARD_H {
            // Fallback single-cell glyph
            if area.width > 0 && area.height > 0 {
                let ch = self.rank_label();
                buf[(area.x, area.y)]
                    .set_symbol(&ch)
                    .set_style(Style::default().fg(self.ink()).bg(PAPER));
            }
            return;
        }

        let bg = if self.dim {
            PAPER_DIM
        } else if self.selected {
            Color::Rgb(255, 248, 200)
        } else {
            PAPER
        };
        let border = if self.cursor {
            theme::CYAN
        } else if self.selected {
            theme::ACCENT
        } else if self.is_wild() {
            INK_GOLD
        } else {
            Color::Rgb(90, 90, 100)
        };
        let ink = self.ink();
        let bold = Modifier::BOLD;

        let x = area.x;
        let y = area.y;

        // Top border
        let top = if self.selected {
            "╔═══╗"
        } else {
            "┌───┐"
        };
        for (i, ch) in top.chars().enumerate() {
            buf[(x + i as u16, y)]
                .set_symbol(&ch.to_string())
                .set_style(Style::default().fg(border).bg(bg).add_modifier(bold));
        }

        // Rank row
        let rank = self.rank_label();
        let rank_pad = if rank.len() == 1 {
            format!("│{rank}  │")
        } else if rank == "10" {
            "│10 │".to_string()
        } else {
            format!("│{rank:<2} │")
        };
        // Ensure width 5
        let rank_line: String = if self.selected {
            if rank == "10" {
                "║10 ║".into()
            } else if rank.chars().count() == 1 {
                format!("║{rank}  ║")
            } else {
                format!("║{rank:<2} ║")
            }
        } else {
            rank_pad
        };
        for (i, ch) in rank_line.chars().take(CARD_W as usize).enumerate() {
            let style = if i == 0 || i == 4 {
                Style::default().fg(border).bg(bg).add_modifier(bold)
            } else {
                Style::default().fg(ink).bg(bg).add_modifier(bold)
            };
            buf[(x + i as u16, y + 1)]
                .set_symbol(&ch.to_string())
                .set_style(style);
        }

        // Suit row (+ wild star)
        let suit = self.suit_label();
        // Keep suit row ASCII-width stable (suits are usually 1 cell; wild uses '*').
        let mid = if self.is_wild() {
            if self.selected {
                format!("║{suit}*║")
            } else {
                format!("│{suit}*│")
            }
        } else if self.selected {
            format!("║ {suit} ║")
        } else {
            format!("│ {suit} │")
        };
        for (i, ch) in mid.chars().take(CARD_W as usize).enumerate() {
            let style = if i == 0 || i == 4 {
                Style::default().fg(border).bg(bg).add_modifier(bold)
            } else {
                Style::default().fg(ink).bg(bg).add_modifier(bold)
            };
            buf[(x + i as u16, y + 2)]
                .set_symbol(&ch.to_string())
                .set_style(style);
        }

        // Bottom
        let bot = if self.selected {
            "╚═══╝"
        } else {
            "└───┘"
        };
        for (i, ch) in bot.chars().enumerate() {
            buf[(x + i as u16, y + 3)]
                .set_symbol(&ch.to_string())
                .set_style(Style::default().fg(border).bg(bg).add_modifier(bold));
        }
    }
}

/// Render a horizontal row of cards (no wrap). Returns width used.
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
        let face = CardFace {
            card,
            level,
            selected,
            cursor,
            dim: false,
        };
        let r = Rect::new(x, area.y, CARD_W, CARD_H.min(area.height));
        face.render(r, buf);
        x = x.saturating_add(CARD_W + gap);
    }
}

/// Wrap cards into multiple rows inside `area`.
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
    let row_h = CARD_H; // no vertical lift to save space for 27 cards
    let mut idx = 0;
    let mut row = 0u16;
    while idx < cards.len() {
        let y = area.y + row * row_h;
        if y + CARD_H > area.y + area.height {
            break;
        }
        let end = (idx + per_row).min(cards.len());
        let slice = &cards[idx..end];
        let row_area = Rect::new(area.x, y, area.width, CARD_H);
        render_card_row(buf, row_area, slice, level, gap);
        idx = end;
        row += 1;
    }
}

/// Compact 2-line strip for played cards in the center of the table.
pub fn card_strip_lines(cards: &[Card], level: Rank) -> Vec<Line<'static>> {
    if cards.is_empty() {
        return vec![Line::from(Span::styled(
            "—",
            Style::default().fg(theme::MUTED),
        ))];
    }
    let mut ranks = Vec::new();
    let mut suits = Vec::new();
    for c in cards {
        let face = CardFace {
            card: *c,
            level,
            selected: false,
            cursor: false,
            dim: false,
        };
        let ink = face.ink();
        let style = Style::default()
            .fg(ink)
            .bg(PAPER)
            .add_modifier(Modifier::BOLD);
        let rl = face.rank_label();
        let pad = if rl.len() >= 2 {
            format!(" {rl} ")
        } else {
            format!(" {rl}  ")
        };
        ranks.push(Span::styled(pad, style));
        ranks.push(Span::raw(" "));
        let sl = if face.is_wild() {
            format!(" {}* ", face.suit_label())
        } else {
            format!(" {}  ", face.suit_label())
        };
        suits.push(Span::styled(sl, style));
        suits.push(Span::raw(" "));
    }
    vec![Line::from(ranks), Line::from(suits)]
}

/// Back-of-card stack indicator for opponents.
pub fn card_backs_line(count: usize) -> Line<'static> {
    let n = count.min(12);
    let mut spans = Vec::new();
    for _ in 0..n {
        spans.push(Span::styled(
            "🂠",
            Style::default().fg(Color::Rgb(40, 70, 140)),
        ));
    }
    if count > 12 {
        spans.push(Span::styled(
            format!("+{}", count - 12),
            Style::default().fg(theme::MUTED),
        ));
    }
    if count == 0 {
        spans.push(Span::styled("空", Style::default().fg(theme::MUTED)));
    }
    Line::from(spans)
}
