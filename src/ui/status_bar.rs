use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::session::{group_by_project, Session, SessionState};

pub struct StatusBar<'a> {
    sessions: &'a [Session],
    active_index: Option<usize>,
}

impl<'a> StatusBar<'a> {
    pub fn new(sessions: &'a [Session], active_index: Option<usize>) -> Self {
        Self { sessions, active_index }
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 { return; }

        let bg_style = Style::default().bg(Color::Rgb(30, 30, 40));
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
                cell.set_symbol(" ");
                cell.set_style(bg_style);
            }
        }

        let f12_hint = "\u{2317} F12";
        let f12_hint_width = f12_hint.width();
        let tab_budget = (area.width as usize).saturating_sub(f12_hint_width + 1);

        let groups = group_by_project(self.sessions);
        let mut session_order: Vec<usize> = Vec::new();
        let mut group_boundaries: Vec<usize> = Vec::new();
        for group in &groups {
            group_boundaries.push(session_order.len());
            session_order.extend(&group.sessions);
        }

        let show_name_for = mark_named_tabs(&groups, &session_order, self.active_index);

        // Pre-compute tab widths (including group separator)
        let tab_widths: Vec<usize> = session_order.iter().enumerate().map(|(pos, &sess_idx)| {
            let session = &self.sessions[sess_idx];
            let fkey = format!("F{}", pos + 1);
            let icon = session.state.bar_icon();
            let tab_w = if show_name_for.get(pos).copied().unwrap_or(true) {
                format!(" {} {} {} ", fkey, icon, session.name).width()
            } else {
                format!(" {} {} ", fkey, icon).width()
            };
            let sep_w = if pos > 0 && group_boundaries.contains(&pos) { 1 } else { 0 };
            sep_w + tab_w
        }).collect();

        let total: usize = tab_widths.iter().sum();
        let active_pos = self.active_index
            .and_then(|ai| session_order.iter().position(|&si| si == ai));

        // Find the visible window [start..end) that includes the active tab
        let (vis_start, vis_end) = if total <= tab_budget {
            (0, session_order.len())
        } else {
            find_visible_window(&tab_widths, tab_budget, active_pos)
        };

        let hidden_before = vis_start;
        let hidden_after = session_order.len() - vis_end;

        // Render
        let mut x = area.x;
        let max_tab_x = area.x + tab_budget as u16;
        let overflow_style = Style::default()
            .fg(Color::Rgb(120, 120, 140))
            .bg(Color::Rgb(30, 30, 40));

        if hidden_before > 0 {
            let text = format!(" +{}\u{2039} ", hidden_before);
            x = write_str(buf, x, area.y, &text, overflow_style, max_tab_x);
        }

        for (pos, &sess_idx) in session_order.iter().enumerate().take(vis_end).skip(vis_start) {
            let session = &self.sessions[sess_idx];

            if pos > 0 && group_boundaries.contains(&pos)
                && (pos > vis_start || hidden_before == 0) {
                    let sep_style = Style::default()
                        .fg(Color::Rgb(60, 60, 80))
                        .bg(Color::Rgb(30, 30, 40));
                    x = write_str(buf, x, area.y, "\u{2502}", sep_style, max_tab_x);
                }

            let fkey = format!("F{}", pos + 1);
            let icon = session.state.bar_icon();
            let tab_text = if show_name_for.get(pos).copied().unwrap_or(true) {
                format!(" {} {} {} ", fkey, icon, session.name)
            } else {
                format!(" {} {} ", fkey, icon)
            };

            let is_active = self.active_index == Some(sess_idx);
            let style = tab_style(session.state, is_active);
            x = write_str(buf, x, area.y, &tab_text, style, max_tab_x);
        }

        if hidden_after > 0 {
            let text = format!(" \u{203A}+{} ", hidden_after);
            write_str(buf, x, area.y, &text, overflow_style, max_tab_x);
        }

        let hint_x = area.x + area.width - f12_hint_width as u16;
        let hint_style = Style::default()
            .fg(Color::Rgb(150, 150, 170))
            .bg(Color::Rgb(30, 30, 40));
        write_str(buf, hint_x, area.y, f12_hint, hint_style, area.x + area.width);
    }
}

