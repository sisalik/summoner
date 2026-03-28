use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::session::{Session, SessionState};

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

        for (i, session) in self.sessions.iter().enumerate() {
            let fkey = format!("F{}", i + 1);
            let icon = session.state.icon();
            let tab_text = format!(" {} {} {} ", fkey, icon, session.name);

            if x + tab_text.len() as u16 > max_tab_x {
                overflow_count = self.sessions.len() - i;
                break;
            }

            let is_active = self.active_index == Some(i);
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
    let fg = state_color(state);
    let bg = if active { Color::Rgb(50, 50, 70) } else { Color::Rgb(30, 30, 40) };
    let mut style = Style::default().fg(fg).bg(bg);
    if active { style = style.add_modifier(Modifier::BOLD); }
    style
}

fn state_color(state: SessionState) -> Color {
    match state {
        SessionState::Working => Color::Rgb(0, 200, 120),
        SessionState::Waiting => Color::Rgb(255, 180, 50),
        SessionState::Idle => Color::Rgb(100, 120, 220),
        SessionState::Sleeping => Color::Rgb(80, 90, 120),
        SessionState::Disconnected => Color::Rgb(100, 100, 100),
        SessionState::ShellOnly => Color::Rgb(200, 200, 210),
    }
}
