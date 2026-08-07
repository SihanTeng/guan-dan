//! Mouse hit-testing — pure layout mirrors of `lobby` / `game` draw code.
//!
//! Regions are recomputed from `(screen, terminal size, app layout state)` so
//! draw stays immutable and clicks always match what was painted.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use super::cards::{CARD_H, CARD_W};
use crate::app::{App, LobbyFocus, Screen};

/// Something the pointer can land on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    LobbyItem(LobbyFocus),
    RoomReady,
    RoomLeave,
    /// Index into `app.hand`.
    Card(usize),
    CounterToggle,
    /// Submit current selection / typed play.
    Play,
    Pass,
    ConfirmResult,
    CloseHelp,
    MatchLeave,
}

/// Point-in-rect, inclusive of the top-left edge (ratatui convention).
#[inline]
pub fn contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x
        && row >= rect.y
        && col < rect.x.saturating_add(rect.width)
        && row < rect.y.saturating_add(rect.height)
}

/// Resolve the topmost interactive target under `(col, row)`.
pub fn hit_test(app: &App, area: Rect, col: u16, row: u16) -> Option<HitTarget> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    match app.screen {
        Screen::Lobby => hit_lobby(app, area, col, row),
        Screen::Room => hit_room(area, col, row),
        Screen::Game => hit_game(app, area, col, row),
        Screen::HandResult => hit_result(area, col, row, false),
        Screen::MatchOver => hit_result(area, col, row, true),
        Screen::Help => hit_help(area, col, row),
    }
}

// ── Lobby ──────────────────────────────────────────────────────────────────

fn lobby_menu_rects(area: Rect) -> (Rect, [Rect; 5]) {
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

    // Menu block has a 1-cell border; items are the first 5 inner rows.
    let menu = body[1];
    let inner = Rect::new(
        menu.x.saturating_add(1),
        menu.y.saturating_add(1),
        menu.width.saturating_sub(2),
        menu.height.saturating_sub(2),
    );
    let mut items = [Rect::default(); 5];
    for (i, r) in items.iter_mut().enumerate() {
        let y = inner.y.saturating_add(i as u16);
        if y < inner.y.saturating_add(inner.height) {
            *r = Rect::new(inner.x, y, inner.width, 1);
        }
    }
    (menu, items)
}

fn hit_lobby(_app: &App, area: Rect, col: u16, row: u16) -> Option<HitTarget> {
    let (_menu, items) = lobby_menu_rects(area);
    let focuses = [
        LobbyFocus::Practice,
        LobbyFocus::Create,
        LobbyFocus::Quick,
        LobbyFocus::Join,
        LobbyFocus::Help,
    ];
    for (rect, focus) in items.iter().zip(focuses) {
        if rect.width > 0 && contains(*rect, col, row) {
            return Some(HitTarget::LobbyItem(focus));
        }
    }
    None
}

// ── Room ───────────────────────────────────────────────────────────────────

fn room_inner(area: Rect) -> Rect {
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
    // bordered panel
    let panel = col[1];
    Rect::new(
        panel.x.saturating_add(1),
        panel.y.saturating_add(1),
        panel.width.saturating_sub(2),
        panel.height.saturating_sub(2),
    )
}

fn hit_room(area: Rect, col: u16, row: u16) -> Option<HitTarget> {
    let inner = room_inner(area);
    if !contains(inner, col, row) {
        // Click outside the room card → leave (Esc equivalent).
        return Some(HitTarget::RoomLeave);
    }
    // Footer lines: blank, seats…, blank, "R  准备  Esc  离开"
    // Treat bottom 2 rows of the panel as Ready; top-right strip as Leave via
    // the whole lower half being Ready for a large click target.
    let ready_y = inner.y.saturating_add(inner.height.saturating_sub(2));
    if row >= ready_y {
        // Left half = ready, right half = leave (matches "R 准备 … Esc 离开").
        let mid = inner.x + inner.width / 2;
        if col < mid {
            return Some(HitTarget::RoomReady);
        }
        return Some(HitTarget::RoomLeave);
    }
    // Click anywhere else in the room card = ready (primary action).
    Some(HitTarget::RoomReady)
}

// ── Game ───────────────────────────────────────────────────────────────────