/// Result of a status bar hit-test.
pub enum TabHit {
    /// Clicked on a visible tab (flat session position).
    Tab(usize),
    /// Clicked the left overflow indicator — scroll to show earlier tabs.
    ScrollLeft,
    /// Clicked the right overflow indicator — scroll to show later tabs.
    ScrollRight,
    /// Clicked the F12/dashboard indicator.
    Dashboard,
}

/// Hit-test: given a click x-coordinate on the status bar, return what was clicked.
pub fn tab_at_x(
    sessions: &[Session],
    active_index: Option<usize>,
    area: Rect,
    click_x: u16,
) -> Option<TabHit> {
    if area.height == 0 || sessions.is_empty() { return None; }

    let f12_hint = "\u{2317} F12";
    let f12_hint_width = f12_hint.width();
    let tab_budget = (area.width as usize).saturating_sub(f12_hint_width + 1);

    let groups = group_by_project(sessions);
    let mut session_order: Vec<usize> = Vec::new();
    let mut group_boundaries: Vec<usize> = Vec::new();
    for group in &groups {
        group_boundaries.push(session_order.len());
        session_order.extend(&group.sessions);
    }

    let show_name_for = mark_named_tabs(&groups, &session_order, active_index);

    let tab_widths: Vec<usize> = session_order.iter().enumerate().map(|(pos, &sess_idx)| {
        let session = &sessions[sess_idx];
        let fkey = format!("F{}", pos + 1);
        let icon = session.state.bar_icon();
        let tab_w = if show_name_for.get(pos).copied().unwrap_or(true) {
            format!(" {} {} {} ", fkey, icon, session.name).width()
        } else {
            format!(" {} {} ", fkey, icon).width()
        };
        let sep_w = if pos > 0 && group_boundaries.contains(&pos) { 1 } else { 0 };
        sep_w + tab_w
    }).collect();

    let total: usize = tab_widths.iter().sum();
    let active_pos = active_index
        .and_then(|ai| session_order.iter().position(|&si| si == ai));

    let (vis_start, vis_end) = if total <= tab_budget {
        (0, session_order.len())
    } else {
        find_visible_window(&tab_widths, tab_budget, active_pos)
    };

    let hidden_before = vis_start;
    let hidden_after = session_order.len() - vis_end;

    let mut x = area.x;
    let max_tab_x = area.x + tab_budget as u16;

    if hidden_before > 0 {
        let text = format!(" +{}\u{2039} ", hidden_before);
        let indicator_end = x + text.width() as u16;
        if click_x >= x && click_x < indicator_end {
            return Some(TabHit::ScrollLeft);
        }
        x = indicator_end;
    }

    for (pos, &sess_idx) in session_order.iter().enumerate().take(vis_end).skip(vis_start) {
        let session = &sessions[sess_idx];

        let mut tab_start = x;
        if pos > 0 && group_boundaries.contains(&pos)
            && (pos > vis_start || hidden_before == 0) {
                tab_start += 1; // separator
            }

        let fkey = format!("F{}", pos + 1);
        let icon = session.state.bar_icon();
        let tab_text = if show_name_for.get(pos).copied().unwrap_or(true) {
            format!(" {} {} {} ", fkey, icon, session.name)
        } else {
            format!(" {} {} ", fkey, icon)
        };
        let tab_w = tab_text.width() as u16;
        let tab_end = (tab_start + tab_w).min(max_tab_x);

        if click_x >= tab_start && click_x < tab_end {
            return Some(TabHit::Tab(pos));
        }

        x = tab_start + tab_w;
    }

    if hidden_after > 0 && click_x >= x && click_x < max_tab_x {
        return Some(TabHit::ScrollRight);
    }

    // F12 dashboard hint at the right edge
    let hint_x = area.x + area.width - f12_hint_width as u16;
    if click_x >= hint_x && click_x < area.x + area.width {
        return Some(TabHit::Dashboard);
    }

    None
}

