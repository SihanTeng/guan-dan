//! Ratatui views for lobby / room / table.

use guandan_core::TeamId;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{hand_type_cn, App, LobbyFocus, Screen};

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    match app.screen {
        Screen::Lobby => draw_lobby(f, app, area),
        Screen::Room => draw_room(f, app, area),
        Screen::Game | Screen::HandResult => draw_game(f, app, area),
        Screen::Help => {
            draw_game_or_lobby_bg(f, app, area);
            draw_help(f, area);
        }
        Screen::MatchOver => draw_match_over(f, app, area),
    }
    if !app.status.is_empty() {
        draw_status(f, app, area);
    }
}

fn draw_game_or_lobby_bg(f: &mut Frame, app: &App, area: Rect) {
    match app.prev_screen {
        Screen::Game | Screen::HandResult => draw_game(f, app, area),
        Screen::Room => draw_room(f, app, area),
        _ => draw_lobby(f, app, area),
    }
}

fn draw_lobby(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    let title = Paragraph::new(vec![
        Line::from(Span::styled(
            "掼 蛋  GUANDAN",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "在线 Online: {}  |  ↑↓ 选择  Enter 确认  H 帮助  Q 退出",
            app.online
        )),
    ])
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL).title("大厅 Lobby"));
    f.render_widget(title, chunks[0]);

    let items = [
        (
            LobbyFocus::Practice,
            "1. 人机练习  Practice (1 human + 3 bots)",
        ),
        (LobbyFocus::Create, "2. 创建房间  Create room"),
        (LobbyFocus::Quick, "3. 快速匹配  Quick match"),
        (
            LobbyFocus::Join,
            &format!("4. 加入房间  Join room: {}_", app.input_buf) as &str,
        ),
        (LobbyFocus::Help, "5. 游戏规则  Rules / Help"),
    ];
    // Fix join string ownership
    let join_line = format!("4. 加入房间  Join room: {}_", app.input_buf);
    let lines: Vec<Line> = [
        (
            LobbyFocus::Practice,
            "1. 人机练习  Practice (1 human + 3 bots)",
        ),
        (LobbyFocus::Create, "2. 创建房间  Create room"),
        (LobbyFocus::Quick, "3. 快速匹配  Quick match"),
        (LobbyFocus::Join, join_line.as_str()),
        (LobbyFocus::Help, "5. 游戏规则  Rules / Help"),
    ]
    .into_iter()
    .map(|(focus, text)| {
        let style = if app.lobby_focus == focus {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        Line::from(Span::styled(format!("  {text}"), style))
    })
    .collect();

    let menu =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("菜单 Menu"));
    f.render_widget(menu, chunks[1]);

    let hint = Paragraph::new(
        "灵感来自 fight-the-landlord · 真随机发牌 · 公平对战\nInspired by ddz ref · fair shuffle · no card control",
    )
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint, chunks[2]);

    let _ = items;
}

