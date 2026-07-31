//! In-game felt table layout.

use guandan_core::TeamId;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use ratatui::Frame;

use super::cards::{self, card_backs_line, card_strip_lines, render_card_grid, CARD_H};
use super::theme::{self, ACCENT, FELT_DARK, MUTED, PAPER, TURN_GLOW};
use crate::app::{hand_type_cn, App};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(5), // partner
            Constraint::Length(8), // left | play | right
            Constraint::Min(6),    // hand
            Constraint::Length(2), // footer
        ])
        .split(area);

    draw_header(f, app, chunks[0]);
    draw_partner(f, app, chunks[1]);
    draw_middle(f, app, chunks[2]);
    draw_hand(f, app, chunks[3]);
    draw_footer(f, app, chunks[4]);

    if app.screen == crate::app::Screen::HandResult {
        super::draw_popup(
            f,
            area,
            " 本局结果 ",
            &format!("{}\n\nEnter 继续下一局", app.last_hand_result),
        );
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let my_team = app
        .seats
        .iter()
        .find(|s| s.seat == app.my_seat)
        .map(|s| s.team)
        .unwrap_or(TeamId::A);
    let team_label = match my_team {
        TeamId::A => "队A",
        TeamId::B => "队B",
    };

    let turn = match app.current {
        Some(s) if s == app.my_seat => Span::styled(
            "  ★ 轮到你  ",
            Style::default()
                .fg(FELT_DARK)
                .bg(TURN_GLOW)
                .add_modifier(Modifier::BOLD),
        ),
        Some(s) => Span::styled(
            format!("  等待 {}  ", app.seat_name(s)),
            Style::default().fg(MUTED).bg(FELT_DARK),
        ),
        None => Span::raw(""),
    };

    let line = Line::from(vec![
        Span::styled(" 级牌 ", Style::default().fg(MUTED).bg(FELT_DARK)),
        Span::styled(
            format!(" {} ", app.hand_level.label()),
            Style::default()
                .fg(FELT_DARK)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", theme::panel()),
        Span::styled("A ", Style::default().fg(theme::CYAN).bg(FELT_DARK)),
        Span::styled(
            format!("{} ", app.team_levels[0].label()),
            Style::default().fg(PAPER).bg(FELT_DARK),
        ),
        Span::styled("B ", Style::default().fg(theme::DANGER).bg(FELT_DARK)),
        Span::styled(
            format!("{} ", app.team_levels[1].label()),
            Style::default().fg(PAPER).bg(FELT_DARK),
        ),
        Span::styled(
            format!(" 你:{team_label}  "),
            Style::default().fg(MUTED).bg(FELT_DARK),
        ),
        Span::styled(
            format!(" {} ", app.room_id.as_deref().unwrap_or("-")),
            Style::default().fg(MUTED).bg(FELT_DARK),
        ),
        Span::raw("  "),
        turn,
    ]);

    f.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::panel_border())
                .style(theme::panel())
                .title(Span::styled(" 掼蛋 ", theme::panel_title())),
        ),
        area,
    );
}

fn draw_partner(f: &mut Frame, app: &App, area: Rect) {
    let seat = app.relative_seat(2);
    let block = seat_block(app, seat, "对家 · Partner", true);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let count = app.counts.get(seat).copied().unwrap_or(0);
    let lines = vec![
        Line::from(Span::styled(
            format!("  {}  ", app.seat_name(seat)),
            Style::default()
                .fg(PAPER)
                .bg(FELT_DARK)
                .add_modifier(Modifier::BOLD),
        )),
        card_backs_line(count),
        Line::from(Span::styled(
            format!("  剩 {count} 张  "),
            Style::default().fg(MUTED).bg(FELT_DARK),
        )),
    ];
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

fn draw_middle(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(22),
            Constraint::Percentage(56),
            Constraint::Percentage(22),
        ])
        .split(area);

    draw_side_seat(f, app, cols[0], app.relative_seat(1), "左 · Left");
    draw_play_area(f, app, cols[1]);
    draw_side_seat(f, app, cols[2], app.relative_seat(3), "右 · Right");
}

fn draw_side_seat(f: &mut Frame, app: &App, area: Rect, seat: usize, label: &str) {
    let block = seat_block(app, seat, label, false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let count = app.counts.get(seat).copied().unwrap_or(0);
    let team = team_badge(app, seat);
    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", app.seat_name(seat)),
            Style::default().fg(PAPER).bg(FELT_DARK),
        )),
        Line::from(team),
        card_backs_line(count.min(6)),
        Line::from(Span::styled(
            format!(" 剩{count}"),
            Style::default().fg(MUTED).bg(FELT_DARK),
        )),
    ];
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

