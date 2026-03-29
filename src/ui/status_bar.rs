use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

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

        let mut x = area.x;
        let f12_hint = " F12 Dashboard ";
        let max_tab_x = area.x + area.width - f12_hint.len() as u16 - 1;
        let mut overflow_count = 0;

        let groups = group_by_project(self.sessions);
        let mut session_order: Vec<usize> = Vec::new();
        let mut group_boundaries: Vec<usize> = Vec::new();
        for group in &groups {
            group_boundaries.push(session_order.len());
            session_order.extend(&group.sessions);
        }

        // Pre-compute: for each group, find the last session index in the group
        // or the active session if one exists in the group
        let mut show_name_for: Vec<bool> = vec![false; session_order.len()];
        {
            let mut group_start = 0;
            for group in &groups {
                let group_end = group_start + group.sessions.len();
                // Find if any session in this group is active
                let active_in_group = (group_start..group_end).any(|pos| {
                    session_order.get(pos).map_or(false, |&si| self.active_index == Some(si))
                });
                if group.sessions.len() <= 1 {
                    // Single session — always show name
                    if group_start < show_name_for.len() {
                        show_name_for[group_start] = true;
                    }
                } else if active_in_group {
                    // Show name only for the active session
                    for pos in group_start..group_end {
                        if let Some(&si) = session_order.get(pos) {
                            if self.active_index == Some(si) {
                                show_name_for[pos] = true;
                            }
                        }
                    }
                } else {
                    // No active session in group — show name for last one
                    if group_end > 0 && group_end - 1 < show_name_for.len() {
                        show_name_for[group_end - 1] = true;
                    }
                }
                group_start = group_end;
            }
        }

        for (pos, &sess_idx) in session_order.iter().enumerate() {
            let session = &self.sessions[sess_idx];

            // Add separator between groups
            if pos > 0 && group_boundaries.contains(&pos) {
                let sep = "\u{2502}";
                if x < max_tab_x {
                    let sep_style = Style::default()
                        .fg(Color::Rgb(60, 60, 80))
                        .bg(Color::Rgb(30, 30, 40));
                    if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
                        cell.set_symbol(sep);
                        cell.set_style(sep_style);
                    }
                    x += 1;
                }
            }

            let fkey = format!("F{}", pos + 1);
            let icon = session.state.icon();
            let tab_text = if show_name_for.get(pos).copied().unwrap_or(true) {
                format!(" {} {} {} ", fkey, icon, session.name)
            } else {
                format!(" {} {} ", fkey, icon)
            };

            if x + tab_text.len() as u16 > max_tab_x {
                overflow_count = session_order.len() - pos;
                break;
            }

            let is_active = self.active_index == Some(sess_idx);
            let style = tab_style(session.state, is_active);

            for ch in tab_text.chars() {
                if x >= area.x + area.width { break; }
                if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
                    cell.set_symbol(&ch.to_string());
                    cell.set_style(style);
                }
                x += 1;
            }
        }

        if overflow_count > 0 {
            let overflow_text = format!(" +{} more ", overflow_count);
            let overflow_style = Style::default()
                .fg(Color::Rgb(120, 120, 140))
                .bg(Color::Rgb(30, 30, 40));
            for ch in overflow_text.chars() {
                if x >= max_tab_x { break; }
                if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
                    cell.set_symbol(&ch.to_string());
                    cell.set_style(overflow_style);
                }
                x += 1;
            }
        }

        let hint_x = area.x + area.width - f12_hint.len() as u16;
        let hint_style = Style::default()
            .fg(Color::Rgb(150, 150, 170))
            .bg(Color::Rgb(30, 30, 40));
        for (i, ch) in f12_hint.chars().enumerate() {
            let pos = Position { x: hint_x + i as u16, y: area.y };
            if let Some(cell) = buf.cell_mut(pos) {
                cell.set_symbol(&ch.to_string());
                cell.set_style(hint_style);
            }
        }
    }
}

fn tab_style(state: SessionState, active: bool) -> Style {
    let bg = if active { Color::Rgb(50, 50, 70) } else { Color::Rgb(30, 30, 40) };
    let mut style = Style::default().fg(state.color()).bg(bg);
    if active { style = style.add_modifier(Modifier::BOLD); }
    style
}
