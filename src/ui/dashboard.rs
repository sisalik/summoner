// Dashboard grid — flat navigation, terminal icons for shell-only sessions

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::creature::animate::{animate_sprite, AnimationState};
use crate::creature::generate::Sprite;
use crate::creature::render::{render_sprite_to_buffer, state_palette, terminal_icon_sprite};
use crate::session::{Session, SessionState};
use crate::ui::dashboard_nav::DashboardNav;

const CREATURE_WIDTH: u16 = 12;
const CREATURE_HEIGHT: u16 = 8;

pub struct Dashboard<'a> {
    sessions: &'a [Session],
    animations: &'a [AnimationState],
    sprites: &'a [Sprite],
    nav: &'a DashboardNav,
}

impl<'a> Dashboard<'a> {
    pub fn new(
        sessions: &'a [Session],
        animations: &'a [AnimationState],
        sprites: &'a [Sprite],
        nav: &'a DashboardNav,
    ) -> Self {
        Self { sessions, animations, sprites, nav }
    }
}

/// Compute grid layout: (cols, rows)
fn grid_layout(count: usize) -> (usize, usize) {
    match count {
        0 => (0, 0),
        1 => (1, 1),
        2 | 3 => (count, 1),
        4 | 5 | 6 => (3, 2),
        _ => (3, (count + 2) / 3),
    }
}

fn fill_background(area: Rect, buf: &mut Buffer) {
    let bg = Style::default().bg(Color::Rgb(20, 20, 30));
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut(Position { x, y }) {
                cell.set_symbol(" ");
                cell.set_style(bg);
            }
        }
    }
}

fn draw_text(x: u16, y: u16, text: &str, style: Style, area: Rect, buf: &mut Buffer) {
    for (i, ch) in text.chars().enumerate() {
        let px = x + i as u16;
        if px >= area.x + area.width { break; }
        if let Some(cell) = buf.cell_mut(Position { x: px, y }) {
            cell.set_symbol(&ch.to_string());
            cell.set_style(style);
        }
    }
}

impl<'a> Widget for Dashboard<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        fill_background(area, buf);

        if area.height < 3 { return; }

        // Reserve 1 row at bottom for hints
        let hint_y = area.y + area.height - 1;
        let hint_text = " \u{2190}\u{2192}\u{2191}\u{2193} navigate \u{2502} Enter select \u{2502} n new session \u{2502} N new dir \u{2502} x close \u{2502} X close project \u{2502} r restore ";
        let hint_style = Style::default()
            .fg(Color::Rgb(120, 120, 140))
            .bg(Color::Rgb(20, 20, 30));
        draw_text(area.x, hint_y, hint_text, hint_style, area, buf);

        // Content area (above hints)
        let content_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height.saturating_sub(1),
        };

        let session_count = self.sessions.len();

        if session_count == 0 {
            let msg = "No sessions. Press N to open a directory.";
            let msg_len = msg.len() as u16;
            let mx = content_area.x + content_area.width.saturating_sub(msg_len) / 2;
            let my = content_area.y + content_area.height / 2;
            let msg_style = Style::default().fg(Color::Rgb(120, 120, 140));
            draw_text(mx, my, msg, msg_style, content_area, buf);
            return;
        }

        let (cols, _rows) = grid_layout(session_count);
        if cols == 0 { return; }

        // Creature render height in terminal rows = CREATURE_HEIGHT / 2 (half-block)
        let creature_render_h = (CREATURE_HEIGHT + 1) / 2;

        // Card dimensions: each session gets its own card slot
        let total_margin_x = (cols as u16) + 1;
        let card_width = content_area.width.saturating_sub(total_margin_x) / cols as u16;

        // card inner height: padding(1) + creature(creature_render_h) + selection_bar(1) + name(1) + padding(1)
        let card_inner_height = 1 + creature_render_h + 1 + 1 + 1;
        let card_height = card_inner_height + 2; // + border top/bottom
        let row_stride = card_height + 1;

        let selected = self.nav.selected();

        for (sess_idx, session) in self.sessions.iter().enumerate() {
            let col = sess_idx % cols;
            let row = sess_idx / cols;

            let card_x = content_area.x + 1 + col as u16 * (card_width + 1);
            let card_y = content_area.y + 1 + row as u16 * row_stride;

            if card_y + card_height > content_area.y + content_area.height {
                break;
            }

            let card_area = Rect {
                x: card_x,
                y: card_y,
                width: card_width,
                height: card_height,
            };

            let is_selected = sess_idx == selected;
            let border_color = if is_selected {
                Color::Rgb(200, 200, 255)
            } else {
                Color::Rgb(60, 60, 80)
            };

            let title_str = format!(" {} {} ", session.state.icon(), session.name);
            let title_style = if is_selected {
                Style::default().fg(Color::Rgb(255, 255, 255))
            } else {
                Style::default().fg(Color::Rgb(140, 140, 160))
            };

            // Draw border
            let block = ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(title_str.as_str())
                .title_style(title_style)
                .padding(ratatui::widgets::Padding::uniform(1));

            let inner = block.inner(card_area);
            block.render(card_area, buf);

            // Render creature or terminal icon
            let creature_area = Rect {
                x: inner.x,
                y: inner.y,
                width: CREATURE_WIDTH.min(inner.width),
                height: creature_render_h,
            };

            let is_claude = session.claude_conversation_id.is_some()
                || session.state == SessionState::Working
                || session.state == SessionState::Waiting
                || session.state == SessionState::Idle;

            if is_claude {
                if let (Some(sprite), Some(anim)) =
                    (self.sprites.get(sess_idx), self.animations.get(sess_idx))
                {
                    let animated = animate_sprite(sprite, anim);
                    let palette = state_palette(session.state);
                    render_sprite_to_buffer(&animated, &palette, creature_area, buf);
                }
            } else {
                let icon = terminal_icon_sprite();
                let palette = state_palette(session.state);
                render_sprite_to_buffer(&icon, &palette, creature_area, buf);
            }

            // Draw selection highlight bar below the creature
            if is_selected {
                let bar_y = creature_area.y + creature_render_h;
                let bar_style = Style::default().fg(Color::Rgb(100, 100, 200));
                for x in creature_area.x..creature_area.x + CREATURE_WIDTH.min(creature_area.width) {
                    if bar_y < card_area.y + card_area.height {
                        if let Some(cell) = buf.cell_mut(Position { x, y: bar_y }) {
                            cell.set_symbol("\u{2580}");
                            cell.set_style(bar_style);
                        }
                    }
                }
            }
        }
    }
}