fn draw_play_area(f: &mut Frame, app: &App, area: Rect) {
    let is_my_turn = app.current == Some(app.my_seat);
    let border = if is_my_turn {
        theme::active_border()
    } else {
        theme::panel_border()
    };

    let title = if let Some(ref lp) = app.last_play {
        format!(
            " 出牌 · {} · {} ",
            app.seat_name(lp.seat),
            hand_type_cn(lp.hand_type)
        )
    } else {
        " 出牌区 · 自由出牌 ".into()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .style(theme::panel())
        .title(Span::styled(title, theme::panel_title()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(ref lp) = app.last_play {
        let mut lines = card_strip_lines(&lp.cards, app.hand_level);
        lines.insert(
            0,
            Line::from(Span::styled(
                format!("  {}  ", hand_type_cn(lp.hand_type)),
                Style::default()
                    .fg(ACCENT)
                    .bg(FELT_DARK)
                    .add_modifier(Modifier::BOLD),
            )),
        );
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    } else {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  等待首出…  ",
                    Style::default().fg(MUTED).bg(FELT_DARK),
                )),
            ])
            .alignment(Alignment::Center),
            inner,
        );
    }
}

fn draw_hand(f: &mut Frame, app: &App, area: Rect) {
    let n = app.hand.len();
    let sel: usize = app.selected.iter().filter(|s| **s).count();
    let title = if app.tribute_mode {
        format!(" 回贡 · 选 1 张 ≤10 的牌  ({n}) ")
    } else {
        format!(" 我的手牌  {n} 张 · 已选 {sel} ")
    };

    let is_my = app.current == Some(app.my_seat);
    let border = if is_my {
        theme::active_border()
    } else {
        theme::panel_border()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .style(theme::panel())
        .title(Span::styled(title, theme::panel_title()))
        .title_bottom(Span::styled(
            " ←→ 移动  Space 选  点数键  Enter 出  P 不出 ",
            Style::default().fg(MUTED).bg(FELT_DARK),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.hand.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "（已出完）",
                Style::default().fg(MUTED).bg(FELT_DARK),
            ))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Custom widget for card grid
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

    let widget = HandGrid {
        cards,
        level: app.hand_level,
    };
    f.render_widget(widget, inner);
}

struct HandGrid {
    cards: Vec<(guandan_core::Card, bool, bool)>,
    level: guandan_core::Rank,
}

impl Widget for HandGrid {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center horizontally if few cards
        let gap = 0u16;
        let per_row = ((area.width + gap) / (cards::CARD_W + gap)).max(1) as usize;
        let rows = self.cards.len().div_ceil(per_row).max(1) as u16;
        let used_h = rows * CARD_H;
        let y_off = area.height.saturating_sub(used_h) / 2;

        let first_row_n = per_row.min(self.cards.len());
        let used_w = first_row_n as u16 * cards::CARD_W;
        let x_off = area.width.saturating_sub(used_w) / 2;

        let grid = Rect::new(
            area.x + x_off,
            area.y + y_off,
            area.width.saturating_sub(x_off),
            area.height.saturating_sub(y_off),
        );
        render_card_grid(buf, grid, &self.cards, self.level, gap);
    }
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let text = if app.show_counter {
        format!(
            " 记牌  #1:{}  #2:{}  #3:{}  #4:{}   (C 关闭)",
            app.counts[0], app.counts[1], app.counts[2], app.counts[3]
        )
    } else {
        " C 记牌器  ·  H 帮助  ·  点数 3-9 T J Q K A 2 B R  ·  红心级牌 = 逢人配 ★ ".into()
    };
    f.render_widget(
        Paragraph::new(Span::styled(text, theme::muted_on_felt())).alignment(Alignment::Center),
        area,
    );
}

fn seat_block<'a>(app: &'a App, seat: usize, label: &'a str, _wide: bool) -> Block<'a> {
    let active = app.current == Some(seat);
    let border = if active {
        theme::active_border()
    } else {
        theme::panel_border()
    };
    let mark = if active { " ● " } else { " " };
    Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .style(theme::panel())
        .title(Span::styled(
            format!("{mark}{label}{mark}"),
            if active {
                Style::default()
                    .fg(FELT_DARK)
                    .bg(TURN_GLOW)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme::panel_title()
            },
        ))
}

fn team_badge(app: &App, seat: usize) -> Span<'static> {
    let (label, color) = app
        .seats
        .iter()
        .find(|s| s.seat == seat)
        .map(|s| match s.team {
            TeamId::A => ("队A", theme::CYAN),
            TeamId::B => ("队B", theme::DANGER),
        })
        .unwrap_or(("?", MUTED));
    Span::styled(
        format!(" {label} "),
        Style::default()
            .fg(color)
            .bg(FELT_DARK)
            .add_modifier(Modifier::BOLD),
    )
}
