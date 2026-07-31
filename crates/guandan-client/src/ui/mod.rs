//! Minimal TUI — charcoal canvas, soft accent, low visual load.
//!
//! Hallmark · genre: modern-minimal · tone: austere
//! Pre-emit: hierarchy clean · restraint high · no neon floods

mod cards;
mod game;
mod lobby;
mod theme;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Screen};
use theme::{ACCENT, BG, MUTED, SURFACE, TEXT};

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG)), area);

    match app.screen {
        Screen::Lobby => lobby::draw(f, app, area),
        Screen::Room => lobby::draw_room(f, app, area),
        Screen::Game | Screen::HandResult => game::draw(f, app, area),
        Screen::Help => {
            draw_bg(f, app, area);
            draw_help(f, area);
        }
        Screen::MatchOver => {
            draw_bg(f, app, area);
            draw_match_over(f, app, area);
        }
    }
    if !app.status.is_empty() {
        draw_status(f, app, area);
    }
}

fn draw_bg(f: &mut Frame, app: &App, area: Rect) {
    match app.prev_screen {
        Screen::Game | Screen::HandResult => game::draw(f, app, area),
        Screen::Room => lobby::draw_room(f, app, area),
        _ => lobby::draw(f, app, area),
    }
}

fn draw_help(f: &mut Frame, area: Rect) {
    let text = "\
规则
  4 人两队 · 108 张 · 每人 27
  级牌 > A · 红心级牌为逢人配 *
  单 / 对 / 三 / 三带二 / 顺子 / 三连对 / 钢板
  炸弹 4·5 → 同花顺 → 6+ → 天王炸

出牌
  键入点数  34567  KK  3334  BR
  Enter 出  P 过  ⌫ 删  Esc 清空
  ←→ Space 点选

计时（固定）
  每回合 30 秒（超时自动过 / 首出最小牌）
  他人出牌展示 3 秒

H / Esc  关闭";
    draw_popup(f, area, "帮助", text);
}

fn draw_match_over(f: &mut Frame, app: &App, area: Rect) {
    let team = match app.winner_team {
        Some(guandan_core::TeamId::A) => "队 A",
        Some(guandan_core::TeamId::B) => "队 B",
        None => "—",
    };
    draw_popup(
        f,
        area,
        "结束",
        &format!(
            "胜者  {team}\nA {}  ·  B {}\n\nEnter 返回",
            app.team_levels[0].label(),
            app.team_levels[1].label()
        ),
    );
}

pub(crate) fn draw_popup(f: &mut Frame, area: Rect, title: &str, text: &str) {
    let w = area.width.clamp(36, 56);
    let h = area.height.clamp(12, 20);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect::new(x, y, w, h);
    f.render_widget(Clear, rect);
    let p = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(TEXT).bg(SURFACE))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER).bg(SURFACE))
                .title(format!(" {title} "))
                .title_style(
                    Style::default()
                        .fg(ACCENT)
                        .bg(SURFACE)
                        .add_modifier(Modifier::BOLD),
                )
                .style(Style::default().bg(SURFACE)),
        );
    f.render_widget(p, rect);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let y = area.y + area.height.saturating_sub(1);
    let rect = Rect::new(area.x, y, area.width, 1);
    f.render_widget(
        Paragraph::new(format!("  {}  ", app.status)).style(Style::default().fg(MUTED).bg(SURFACE)),
        rect,
    );
}