/// Returns the visible window (start, end) of tab positions for the current state.
pub fn tab_visible_range(
    sessions: &[Session],
    active_index: Option<usize>,
    area: Rect,
) -> (usize, usize) {
    if area.height == 0 || sessions.is_empty() { return (0, 0); }

    let f12_hint = "\u{2317} F12";
    let f12_hint_width = f12_hint.width();
    let tab_budget = (area.width as usize).saturating_sub(f12_hint_width + 1);

    let groups = group_by_project(sessions);
    let mut session_order: Vec<usize> = Vec::new();
    let mut group_boundaries: Vec<usize> = Vec::new();
    for group in &groups {
        group_boundaries.push(session_order.len());
        session_order.extend(&group.sessions);
    }

    let show_name_for = mark_named_tabs(&groups, &session_order, active_index);

    let tab_widths: Vec<usize> = session_order.iter().enumerate().map(|(pos, &sess_idx)| {
        let session = &sessions[sess_idx];
        let fkey = format!("F{}", pos + 1);
        let icon = session.state.bar_icon();
        let tab_w = if show_name_for.get(pos).copied().unwrap_or(true) {
            format!(" {} {} {} ", fkey, icon, session.name).width()
        } else {
            format!(" {} {} ", fkey, icon).width()
        };
        let sep_w = if pos > 0 && group_boundaries.contains(&pos) { 1 } else { 0 };
        sep_w + tab_w
    }).collect();

    let total: usize = tab_widths.iter().sum();
    let active_pos = active_index
        .and_then(|ai| session_order.iter().position(|&si| si == ai));

    if total <= tab_budget {
        (0, session_order.len())
    } else {
        find_visible_window(&tab_widths, tab_budget, active_pos)
    }
}

fn write_str(buf: &mut Buffer, mut x: u16, y: u16, text: &str, style: Style, max_x: u16) -> u16 {
    for ch in text.chars() {
        if x >= max_x { break; }
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
        if let Some(cell) = buf.cell_mut(Position { x, y }) {
            cell.set_symbol(&ch.to_string());
            cell.set_style(style);
        }
        x += cw as u16;
    }
    x
}

/// Decide which tabs show their session name: singleton groups always show
/// theirs; multi-session groups show only the active tab's, or the last
/// tab's when the group has no active session.
fn mark_named_tabs(
    groups: &[crate::session::ProjectGroup],
    session_order: &[usize],
    active_index: Option<usize>,
) -> Vec<bool> {
    let mut show_name_for = vec![false; session_order.len()];
    let mut group_start = 0;
    for group in groups {
        let group_end = (group_start + group.sessions.len()).min(session_order.len());
        let group_slice = &session_order[group_start..group_end];
        let active_in_group = group_slice.iter().any(|&si| active_index == Some(si));
        if group.sessions.len() <= 1 {
            if group_start < show_name_for.len() {
                show_name_for[group_start] = true;
            }
        } else if active_in_group {
            for (offset, &si) in group_slice.iter().enumerate() {
                if active_index == Some(si) {
                    show_name_for[group_start + offset] = true;
                }
            }
        } else if group_end > 0 {
            show_name_for[group_end - 1] = true;
        }
        group_start = group_end;
    }
    show_name_for
}

/// Find the visible window [start..end) that fits within `budget` and includes `active_pos`.
/// Keeps the window scrolled as far left as possible while the active tab remains visible.
fn find_visible_window(widths: &[usize], budget: usize, active_pos: Option<usize>) -> (usize, usize) {
    let n = widths.len();
    let active = active_pos.unwrap_or(0);
    let left_indicator_max = format!(" +{}\u{2039} ", n).width();
    let right_indicator_max = format!(" \u{203A}+{} ", n).width();

    // Check whether tabs [start..=active] fit within the budget
    let active_fits = |start: usize| -> bool {
        let left_cost = if start > 0 { left_indicator_max } else { 0 };
        let right_cost = if active + 1 < n { right_indicator_max } else { 0 };
        let available = budget.saturating_sub(left_cost + right_cost);
        let used: usize = widths[start..=active].iter().sum();
        used <= available
    };

    // Find the smallest start where the active tab fits
    let mut start = 0;
    while start <= active && !active_fits(start) {
        start += 1;
    }

    // Now greedily expand end past active to fill remaining space
    let left_cost = if start > 0 { left_indicator_max } else { 0 };
    let mut used: usize = widths[start..=active].iter().sum();
    let mut end = active + 1;
    while end < n {
        let right_cost = if end + 1 < n { right_indicator_max } else { 0 };
        let available = budget.saturating_sub(left_cost + right_cost);
        if used + widths[end] > available { break; }
        used += widths[end];
        end += 1;
    }

    (start, end)
}

fn tab_style(state: SessionState, active: bool) -> Style {
    let bg = if active { Color::Rgb(50, 50, 70) } else { Color::Rgb(30, 30, 40) };
    let mut style = Style::default().fg(state.color()).bg(bg);
    if active { style = style.add_modifier(Modifier::BOLD); }
    style
}
