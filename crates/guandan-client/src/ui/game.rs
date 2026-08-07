//! In-game layout — a real card table.
//!
//! ```text
//!   status strip
//!           [partner chip]
//!           [partner play]
//!   [left]  |   felt    |  [right]
//!           [self chip + play]
//!   [ hand ……………………………… ]
//!   input / hints
//! ```
//!
//! Vertical budget targets 24 rows (common terminal): fixed chrome ≤ 7,
//! mid ≥ 5, hand ≥ 6 so a full card face always fits.

use guandan_core::{Seat, TeamId};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use ratatui::Frame;

use super::cards::{self, card_strip_lines, render_card_grid, CARD_H, CARD_W};
use super::theme::{self, ACCENT, BG, MUTED, SURFACE, TEXT, TURN};
use crate::app::{hand_type_cn, App, TrickEntry};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    // Leave the last row free when a global status toast will paint over it
    // (see ui::draw_status). Without this, the input line gets clobbered.
    let toast = u16::from(!app.status.is_empty());
    let table = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(toast),
    );

    // Vertical budget (24-row terminal, no toast):
    //   status 1 + partner 3 + self 2 + input 1 = 7 fixed
    //   remaining shared by mid (Fill 1) and hand (Fill 2)
    //   so the hand grows faster and can show a second card row.
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // match status strip
            Constraint::Length(3), // partner (name + play)
            Constraint::Fill(1),   // left | felt | right
            Constraint::Length(2), // self
            Constraint::Fill(2),   // hand — prefer space here
            Constraint::Length(1), // input
        ])
        .horizontal_margin(1)
        .split(table);

    draw_status_strip(f, app, root[0]);
    draw_partner(f, app, root[1]);

    let side_w = side_panel_width(root[2].width);
    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(side_w),
            Constraint::Min(16),
            Constraint::Length(side_w),
        ])
        .split(root[2]);
    draw_side_seat(f, app, mid[0], app.relative_seat(1), "左");
    draw_felt(f, app, mid[1]);
    draw_side_seat(f, app, mid[2], app.relative_seat(3), "右");

    draw_self_entry(f, app, root[3]);
    draw_hand(f, app, root[4]);
    draw_input(f, app, root[5]);

    if app.screen == crate::app::Screen::HandResult {
        draw_result_board(f, app, area, false);
    }
}

/// Side panels: ~22% each, clamped so the felt keeps a usable middle.
fn side_panel_width(total: u16) -> u16 {
    let ideal = (total as u32 * 22 / 100) as u16;
    ideal.clamp(14, 22).min(total.saturating_sub(16) / 2)
}

fn rank_cn(r: guandan_core::FinishRank) -> &'static str {
    match r {
        guandan_core::FinishRank::Banker => "上游",
        guandan_core::FinishRank::Follower => "二游",
        guandan_core::FinishRank::Third => "三游",
        guandan_core::FinishRank::Dweller => "下游",
    }
}

pub fn draw_match_result(f: &mut Frame, app: &App, area: Rect) {
    draw_result_board(f, app, area, true);
}

fn draw_result_board(f: &mut Frame, app: &App, area: Rect, match_over: bool) {
    let mut body = String::new();
    body.push_str(&format!("{}\n\n", app.last_hand_result));
    body.push_str("名次\n");
    for (i, seat) in app.result_finish_order.iter().enumerate() {
        let r = app
            .result_ranks
            .get(i)
            .copied()
            .unwrap_or(guandan_core::FinishRank::Dweller);
        let conf = if app.result_confirmed.get(*seat).copied().unwrap_or(false) {
            "✓"
        } else {
            "…"
        };
        let you = if *seat == app.my_seat { " ←你" } else { "" };
        body.push_str(&format!(
            "  {}  座位{}  {}{}  [{conf}]\n",
            rank_cn(r),
            seat + 1,
            app.seat_name(*seat),
            you
        ));
    }
    body.push('\n');
    let n = app.result_confirmed.iter().filter(|c| **c).count();
    if app.my_result_confirmed {
        body.push_str(&format!("已确认  {n}/4  ·  等待其他人…"));
    } else {
        body.push_str("Enter 确认本局名次  (四人全部确认后开下一局)");
    }
    if match_over {
        body.push_str("\n比赛结束");
    }
    super::draw_popup(f, area, "本局结果 · 确认名次", &body);
}

