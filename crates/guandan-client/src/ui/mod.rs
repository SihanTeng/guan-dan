//! Ratatui views: felt table, mini card faces, polished lobby.

mod cards;
mod game;
mod lobby;
mod theme;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Screen};
use theme::{ACCENT, INK, PAPER};

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    // Subtle table felt fill
    f.render_widget(
        Block::default().style(Style::default().bg(theme::FELT)),
        area,
    );

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
【掼蛋规则摘要】\n\
• 4 人两队（对家一队），两副牌 108 张，每人 27 张\n\
• 级牌大于 A、小于王；红心级牌 = 逢人配（万能）★\n\
• 牌型：单 / 对 / 三张 / 三带二 / 顺子(5) / 三连对 / 钢板\n\
• 炸弹：4·5 张 → 同花顺 → 6+ → 天王炸（四王最大）\n\
• 升级：头+二 +3 · 头+三 +2 · 头+下 +1；A 级再胜一局获胜\n\
• 进贡：下游最大牌（非逢人配）→ 上游回 ≤10\n\
\n\
【按键 · 出牌】\n\
键入点数序列（34567 / KK / 3334 / BR）后 Enter 出牌\n\
←→ 光标  Space 点选  Backspace 删  Esc 清空\n\
P 不出  C 记牌器  H 帮助\n\
\n\
按 H / Esc 关闭";
    draw_popup(f, area, " 📖 帮助 ", text);
}

fn draw_match_over(f: &mut Frame, app: &App, area: Rect) {
    let team = match app.winner_team {
        Some(guandan_core::TeamId::A) => "队 A",
        Some(guandan_core::TeamId::B) => "队 B",
        None => "?",
    };
    draw_popup(
        f,
        area,
        " 🏆 比赛结束 ",
        &format!(
            "胜者  {team}\n\
             等级  A={} · B={}\n\n\
             Enter 返回大厅",
            app.team_levels[0].label(),
            app.team_levels[1].label()
        ),
    );
}

pub(crate) fn draw_popup(f: &mut Frame, area: Rect, title: &str, text: &str) {
    let w = area.width.clamp(42, 72);
    let h = area.height.clamp(12, 22);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect::new(x, y, w, h);
    f.render_widget(Clear, rect);
    let p = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(INK).bg(PAPER))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT).bg(PAPER))
                .title(title)
                .title_style(
                    Style::default()
                        .fg(ACCENT)
                        .bg(PAPER)
                        .add_modifier(Modifier::BOLD),
                )
                .style(Style::default().bg(PAPER)),
        );
    f.render_widget(p, rect);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let y = area.y + area.height.saturating_sub(1);
    let rect = Rect::new(area.x, y, area.width, 1);
    f.render_widget(
        Paragraph::new(format!("  {}  ", app.status)).style(
            Style::default()
                .fg(theme::FELT_DARK)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        rect,
    );
}