/// Vertical splits for the game table (mirrors `game::draw`).
fn game_root(app: &App, area: Rect) -> Vec<Rect> {
    let toast = u16::from(!app.status.is_empty());
    let table = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(toast),
    );
    let counter_h = if app.show_counter { 2 } else { 0 };
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),         // status strip
            Constraint::Length(counter_h), // 记牌器
            Constraint::Length(2),         // partner
            Constraint::Fill(1),           // left | felt | right
            Constraint::Length(1),         // self
            Constraint::Fill(3),           // hand
            Constraint::Length(1),         // input
        ])
        .horizontal_margin(1)
        .split(table)
        .to_vec()
}

fn side_panel_width(total: u16) -> u16 {
    let ideal = (total as u32 * 18 / 100) as u16;
    ideal.clamp(12, 16).min(total.saturating_sub(20) / 2)
}

/// Card face rects inside the hand panel (mirrors `HandGrid`).
pub fn hand_card_rects(hand_panel: Rect, n_cards: usize) -> Vec<Rect> {
    if n_cards == 0 || hand_panel.width < 3 || hand_panel.height < 2 {
        return Vec::new();
    }
    // Inner of the bordered hand block.
    let area = Rect::new(
        hand_panel.x.saturating_add(1),
        hand_panel.y.saturating_add(1),
        hand_panel.width.saturating_sub(2),
        hand_panel.height.saturating_sub(2),
    );
    if area.width < CARD_W || area.height < 2 {
        // Compact one-line strip: ~4 cells per label.
        let mut out = Vec::with_capacity(n_cards);
        let mut x = area.x;
        for _ in 0..n_cards {
            if x >= area.x + area.width {
                break;
            }
            let w = 4u16.min(area.x + area.width - x);
            out.push(Rect::new(x, area.y, w, area.height.min(1)));
            x = x.saturating_add(w);
        }
        return out;
    }

    let gap = 0u16;
    let per_row = ((area.width + gap) / (CARD_W + gap)).max(1) as usize;
    let rows = n_cards.div_ceil(per_row).max(1) as u16;
    let used_h = (rows * CARD_H).min(area.height);
    let y_off = if used_h + CARD_H > area.height && rows > 1 {
        0
    } else {
        area.height.saturating_sub(used_h) / 2
    };
    let first = per_row.min(n_cards) as u16;
    let used_w = first * CARD_W;
    let x_off = area.width.saturating_sub(used_w) / 2;

    let grid = Rect::new(
        area.x + x_off,
        area.y + y_off,
        area.width.saturating_sub(x_off),
        area.height.saturating_sub(y_off),
    );

    let mut out = Vec::with_capacity(n_cards);
    let mut idx = 0;
    let mut row = 0u16;
    while idx < n_cards {
        let y = grid.y + row * CARD_H;
        if y + CARD_H > grid.y + grid.height && row > 0 {
            // Partial last row only if at least one full cell fits.
            if y >= grid.y + grid.height {
                break;
            }
        }
        if y >= grid.y + grid.height {
            break;
        }
        let h = CARD_H.min(grid.y + grid.height - y);
        let end = (idx + per_row).min(n_cards);
        let mut x = grid.x;
        for _ in idx..end {
            if x + CARD_W > grid.x + grid.width {
                break;
            }
            out.push(Rect::new(x, y, CARD_W, h));
            x = x.saturating_add(CARD_W + gap);
        }
        idx = end;
        row += 1;
    }
    out
}

fn hit_game(app: &App, area: Rect, col: u16, row: u16) -> Option<HitTarget> {
    let root = game_root(app, area);
    if root.len() < 7 {
        return None;
    }

    // Status strip: counter badge lives after room id — treat a mid-right
    // band (cols ~40%–70%) as the toggle so we don't steal the turn timer.
    let strip = root[0];
    if contains(strip, col, row) {
        let left = strip.x + strip.width * 2 / 5;
        let right = strip.x + strip.width * 3 / 4;
        if col >= left && col < right {
            return Some(HitTarget::CounterToggle);
        }
    }

    // Click the 记牌器 strip itself to toggle off.
    if app.show_counter && root[1].height > 0 && contains(root[1], col, row) {
        return Some(HitTarget::CounterToggle);
    }

    // Hand cards (primary interaction).
    let cards = hand_card_rects(root[5], app.hand.len());
    for (i, r) in cards.iter().enumerate() {
        if contains(*r, col, row) {
            return Some(HitTarget::Card(i));
        }
    }

    // Input line: left ≈ play, right ≈ pass (large targets).
    let input = root[6];
    if contains(input, col, row) {
        let mid = input.x + input.width / 2;
        if col < mid {
            return Some(HitTarget::Play);
        }
        return Some(HitTarget::Pass);
    }

    // Felt (center of mid band): click to play current selection.
    let mid_band = root[3];
    let side_w = side_panel_width(mid_band.width);
    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(side_w),
            Constraint::Min(16),
            Constraint::Length(side_w),
        ])
        .split(mid_band);
    if mid.len() >= 2 && contains(mid[1], col, row) {
        return Some(HitTarget::Play);
    }

    None
}