fn draw_room(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(format!(
        "房间 Room {}  —  R 准备 Ready  Esc 离开",
        app.room_id.as_deref().unwrap_or("?")
    ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![Line::from(format!("你的座位 Seat: {}", app.my_seat + 1))];
    for s in &app.seats {
        let team = match s.team {
            TeamId::A => "A",
            TeamId::B => "B",
        };
        let ready = if s.ready { "✓" } else { "…" };
        let bot = if s.is_bot { " [BOT]" } else { "" };
        let you = if s.seat == app.my_seat { " ←你" } else { "" };
        lines.push(Line::from(format!(
            "  座位{} 队{team} {ready} {}{bot}{you}",
            s.seat + 1,
            s.name
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(
        "四人准备后自动开局 · Game starts when all ready",
    ));
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_game(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);

    // Header: levels
    let header = Paragraph::new(format!(
        "级牌 Level: {}   队A: {}   队B: {}   房间 {}",
        app.hand_level.label(),
        app.team_levels[0].label(),
        app.team_levels[1].label(),
        app.room_id.as_deref().unwrap_or("-")
    ))
    .block(Block::default().borders(Borders::ALL).title("掼蛋 · 对局"));
    f.render_widget(header, chunks[0]);

    // Opponents layout: partner top, left/right
    let partner = app.relative_seat(2);
    let left = app.relative_seat(1);
    let right = app.relative_seat(3);
    let opp = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .split(chunks[1]);

    f.render_widget(seat_box(app, left, "左"), opp[0]);
    f.render_widget(seat_box(app, partner, "对家 Partner"), opp[1]);
    f.render_widget(seat_box(app, right, "右"), opp[2]);

    // Last play
    let last = if let Some(ref lp) = app.last_play {
        format!(
            "上家/出牌 Last: 座位{} {} {}",
            lp.seat + 1,
            hand_type_cn(lp.hand_type),
            lp.cards
                .iter()
                .map(|c| c.display_with_level(app.hand_level))
                .collect::<Vec<_>>()
                .join(" ")
        )
    } else {
        "自由出牌 Lead free".into()
    };
    let turn = match app.current {
        Some(s) if s == app.my_seat => "【轮到你 YOUR TURN】".to_string(),
        Some(s) => format!("等待 座位{} …", s + 1),
        None => String::new(),
    };
    f.render_widget(
        Paragraph::new(vec![Line::from(last), Line::from(turn)])
            .block(Block::default().borders(Borders::ALL).title("出牌区")),
        chunks[2],
    );

    // Hand
    let hand_line = render_hand_line(app);
    f.render_widget(
        Paragraph::new(hand_line).block(Block::default().borders(Borders::ALL).title(format!(
            "你的手牌 Your hand ({})  ←→ 选  Space 标记  点数键  Enter 出  P 不出  C 记牌  H 帮助",
            app.hand.len()
        ))),
        chunks[3],
    );

    // Footer / counter
    let footer = if app.show_counter {
        format!(
            "记牌器 counts: S1={} S2={} S3={} S4={}",
            app.counts[0], app.counts[1], app.counts[2], app.counts[3]
        )
    } else {
        "C 开关记牌器 · Space 选牌 · 点数键 3-9 T/0 J Q K A 2 B R".into()
    };
    f.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
        chunks[4],
    );

    if app.screen == Screen::HandResult {
        draw_popup(
            f,
            area,
            "本局结果 Hand Result",
            &format!("{}\n\nEnter 继续下一局", app.last_hand_result),
        );
    }
}

fn seat_box<'a>(app: &'a App, seat: usize, label: &'a str) -> Paragraph<'a> {
    let name = app.seat_name(seat);
    let count = app.counts.get(seat).copied().unwrap_or(0);
    let turn = if app.current == Some(seat) {
        " ★"
    } else {
        ""
    };
    let team = app
        .seats
        .iter()
        .find(|s| s.seat == seat)
        .map(|s| match s.team {
            TeamId::A => "A",
            TeamId::B => "B",
        })
        .unwrap_or("?");
    Paragraph::new(format!("{label}\n{name} 队{team}{turn}\n剩 {count}"))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL))
}

fn render_hand_line(app: &App) -> Line<'_> {
    if app.hand.is_empty() {
        return Line::from("(空 empty)");
    }
    let mut spans = Vec::new();
    for (i, card) in app.hand.iter().enumerate() {
        let selected = app.selected.get(i).copied().unwrap_or(false);
        let cursor = i == app.cursor;
        let label = card.display_with_level(app.hand_level);
        let text = if cursor {
            format!("[{label}] ")
        } else {
            format!("{label} ")
        };
        let mut style = if card.suit == guandan_core::Suit::Heart
            || card.suit == guandan_core::Suit::Diamond
            || card.rank == guandan_core::Rank::RedJoker
        {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::White)
        };
        if selected {
            style = style.bg(Color::Blue).add_modifier(Modifier::BOLD);
        }
        if cursor {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        spans.push(Span::styled(text, style));
    }
    Line::from(spans)
}

fn draw_help(f: &mut Frame, area: Rect) {
    let text = "\
【掼蛋规则摘要 Guandan Rules】\n\
• 4 人两队 (对家一队)，两副牌 108 张，每人 27 张\n\
• 级牌：当前打的点数，大于 A 小于王；红心级牌为逢人配(万能)\n\
• 牌型：单、对、三张、三带二、顺子(5)、三连对、钢板、炸弹、同花顺、天王炸\n\
• 炸弹可压普通牌型；天王炸(四王)最大\n\
• 头游+二游 +3 级，+三游 +2，+下游 +1；先打到 A 再胜一局获胜\n\
• 进贡：下游给上游最大牌(非逢人配)，上游回 ≤10 的牌\n\
\n\
【按键 Keys】\n\
←→ 移动  Space 选/取消  点数键 选同点  Enter 出牌  P 不出\n\
C 记牌器  H 帮助  Esc 返回\n\
\n\
按 H / Esc 关闭";
    draw_popup(f, area, "帮助 Help", text);
}

fn draw_match_over(f: &mut Frame, app: &App, area: Rect) {
    let team = match app.winner_team {
        Some(TeamId::A) => "队 A",
        Some(TeamId::B) => "队 B",
        None => "?",
    };
    draw_popup(
        f,
        area,
        "比赛结束 Match Over",
        &format!(
            "胜者 Winner: {team}\n等级 Levels: A={} B={}\n\nEnter 返回大厅",
            app.team_levels[0].label(),
            app.team_levels[1].label()
        ),
    );
}

fn draw_popup(f: &mut Frame, area: Rect, title: &str, text: &str) {
    let w = area.width.clamp(40, 70);
    let h = area.height.clamp(10, 20);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect::new(x, y, w, h);
    f.render_widget(Clear, rect);
    let p = Paragraph::new(text).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(p, rect);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let y = area.y + area.height.saturating_sub(1);
    let rect = Rect::new(area.x, y, area.width, 1);
    f.render_widget(
        Paragraph::new(app.status.as_str()).style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        rect,
    );
}
