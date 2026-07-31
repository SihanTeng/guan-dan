//! Lobby and waiting-room screens.

use guandan_core::TeamId;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::theme::{self, ACCENT, FELT_DARK, MUTED, PAPER};
use crate::app::{App, LobbyFocus};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(10),
            Constraint::Length(4),
        ])
        .margin(1)
        .split(area);

    // Title plate
    let title = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  🥚  掼  蛋  ·  G U A N D A N  ",
            Style::default()
                .fg(FELT_DARK)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  在线 {}  ·  真随机发牌  ·  公平对战  ", app.online),
            theme::muted_on_felt(),
        )),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme::panel_border())
            .style(theme::panel())
            .title(Span::styled(" 大厅 ", theme::panel_title())),
    );
    f.render_widget(title, chunks[0]);

    let join_line = format!(
        "加入房间   Join     {}",
        if app.input_buf.is_empty() {
            "〔输入房号…〕".to_string()
        } else {
            format!("{}▌", app.input_buf)
        }
    );

    let items: [(&str, LobbyFocus, &str); 5] = [
        (
            "①",
            LobbyFocus::Practice,
            "人机练习   Practice  ·  1 人 + 3 Bot",
        ),
        ("②", LobbyFocus::Create, "创建房间   Create room"),
        ("③", LobbyFocus::Quick, "快速匹配   Quick match"),
        ("④", LobbyFocus::Join, join_line.as_str()),
        ("⑤", LobbyFocus::Help, "游戏规则   Rules / Help"),
    ];

    let lines: Vec<Line> = items
        .into_iter()
        .map(|(num, focus, text)| {
            let selected = app.lobby_focus == focus;
            if selected {
                Line::from(vec![
                    Span::styled("  ▸ ", Style::default().fg(ACCENT).bg(FELT_DARK)),
                    Span::styled(
                        format!("{num}  {text}  "),
                        Style::default()
                            .fg(FELT_DARK)
                            .bg(ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled("    ", theme::panel()),
                    Span::styled(
                        format!("{num}  {text}"),
                        Style::default().fg(PAPER).bg(FELT_DARK),
                    ),
                ])
            }
        })
        .collect();

    let mut body = vec![Line::from("")];
    body.extend(lines);
    body.push(Line::from(""));
    body.push(Line::from(Span::styled(
        "  ↑↓ 选择   Enter 确认   H 帮助   Q 退出",
        Style::default().fg(MUTED).bg(FELT_DARK),
    )));

    let menu = Paragraph::new(body).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme::panel_border())
            .style(theme::panel())
            .title(Span::styled(" 菜单 ", theme::panel_title())),
    );
    f.render_widget(menu, chunks[1]);

    let foot = Paragraph::new(vec![Line::from(Span::styled(
        "inspired by fight-the-landlord  ·  MIT  ·  SihanTeng/guan-dan",
        theme::muted_on_felt(),
    ))])
    .alignment(Alignment::Center);
    f.render_widget(foot, chunks[2]);
}

pub fn draw_room(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::panel_border())
        .style(theme::panel())
        .title(Span::styled(
            format!(" 房间 {} ", app.room_id.as_deref().unwrap_or("?")),
            theme::panel_title(),
        ))
        .title_bottom(Span::styled(
            " R 准备  ·  Esc 离开 ",
            Style::default().fg(MUTED).bg(FELT_DARK),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  你的座位  #{}", app.my_seat + 1),
            Style::default()
                .fg(ACCENT)
                .bg(FELT_DARK)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for s in &app.seats {
        let team = match s.team {
            TeamId::A => ("A", theme::CYAN),
            TeamId::B => ("B", theme::DANGER),
        };
        let ready = if s.ready { "●" } else { "○" };
        let ready_style = if s.ready {
            Style::default().fg(theme::ACCENT).bg(FELT_DARK)
        } else {
            Style::default().fg(MUTED).bg(FELT_DARK)
        };
        let bot = if s.is_bot { " 🤖" } else { "" };
        let you = if s.seat == app.my_seat {
            "  ← 你"
        } else {
            ""
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {ready}  "), ready_style),
            Span::styled(
                format!("座位{} ", s.seat + 1),
                Style::default().fg(PAPER).bg(FELT_DARK),
            ),
            Span::styled(
                format!("[{}] ", team.0),
                Style::default()
                    .fg(team.1)
                    .bg(FELT_DARK)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}{}{}", s.name, bot, you),
                Style::default().fg(PAPER).bg(FELT_DARK),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  四人准备后自动开局",
        Style::default().fg(MUTED).bg(FELT_DARK),
    )));

    f.render_widget(Paragraph::new(lines), inner);
}