// ── Overlays ───────────────────────────────────────────────────────────────

fn popup_rect(area: Rect) -> Rect {
    let w = area.width.min(56);
    let h = area.height.min(20);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn hit_help(area: Rect, col: u16, row: u16) -> Option<HitTarget> {
    // Click anywhere (popup or dimmed backdrop) closes help.
    let _ = (col, row, area);
    Some(HitTarget::CloseHelp)
}

fn hit_result(area: Rect, col: u16, row: u16, match_over: bool) -> Option<HitTarget> {
    let pop = popup_rect(area);
    if contains(pop, col, row) {
        return Some(if match_over {
            // Bottom half of popup = confirm / leave; top = still confirm.
            HitTarget::ConfirmResult
        } else {
            HitTarget::ConfirmResult
        });
    }
    // Outside popup on match-over: leave to lobby.
    if match_over {
        return Some(HitTarget::MatchLeave);
    }
    // Outside hand-result popup: still allow confirm via the primary action.
    Some(HitTarget::ConfirmResult)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::NetHandle;
    use guandan_core::card::cards_from_codes;

    fn game_app(n: usize) -> App {
        let mut app = App::new(NetHandle::dummy());
        app.screen = Screen::Game;
        app.status.clear();
        let codes: Vec<&str> = [
            "S3", "H3", "C4", "D5", "S6", "H7", "C8", "D9", "S10", "HJ", "CQ", "DK", "SA", "H2",
            "C2",
        ]
        .into_iter()
        .take(n)
        .collect();
        app.hand = cards_from_codes(&codes);
        app.selected = vec![false; app.hand.len()];
        app.counts = [n, 20, 20, 20];
        app.current = Some(0);
        app.my_seat = 0;
        app
    }

    #[test]
    fn lobby_items_are_clickable() {
        let app = App::new(NetHandle::dummy());
        let area = Rect::new(0, 0, 80, 24);
        let (_menu, items) = lobby_menu_rects(area);
        // First item should resolve to Practice.
        let r = items[0];
        assert!(r.width > 0 && r.height > 0, "practice row empty");
        assert_eq!(
            hit_test(&app, area, r.x + 2, r.y),
            Some(HitTarget::LobbyItem(LobbyFocus::Practice))
        );
        let r = items[3];
        assert_eq!(
            hit_test(&app, area, r.x + 2, r.y),
            Some(HitTarget::LobbyItem(LobbyFocus::Join))
        );
    }

    #[test]
    fn hand_cards_hit_on_80x24() {
        let app = game_app(13);
        let area = Rect::new(0, 0, 80, 24);
        let root = game_root(&app, area);
        let cards = hand_card_rects(root[5], app.hand.len());
        assert!(
            cards.len() >= 10,
            "expected most cards hit-testable, got {}",
            cards.len()
        );
        let r = cards[0];
        assert_eq!(
            hit_test(&app, area, r.x + 1, r.y + 1),
            Some(HitTarget::Card(0))
        );
        let r = cards[5];
        assert_eq!(
            hit_test(&app, area, r.x + 1, r.y + 1),
            Some(HitTarget::Card(5))
        );
    }

    #[test]
    fn help_click_closes() {
        let mut app = App::new(NetHandle::dummy());
        app.screen = Screen::Help;
        let area = Rect::new(0, 0, 80, 24);
        assert_eq!(hit_test(&app, area, 40, 12), Some(HitTarget::CloseHelp));
    }

    #[test]
    fn contains_edges() {
        let r = Rect::new(10, 5, 4, 2);
        assert!(contains(r, 10, 5));
        assert!(contains(r, 13, 6));
        assert!(!contains(r, 14, 5));
        assert!(!contains(r, 10, 7));
    }
}
