// Dashboard grid — grouped by project, with selection boxes

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::creature::animate::{animate_sprite, AnimationState};
use crate::creature::generate::Sprite;
use crate::creature::render::{render_sprite_to_buffer, state_palette, terminal_icon_sprite};
use crate::session::{group_by_project, Session, SessionState};
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

/// Compute grid layout: (cols, rows) based on group count
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

/// Draw a box around an area using box-drawing characters
fn draw_selection_box(area: Rect, buf: &mut Buffer, color: Color) {
    let style = Style::default().fg(color);
    let x1 = area.x;
    let y1 = area.y;
    let x2 = area.x + area.width.saturating_sub(1);
    let y2 = area.y + area.height.saturating_sub(1);

    // Corners
    if let Some(cell) = buf.cell_mut(Position { x: x1, y: y1 }) {
        cell.set_symbol("\u{250c}"); // ┌
        cell.set_style(style);
    }
    if let Some(cell) = buf.cell_mut(Position { x: x2, y: y1 }) {
        cell.set_symbol("\u{2510}"); // ┐
        cell.set_style(style);
    }
    if let Some(cell) = buf.cell_mut(Position { x: x1, y: y2 }) {
        cell.set_symbol("\u{2514}"); // └
        cell.set_style(style);
    }
    if let Some(cell) = buf.cell_mut(Position { x: x2, y: y2 }) {
        cell.set_symbol("\u{2518}"); // ┘
        cell.set_style(style);
    }

    // Top and bottom edges
    for x in (x1 + 1)..x2 {
        if let Some(cell) = buf.cell_mut(Position { x, y: y1 }) {
            cell.set_symbol("\u{2500}"); // ─
            cell.set_style(style);
        }
        if let Some(cell) = buf.cell_mut(Position { x, y: y2 }) {
            cell.set_symbol("\u{2500}"); // ─
            cell.set_style(style);
        }
    }

    // Left and right edges
    for y in (y1 + 1)..y2 {
        if let Some(cell) = buf.cell_mut(Position { x: x1, y }) {
            cell.set_symbol("\u{2502}"); // │
            cell.set_style(style);
        }
        if let Some(cell) = buf.cell_mut(Position { x: x2, y }) {
            cell.set_symbol("\u{2502}"); // │
            cell.set_style(style);
        }
    }
}

impl<'a> Widget for Dashboard<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        fill_background(area, buf);

        if area.height < 3 { return; }

        // Reserve 1 row at bottom for hints (Issue C: shorter hints)
        let hint_y = area.y + area.height - 1;
        let hint_text = " \u{2190}\u{2192}\u{2191}\u{2193} navigate \u{2502} Enter open \u{2502} n/N new session/dir \u{2502} x/X close session/project ";
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

        // Group sessions by project (Issue A)
        let groups = group_by_project(self.sessions);

        if groups.is_empty() {
            let msg = "No sessions. Press N to open a directory.";
            let msg_len = msg.len() as u16;
            let mx = content_area.x + content_area.width.saturating_sub(msg_len) / 2;
            let my = content_area.y + content_area.height / 2;
            let msg_style = Style::default().fg(Color::Rgb(120, 120, 140));
            draw_text(mx, my, msg, msg_style, content_area, buf);
            return;
        }

        let (cols, _rows) = grid_layout(groups.len());
        if cols == 0 { return; }

        // Creature render height in terminal rows = CREATURE_HEIGHT / 2 (half-block)
        let creature_render_h = (CREATURE_HEIGHT + 1) / 2;

        // Card dimensions: each group gets its own card slot
        let total_margin_x = (cols as u16) + 1;
        let card_width = content_area.width.saturating_sub(total_margin_x) / cols as u16;

        // card inner height: padding(1) + creature(creature_render_h) + name_row(1) + padding(1)
        let card_inner_height = 1 + creature_render_h + 1 + 1;
        let card_height = card_inner_height + 2; // + border top/bottom
        let row_stride = card_height + 1;

        let selected = self.nav.selected();

        // Build a mapping: for each group, which global session indices it contains
        // and figure out which group contains the selected session
        for (group_idx, group) in groups.iter().enumerate() {
            let col = group_idx % cols;
            let row = group_idx / cols;

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

            // Check if the selected session is in this group
            let selected_in_group = group.sessions.contains(&selected);
            let border_color = if selected_in_group {
                Color::Rgb(200, 200, 255)
            } else {
                Color::Rgb(60, 60, 80)
            };

            let display_name = crate::app::display_path(&group.directory);
            let title_str = format!(" {} ", display_name);
            let title_style = if selected_in_group {
                Style::default().fg(Color::Rgb(255, 255, 255))
            } else {
                Style::default().fg(Color::Rgb(140, 140, 160))
            };

            // Draw card border
            let block = ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(title_str.as_str())
                .title_style(title_style)
                .padding(ratatui::widgets::Padding::uniform(1));

            let inner = block.inner(card_area);
            block.render(card_area, buf);

            // Render all creatures in this group side by side
            let creature_count = group.sessions.len();
            let creature_slot_width = if creature_count > 0 {
                inner.width / creature_count as u16
            } else {
                inner.width
            };

            for (local_idx, &sess_idx) in group.sessions.iter().enumerate() {
                let session = &self.sessions[sess_idx];

                let cx = inner.x + (local_idx as u16) * creature_slot_width;
                let cw = CREATURE_WIDTH.min(creature_slot_width);

                let creature_area = Rect {
                    x: cx,
                    y: inner.y,
                    width: cw,
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

                // Draw session name below creature
                let name_y = inner.y + creature_render_h;
                if name_y < card_area.y + card_area.height.saturating_sub(1) {
                    let name_style = Style::default().fg(Color::Rgb(140, 140, 160));
                    let icon = session.state.icon();
                    let label = format!("{} {}", icon, session.name);
                    let truncated: String = label.chars().take(cw as usize).collect();
                    draw_text(cx, name_y, &truncated, name_style, card_area, buf);
                }

                // Issue B: Draw selection box around the selected creature
                if sess_idx == selected {
                    // Extend 1 cell in each direction if space allows
                    let box_x = cx.saturating_sub(1).max(card_area.x + 1);
                    let box_y = inner.y.saturating_sub(1).max(card_area.y + 1);
                    let box_right = (cx + cw + 1).min(card_area.x + card_area.width - 1);
                    let box_bottom = (name_y + 2).min(card_area.y + card_area.height - 1);
                    let box_w = box_right.saturating_sub(box_x);
                    let box_h = box_bottom.saturating_sub(box_y);

                    if box_w >= 3 && box_h >= 3 {
                        let box_area = Rect {
                            x: box_x,
                            y: box_y,
                            width: box_w,
                            height: box_h,
                        };
                        draw_selection_box(box_area, buf, Color::Rgb(150, 150, 255));
                    }
                }
            }
        }
    }
}
