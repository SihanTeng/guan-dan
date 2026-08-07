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

记牌器
  C  开/关（本局剩余牌张，不含自己手牌）
  双副 108 · 面值各 8 · 王各 2
  数字=他人手中大致剩余（进贡已计入）
  顶行灰字=点数 · 金色=级牌 · R红=大王
  底行白字=剩余张数 · 暗灰=已出完(0)

计时（固定）
  每回合 30 秒 · 他人出牌展示 3 秒
  本局结束：确认名次 10 秒，超时自动确认
  机器人立刻确认；中途离开由机器人顶上

H / Esc  关闭";
    draw_popup(f, area, "帮助", text);
}

fn draw_match_over(f: &mut Frame, app: &App, area: Rect) {
    // Reuse result board if we have finish order; else compact win screen.
    if !app.result_finish_order.is_empty() {
        crate::ui::game::draw_match_result(f, app, area);
        return;
    }
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
            "胜者  {team}\nA {}  ·  B {}\n\nEnter 确认 / 离开",
            app.team_levels[0].label(),
            app.team_levels[1].label()
        ),
    );
}

pub(crate) fn draw_popup(f: &mut Frame, area: Rect, title: &str, text: &str) {
    // Never size the popup past the available area — ratatui's Clear panics
    // on out-of-bounds cells, and a small terminal pane must not crash us.
    let w = area.width.min(56);
    let h = area.height.min(20);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::net::NetHandle;
    use guandan_core::card::cards_from_codes;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_at(w: u16, h: u16, app: &App) {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
    }

    /// Every screen must render without panicking at any terminal size —
    /// including panes smaller than the popup minimums.
    #[test]
    fn screens_render_at_any_size() {
        let mut app = App::new(NetHandle::dummy());
        app.screen = Screen::Game;
        app.hand = cards_from_codes(&["S3", "H4", "C5", "D6", "S7", "H8", "C9"]);
        app.selected = vec![false; app.hand.len()];
        app.counts = [7, 20, 17, 24];
        app.current = Some(0);
        app.seats = (0..4)
            .map(|seat| guandan_protocol::SeatInfo {
                seat,
                name: format!("玩家{seat}"),
                is_bot: seat != 0,
                ready: true,
                team: if seat % 2 == 0 {
                    guandan_core::TeamId::A
                } else {
                    guandan_core::TeamId::B
                },
            })
            .collect();
        for &(w, h) in &[(80, 24), (120, 40), (40, 12), (30, 10), (20, 6)] {
            render_at(w, h, &app);
        }

        // Popup screens (help / result) on tiny panes used to panic via Clear.
        app.screen = Screen::Help;
        app.prev_screen = Screen::Game;
        for &(w, h) in &[(80, 24), (30, 10), (20, 6), (10, 4)] {
            render_at(w, h, &app);
        }

        app.screen = Screen::HandResult;
        app.result_finish_order = vec![0, 2, 1, 3];
        app.result_ranks = vec![
            guandan_core::FinishRank::Banker,
            guandan_core::FinishRank::Follower,
            guandan_core::FinishRank::Third,
            guandan_core::FinishRank::Dweller,
        ];
        for &(w, h) in &[(80, 24), (30, 10), (20, 6)] {
            render_at(w, h, &app);
        }
    }

    /// Table layout must show the hand (card faces) and seat labels on a
    /// standard 80×24 terminal — the previous Min/Min split squeezed the
    /// hand below CARD_H and cards vanished.
    #[test]
    fn game_table_shows_hand_on_80x24() {
        use crate::app::TrickEntry;
        use guandan_core::{HandType, Rank};
        use guandan_protocol::PublicPlay;

        let mut app = App::new(NetHandle::dummy());
        app.screen = Screen::Game;
        app.status.clear();
        app.my_seat = 0;
        app.hand = cards_from_codes(&[
            "S3", "H3", "C4", "D5", "S6", "H7", "C8", "D9", "S10", "HJ", "CQ", "DK", "SA",
        ]);
        app.selected = vec![false; app.hand.len()];
        app.selected[0] = true;
        app.cursor = 0;
        app.counts = [13, 24, 25, 22];
        app.current = Some(0);
        app.hand_level = Rank::R2;
        app.team_levels = [Rank::R5, Rank::R3];
        app.room_id = Some("ABCD".into());
        app.seats = (0..4)
            .map(|seat| guandan_protocol::SeatInfo {
                seat,
                name: format!("P{seat}"),
                is_bot: seat != 0,
                ready: true,
                team: if seat % 2 == 0 {
                    guandan_core::TeamId::A
                } else {
                    guandan_core::TeamId::B
                },
            })
            .collect();
        app.last_play = Some(PublicPlay {
            seat: 1,
            cards: cards_from_codes(&["S5", "H5"]),
            hand_type: HandType::Pair,
            key: Rank::R5,
        });
        app.trick[1] = Some(TrickEntry {
            cards: cards_from_codes(&["S5", "H5"]),
            hand_type: Some(HandType::Pair),
            pass: false,
        });
        app.trick[2] = Some(TrickEntry {
            cards: vec![],
            hand_type: None,
            pass: true,
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer();

        let mut screen = String::new();
        for y in 0..24u16 {
            for x in 0..80u16 {
                screen.push_str(buf[(x, y)].symbol());
            }
            screen.push('\n');
        }

        // TestBackend stores wide glyphs as char + trailing space, so
        // multi-byte CJK substrings may not match contiguously.
        let compact: String = screen.chars().filter(|c| !c.is_whitespace()).collect();

        // Partner / sides / self labels (table geometry).
        assert!(compact.contains("对家"), "partner label missing:\n{screen}");
        assert!(compact.contains("左"), "left seat missing:\n{screen}");
        assert!(compact.contains("右"), "right seat missing:\n{screen}");
        assert!(compact.contains("你"), "self label missing:\n{screen}");
        // Hand faces must paint (not an empty box).
        assert!(
            screen.contains('┌') && screen.contains('│'),
            "card borders missing:\n{screen}"
        );
        // Rank from hand should appear (3 of spades is selected).
        assert!(compact.contains('3'), "hand rank glyphs missing:\n{screen}");
        // Felt title for play-to-beat.
        assert!(
            compact.contains("要压") || compact.contains("对子"),
            "felt / last-play missing:\n{screen}"
        );
        // Input line is not clobbered by the status toast.
        assert!(
            compact.contains("Enter") || compact.contains("键入"),
            "input line missing:\n{screen}"
        );
    }

    /// 记牌器 panel paints when toggled on without panicking.
    #[test]
    fn game_counter_panel_renders() {
        use guandan_core::{HandType, Rank};
        use guandan_protocol::PublicPlay;

        let mut app = App::new(NetHandle::dummy());
        app.screen = Screen::Game;
        app.status.clear();
        app.show_counter = true;
        app.hand = cards_from_codes(&["S3", "H3", "SK", "HK"]);
        app.selected = vec![false; app.hand.len()];
        app.counts = [4, 24, 25, 22];
        app.current = Some(0);
        app.hand_level = Rank::R2;
        app.seats = (0..4)
            .map(|seat| guandan_protocol::SeatInfo {
                seat,
                name: format!("P{seat}"),
                is_bot: seat != 0,
                ready: true,
                team: if seat % 2 == 0 {
                    guandan_core::TeamId::A
                } else {
                    guandan_core::TeamId::B
                },
            })
            .collect();
        app.counter.note_played(&cards_from_codes(&["CK", "DK"]));
        app.last_play = Some(PublicPlay {
            seat: 1,
            cards: cards_from_codes(&["CK", "DK"]),
            hand_type: HandType::Pair,
            key: Rank::RK,
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer();
        let mut screen = String::new();
        for y in 0..24u16 {
            for x in 0..80u16 {
                screen.push_str(buf[(x, y)].symbol());
            }
            screen.push('\n');
        }
        let compact: String = screen.chars().filter(|c| !c.is_whitespace()).collect();
        // Header row (ranks) + count row must both paint (ASCII-only grid).
        assert!(
            compact.contains('K') && compact.contains('A') && compact.contains('T'),
            "counter rank headers missing:\n{screen}"
        );
        // King remaining after 2 in hand + 2 played = 4.
        assert!(
            compact.contains('4'),
            "counter count cells missing:\n{screen}"
        );

        // Vertical alignment: every rank glyph must share its x with the digit
        // under it (col width 2, centered band).
        let mut hdr_row = None;
        let mut cnt_row = None;
        for y in 0..24u16 {
            let mut line = String::new();
            for x in 0..80u16 {
                line.push_str(buf[(x, y)].symbol());
            }
            if line.contains('K') && line.contains('T') && line.contains('R') {
                hdr_row = Some(y);
            }
            // Count row has digits under the ranks and sits just below headers.
            if let Some(hy) = hdr_row {
                if y == hy + 1 {
                    cnt_row = Some(y);
                }
            }
        }
        let hy = hdr_row.expect("header row");
        let cy = cnt_row.expect("count row under headers");
        for x in 0..80u16 {
            let h = buf[(x, hy)].symbol();
            let c = buf[(x, cy)].symbol();
            let h_rank = matches!(h.chars().next(), Some(ch) if ch.is_ascii_alphanumeric());
            if h_rank {
                let c_digit = matches!(c.chars().next(), Some(ch) if ch.is_ascii_digit());
                assert!(
                    c_digit,
                    "column x={x}: rank '{h}' has no digit under it (got '{c}')\n{screen}"
                );
            }
        }
    }
}
