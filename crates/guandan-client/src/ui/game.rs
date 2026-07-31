//! In-game layout — flat hierarchy, one focus at a time.

use guandan_core::TeamId;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use ratatui::Frame;

use super::cards::{self, card_strip_lines, render_card_grid, CARD_H};
use super::theme::{self, ACCENT, BG, MUTED, SURFACE, TEXT, TURN};
use crate::app::{hand_type_cn, App};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status strip (no box)
            Constraint::Length(1), // spacer
            Constraint::Length(2), // opponents row
            Constraint::Length(1),
            Constraint::Length(4), // last play
            Constraint::Length(1),
            Constraint::Min(5),    // hand
            Constraint::Length(2), // input / hints
        ])
        .margin(1)
        .split(area);

    draw_status_strip(f, app, root[0]);
    draw_opponents(f, app, root[2]);
    draw_play(f, app, root[4]);
    draw_hand(f, app, root[6]);
    draw_input(f, app, root[7]);

    if app.screen == crate::app::Screen::HandResult {
        super::draw_popup(
            f,
            area,
            "本局",
            &format!("{}\n\nEnter 继续", app.last_hand_result),
        );
    }
}

fn draw_status_strip(f: &mut Frame, app: &App, area: Rect) {
    let turn = match app.current {
        Some(s) if s == app.my_seat => Span::styled(
            "  your turn  ",
            Style::default()
                .fg(BG)
                .bg(TURN)
                .add_modifier(Modifier::BOLD),
        ),
        Some(s) => Span::styled(
            format!("  wait {}  ", app.seat_name(s)),
            Style::default().fg(MUTED).bg(BG),
        ),
        None => Span::raw(""),
    };

    let line = Line::from(vec![
        Span::styled(" 级 ", Style::default().fg(MUTED).bg(BG)),
        Span::styled(
            format!("{} ", app.hand_level.label()),
            Style::default()
                .fg(TEXT)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " A:{}  B:{}  ",
                app.team_levels[0].label(),
                app.team_levels[1].label()
            ),
            Style::default().fg(MUTED).bg(BG),
        ),
        Span::styled(
            format!("{}  ", app.room_id.as_deref().unwrap_or("")),
            Style::default().fg(MUTED).bg(BG),
        ),
        turn,
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_opponents(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(44),
            Constraint::Percentage(28),
        ])
        .split(area);

    seat_line(f, app, cols[0], app.relative_seat(1), "左");
    seat_line(f, app, cols[1], app.relative_seat(2), "对家");
    seat_line(f, app, cols[2], app.relative_seat(3), "右");
}

fn seat_line(f: &mut Frame, app: &App, area: Rect, seat: usize, label: &str) {
    let count = app.counts.get(seat).copied().unwrap_or(0);
    let active = app.current == Some(seat);
    let name = app.seat_name(seat);
    let team = app
        .seats
        .iter()
        .find(|s| s.seat == seat)
        .map(|s| match s.team {
            TeamId::A => "A",
            TeamId::B => "B",
        })
        .unwrap_or("-");

    let style = if active {
        Style::default()
            .fg(ACCENT)
            .bg(BG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(MUTED).bg(BG)
    };

    let text = format!("{label}  {name}  [{team}]  ·  {count}");
    f.render_widget(
        Paragraph::new(Span::styled(text, style)).alignment(Alignment::Center),
        area,
    );
}

fn draw_play(f: &mut Frame, app: &App, area: Rect) {
    let is_my = app.current == Some(app.my_seat);
    let border = if is_my {
        theme::active_border()
    } else {
        theme::panel_border()
    };

    let title = if let Some(ref lp) = app.last_play {
        format!(
            " {} · {} ",
            app.seat_name(lp.seat),
            hand_type_cn(lp.hand_type)
        )
    } else {
        " 出牌 ".into()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .style(theme::surface())
        .title(Span::styled(title, theme::panel_title()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(ref lp) = app.last_play {
        let lines = card_strip_lines(&lp.cards, app.hand_level);
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    } else {
        f.render_widget(
            Paragraph::new(Span::styled("—", Style::default().fg(MUTED).bg(SURFACE)))
                .alignment(Alignment::Center),
            inner,
        );
    }
}

fn draw_hand(f: &mut Frame, app: &App, area: Rect) {
    let n = app.hand.len();
    let sel = app.selected.iter().filter(|s| **s).count();
    let title = if app.tribute_mode {
        format!(" 回贡  ·  {n} ")
    } else {
        format!(" 手牌  {n}  ·  选 {sel} ")
    };

    let is_my = app.current == Some(app.my_seat);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_my {
            theme::active_border()
        } else {
            theme::panel_border()
        })
        .style(theme::surface())
        .title(Span::styled(title, theme::panel_title()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.hand.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("—", Style::default().fg(MUTED).bg(SURFACE)))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let cards: Vec<_> = app
        .hand
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                *c,
                app.selected.get(i).copied().unwrap_or(false),
                i == app.cursor,
            )
        })
        .collect();

    f.render_widget(
        HandGrid {
            cards,
            level: app.hand_level,
        },
        inner,
    );
}

struct HandGrid {
    cards: Vec<(guandan_core::Card, bool, bool)>,
    level: guandan_core::Rank,
}

impl Widget for HandGrid {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let gap = 0u16;
        let per_row = ((area.width + gap) / (cards::CARD_W + gap)).max(1) as usize;
        let rows = self.cards.len().div_ceil(per_row).max(1) as u16;
        let used_h = rows * CARD_H;
        let y_off = area.height.saturating_sub(used_h) / 2;
        let first = per_row.min(self.cards.len()) as u16;
        let used_w = first * cards::CARD_W;
        let x_off = area.width.saturating_sub(used_w) / 2;
        render_card_grid(
            buf,
            Rect::new(
                area.x + x_off,
                area.y + y_off,
                area.width.saturating_sub(x_off),
                area.height.saturating_sub(y_off),
            ),
            &self.cards,
            self.level,
            gap,
        );
    }
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let line = if !app.play_buf.is_empty() {
        Line::from(vec![
            Span::styled("  › ", Style::default().fg(ACCENT).bg(BG)),
            Span::styled(
                format!("{}▌", app.play_buf),
                Style::default()
                    .fg(TEXT)
                    .bg(BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "   Enter 出牌   ⌫ 删   Esc 清空",
                Style::default().fg(MUTED).bg(BG),
            ),
        ])
    } else {
        Line::from(Span::styled(
            "  键入点数 34567 / KK    Enter 出    P 过    ←→ Space 点选    H 帮助",
            Style::default().fg(MUTED).bg(BG),
        ))
    };
    f.render_widget(Paragraph::new(line), area);
}