fn draw_status_strip(f: &mut Frame, app: &App, area: Rect) {
    let timer = app
        .turn_secs_left()
        .map(|s| format!("  {s:2}s  "))
        .unwrap_or_else(|| "  --  ".into());

    let turn = match app.current {
        Some(s) if s == app.my_seat => Span::styled(
            format!("  轮到你 {timer}"),
            Style::default()
                .fg(BG)
                .bg(TURN)
                .add_modifier(Modifier::BOLD),
        ),
        Some(s) => Span::styled(
            format!("  {} {} ", app.seat_name(s), timer.trim()),
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

/// Name · team · remaining · finish rank. Compact single-line chip.
fn seat_label(app: &App, seat: Seat) -> String {
    let count = app.counts.get(seat).copied().unwrap_or(0);
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
    let out = app
        .finish_order
        .iter()
        .find(|(s, _)| *s == seat)
        .map(|(_, r)| format!("·{}", rank_cn(*r)))
        .unwrap_or_default();
    format!("{name} [{team}] 余{count}{out}")
}

fn seat_name_style(app: &App, seat: Seat) -> Style {
    if app.current == Some(seat) {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT)
    }
}

/// Latest action for a seat: pass mark, or card strip + type.
fn trick_content(app: &App, seat: Seat) -> Vec<Line<'static>> {
    match app.trick.get(seat).and_then(|t| t.as_ref()) {
        Some(e) if e.pass => vec![Line::from(Span::styled("不出", Style::default().fg(MUTED)))],
        Some(e) => trick_play_lines(e, app.hand_level),
        None => Vec::new(),
    }
}

fn trick_play_lines(e: &TrickEntry, level: guandan_core::Rank) -> Vec<Line<'static>> {
    let mut lines = card_strip_lines(&e.cards, level);
    if let Some(ty) = e.hand_type {
        lines.push(Line::from(Span::styled(
            hand_type_cn(ty),
            Style::default().fg(MUTED),
        )));
    }
    lines
}

/// Partner across the top: chip centered, play under it.
fn draw_partner(f: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let seat = app.relative_seat(2);
    let active = app.current == Some(seat);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let chip = Line::from(vec![
        Span::styled("对家 ", Style::default().fg(MUTED)),
        Span::styled(seat_label(app, seat), seat_name_style(app, seat)),
    ]);
    let chip_style = if active {
        Style::default().bg(SURFACE)
    } else {
        Style::default().bg(BG)
    };
    f.render_widget(
        Paragraph::new(chip)
            .alignment(Alignment::Center)
            .style(chip_style),
        rows[0],
    );

    let play = trick_content(app, seat);
    if !play.is_empty() {
        f.render_widget(Paragraph::new(play).alignment(Alignment::Center), rows[1]);
    }
}

