//! Lobby + room — quiet list, minimal chrome.

use guandan_core::TeamId;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::theme::{self, ACCENT, BG, MUTED, SURFACE, TEXT};
use crate::app::{App, LobbyFocus};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    // Center a narrow column for calm reading width
    let h = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(12),
            Constraint::Min(14),
            Constraint::Percentage(12),
        ])
        .split(area);

    let col = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(18),
            Constraint::Percentage(64),
            Constraint::Percentage(18),
        ])
        .split(h[1]);

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(col[1]);

    let title = Paragraph::new(vec![
        Line::from(Span::styled(
            "掼蛋",
            Style::default()
                .fg(TEXT)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("Guandan  ·  online {online}", online = app.online),
            Style::default().fg(MUTED).bg(BG),
        )),
    ])
    .alignment(Alignment::Left);
    f.render_widget(title, body[0]);

    let join = if app.input_buf.is_empty() {
        "加入房间 …".to_string()
    } else {
        format!("加入房间  {}▌", app.input_buf)
    };

    let items: [(LobbyFocus, &str); 5] = [
        (LobbyFocus::Practice, "人机练习"),
        (LobbyFocus::Create, "创建房间"),
        (LobbyFocus::Quick, "快速匹配"),
        (LobbyFocus::Join, join.as_str()),
        (LobbyFocus::Help, "规则说明"),
    ];

    let mut lines = Vec::new();
    for (focus, label) in items {
        let on = app.lobby_focus == focus;
        if on {
            lines.push(Line::from(vec![
                Span::styled("  › ", Style::default().fg(ACCENT).bg(SURFACE)),
                Span::styled(
                    format!("{label}  "),
                    Style::default()
                        .fg(TEXT)
                        .bg(SURFACE)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(Span::styled(
                format!("    {label}"),
                Style::default().fg(MUTED).bg(BG),
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  ↑↓  选择    Enter  确认    q  退出",
        Style::default().fg(MUTED).bg(BG),
    )));

    let menu = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER).bg(BG))
            .style(Style::default().bg(BG)),
    );
    f.render_widget(menu, body[1]);

    f.render_widget(
        Paragraph::new(Span::styled(
            "fair shuffle  ·  open rules",
            Style::default().fg(MUTED).bg(BG),
        ))
        .alignment(Alignment::Left),
        body[2],
    );
}

pub fn draw_room(f: &mut Frame, app: &App, area: Rect) {
    let outer = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(area);
    let col = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Min(10),
            Constraint::Percentage(15),
        ])
        .split(outer[1]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::panel_border())
        .style(theme::surface())
        .title(Span::styled(
            format!(" 房间 {} ", app.room_id.as_deref().unwrap_or("—")),
            theme::panel_title(),
        ));
    let inner = block.inner(col[1]);
    f.render_widget(block, col[1]);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  座位  {}", app.my_seat + 1),
            Style::default().fg(ACCENT).bg(SURFACE),
        )),
        Line::from(""),
    ];

    for s in &app.seats {
        let team = match s.team {
            TeamId::A => "A",
            TeamId::B => "B",
        };
        let mark = if s.ready { "●" } else { "○" };
        let bot = if s.is_bot { " bot" } else { "" };
        let you = if s.seat == app.my_seat { "  you" } else { "" };
        lines.push(Line::from(Span::styled(
            format!(
                "  {mark}  #{seat}  [{team}]  {name}{bot}{you}",
                seat = s.seat + 1,
                name = s.name,
            ),
            Style::default().fg(TEXT).bg(SURFACE),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  R  准备      Esc  离开",
        Style::default().fg(MUTED).bg(SURFACE),
    )));

    f.render_widget(Paragraph::new(lines), inner);
}