/// Left / right opponent: bordered panel, name on top, play below.
fn draw_side_seat(f: &mut Frame, app: &App, area: Rect, seat: Seat, label: &str) {
    if area.width < 3 || area.height < 2 {
        return;
    }
    let active = app.current == Some(seat);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if active {
            theme::active_border()
        } else {
            theme::panel_border()
        })
        .style(theme::surface())
        .title(Span::styled(format!(" {label} "), theme::panel_title()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let play = trick_content(app, seat);
    let mut body: Vec<Line> = vec![Line::from(Span::styled(
        seat_label(app, seat),
        seat_name_style(app, seat),
    ))];
    if !play.is_empty() {
        body.push(Line::from(""));
        body.extend(play);
    } else {
        // Idle seat: show a quiet placeholder so the panel isn't blank.
        body.push(Line::from(""));
        body.push(Line::from(Span::styled("·", Style::default().fg(MUTED))));
    }

    // Vertically center the chip + play block inside the panel.
    let used = body.len() as u16;
    let pad = inner.height.saturating_sub(used) / 2;
    let mut lines = Vec::with_capacity((pad + used) as usize);
    for _ in 0..pad {
        lines.push(Line::from(""));
    }
    lines.extend(body);
    f.render_widget(Paragraph::new(lines), inner);
}

/// Center felt: the play to beat (last_play), with optional mini faces.
fn draw_felt(f: &mut Frame, app: &App, area: Rect) {
    if area.width < 3 || area.height < 2 {
        return;
    }
    let is_my = app.current == Some(app.my_seat);
    let revealing = app.revealing();
    let border = if revealing {
        Style::default().fg(ACCENT).bg(SURFACE)
    } else if is_my {
        theme::active_border()
    } else {
        theme::panel_border()
    };

    let title = if let Some(ref lp) = app.last_play {
        let hold = if revealing {
            let left = app
                .reveal_until
                .map(|t| {
                    t.saturating_duration_since(std::time::Instant::now())
                        .as_secs()
                })
                .unwrap_or(0);
            format!(" · 展示{left}s")
        } else {
            String::new()
        };
        format!(
            " 要压 · {} · {}{} ",
            app.seat_name(lp.seat),
            hand_type_cn(lp.hand_type),
            hold
        )
    } else if app.must_lead {
        " 自由出牌 ".into()
    } else {
        " 牌桌 ".into()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .style(theme::surface())
        .title(Span::styled(
            title,
            if revealing {
                Style::default()
                    .fg(ACCENT)
                    .bg(SURFACE)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme::panel_title()
            },
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    if let Some(ref lp) = app.last_play {
        // Prefer real card faces when the felt is tall enough.
        if inner.height >= CARD_H && inner.width >= CARD_W {
            let cards: Vec<_> = lp.cards.iter().map(|c| (*c, false, false)).collect();
            f.render_widget(
                FeltCards {
                    cards,
                    level: app.hand_level,
                },
                inner,
            );
        } else {
            let lines = card_strip_lines(&lp.cards, app.hand_level);
            f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
        }
    } else {
        f.render_widget(
            Paragraph::new(Span::styled("—", Style::default().fg(MUTED).bg(SURFACE)))
                .alignment(Alignment::Center),
            inner,
        );
    }
}

/// Centered card faces for the felt (no selection chrome).
struct FeltCards {
    cards: Vec<(guandan_core::Card, bool, bool)>,
    level: guandan_core::Rank,
}

impl Widget for FeltCards {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.cards.is_empty() || area.height < CARD_H || area.width < CARD_W {
            return;
        }
        let n = self.cards.len() as u16;
        let gap = 0u16;
        let used_w = n * CARD_W + n.saturating_sub(1) * gap;
        // If cards overflow, fall back to a single strip line in-place.
        if used_w > area.width {
            let labels: Vec<Span> = self
                .cards
                .iter()
                .map(|(c, _, _)| {
                    let face = cards::CardFace {
                        card: *c,
                        level: self.level,
                        selected: false,
                        cursor: false,
                    };
                    Span::styled(
                        format!("{} ", face.strip_label()),
                        Style::default()
                            .fg(face.ink())
                            .bg(theme::PAPER)
                            .add_modifier(Modifier::BOLD),
                    )
                })
                .collect();
            Paragraph::new(Line::from(labels))
                .alignment(Alignment::Center)
                .render(area, buf);
            return;
        }
        let x_off = area.width.saturating_sub(used_w) / 2;
        let y_off = area.height.saturating_sub(CARD_H) / 2;
        cards::render_card_row(
            buf,
            Rect::new(area.x + x_off, area.y + y_off, used_w, CARD_H),
            &self.cards,
            self.level,
            gap,
        );
    }
}

/// Your row under the felt: seat chip + latest action, one or two lines.
fn draw_self_entry(f: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let seat = app.my_seat;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let chip = Line::from(vec![
        Span::styled("你 ", Style::default().fg(MUTED)),
        Span::styled(seat_label(app, seat), seat_name_style(app, seat)),
    ]);
    f.render_widget(Paragraph::new(chip).alignment(Alignment::Center), rows[0]);

    if rows[1].height == 0 {
        return;
    }
    let play = trick_content(app, seat);
    if !play.is_empty() {
        f.render_widget(Paragraph::new(play).alignment(Alignment::Center), rows[1]);
    }
}

fn draw_hand(f: &mut Frame, app: &App, area: Rect) {
    if area.width < 3 || area.height < 2 {
        return;
    }
    let n = app.hand.len();
    let sel = app.selected.iter().filter(|s| **s).count();
    let title = if app.tribute_mode {
        format!(" 回贡 · {n} ")
    } else {
        format!(" 手牌 {n} · 选 {sel} ")
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
        if area.height < 2 || area.width < CARD_W {
            // Extremely tight: one-line strip of ranks.
            let labels: Vec<Span> = self
                .cards
                .iter()
                .map(|(c, sel, cur)| {
                    let face = cards::CardFace {
                        card: *c,
                        level: self.level,
                        selected: *sel,
                        cursor: *cur,
                    };
                    let style = if *cur {
                        Style::default()
                            .fg(face.ink())
                            .bg(theme::PAPER_SEL)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                    } else if *sel {
                        Style::default()
                            .fg(face.ink())
                            .bg(theme::PAPER_SEL)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(face.ink())
                            .bg(theme::PAPER)
                            .add_modifier(Modifier::BOLD)
                    };
                    Span::styled(format!("{} ", face.strip_label()), style)
                })
                .collect();
            Paragraph::new(Line::from(labels)).render(area, buf);
            return;
        }
        let gap = 0u16;
        let per_row = ((area.width + gap) / (CARD_W + gap)).max(1) as usize;
        let rows = self.cards.len().div_ceil(per_row).max(1) as u16;
        let used_h = (rows * CARD_H).min(area.height);
        // Prefer top alignment when multi-row won't fit fully, so the first
        // ranks stay visible; center only when everything fits.
        let y_off = if used_h + CARD_H > area.height && rows > 1 {
            0
        } else {
            area.height.saturating_sub(used_h) / 2
        };
        let first = per_row.min(self.cards.len()) as u16;
        let used_w = first * CARD_W;
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
    if area.height == 0 {
        return;
    }
    let line = if app.no_legal_play && app.current == Some(app.my_seat) {
        Line::from(Span::styled(
            "  ⚠ 无牌可出 — 按 P 过",
            Style::default()
                .fg(BG)
                .bg(theme::INK_RED)
                .add_modifier(Modifier::BOLD),
        ))
    } else if !app.play_buf.is_empty() {
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
            "  键入 34567 / KK    Enter 出    P 过    ←→ Space 点选    H 帮助",
            Style::default().fg(MUTED).bg(BG),
        ))
    };
    f.render_widget(Paragraph::new(line), area);
}
