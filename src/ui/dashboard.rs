// Dashboard grid — Task 10

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Padding, Widget};

use crate::creature::animate::{animate_sprite, AnimationState};
use crate::creature::generate::Sprite;
use crate::creature::render::{render_sprite_to_buffer, sprite_cell_size, state_palette};
use crate::session::{group_by_project, Session};
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
fn grid_layout(card_count: usize) -> (usize, usize) {
    match card_count {
        0 => (0, 0),
        1 => (1, 1),
        2 | 3 => (card_count, 1),
        4 | 5 | 6 => (3, 2),
        _ => (3, (card_count + 2) / 3),
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
        let hint_text = " ←→↑↓ navigate │ Enter select │ n new session │ N new dir │ d close │ r restore │ q back ";
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

        let groups = group_by_project(self.sessions);

        if groups.is_empty() {
            // Show "no sessions" message centered
            let msg = "No sessions. Press N to open a directory.";
            let msg_len = msg.len() as u16;
            let mx = content_area.x + content_area.width.saturating_sub(msg_len) / 2;
            let my = content_area.y + content_area.height / 2;
            let msg_style = Style::default().fg(Color::Rgb(120, 120, 140));
            draw_text(mx, my, msg, msg_style, content_area, buf);
            return;
        }

        let card_count = groups.len();
        let (cols, _rows) = grid_layout(card_count);
        if cols == 0 { return; }

        // Card dimensions: divide available space with 1-cell margins
        // margins: 1 cell on left, 1 between cards, 1 on right => total margin = cols + 1
        let total_margin_x = (cols as u16) + 1;
        let card_width = content_area.width.saturating_sub(total_margin_x) / cols as u16;

        // Creature render height in terminal rows = CREATURE_HEIGHT / 2 (half-block)
        let creature_render_h = (CREATURE_HEIGHT + 1) / 2;
        // card inner height: padding(1) + creatures(creature_render_h) + footer(1) + padding(1)
        let card_inner_height = 1 + creature_render_h + 1 + 1;
        // card outer height: inner + border top + border bottom = inner + 2
        let card_height = card_inner_height + 2;

        // Row spacing: 1 cell margin between rows
        let row_stride = card_height + 1;

        let selected_card = self.nav.selected_card();
        let selected_session_in_card = self.nav.selected_session_in_card();

        for (card_idx, group) in groups.iter().enumerate() {
            let col = card_idx % cols;
            let row = card_idx / cols;

            let card_x = content_area.x + 1 + col as u16 * (card_width + 1);
            let card_y = content_area.y + 1 + row as u16 * row_stride;

            if card_y + card_height > content_area.y + content_area.height {
                break; // out of vertical space
            }

            let card_area = Rect {
                x: card_x,
                y: card_y,
                width: card_width,
                height: card_height,
            };

            let is_selected = card_idx == selected_card;
            let in_card = selected_session_in_card.is_some();
            let border_color = if is_selected && in_card {
                Color::Rgb(255, 255, 255)
            } else if is_selected {
                Color::Rgb(200, 200, 255)
            } else {
                Color::Rgb(60, 60, 80)
            };

            let title_str = format!(" {} ", group.name);
            let title_style = if is_selected {
                Style::default().fg(Color::Rgb(255, 255, 255))
            } else {
                Style::default().fg(Color::Rgb(140, 140, 160))
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(title_str.as_str())
                .title_style(title_style)
                .padding(Padding::uniform(1));

            let inner = block.inner(card_area);
            block.render(card_area, buf);

            // Render creatures side by side within the inner area
            // Each creature occupies CREATURE_WIDTH terminal columns
            let session_count = group.sessions.len();
            for (si, &sess_idx) in group.sessions.iter().enumerate() {
                let creature_x = inner.x + si as u16 * CREATURE_WIDTH;
                if creature_x + CREATURE_WIDTH > inner.x + inner.width {
                    break;
                }

                let creature_area = Rect {
                    x: creature_x,
                    y: inner.y,
                    width: CREATURE_WIDTH,
                    height: creature_render_h,
                };

                if let (Some(sprite), Some(anim)) =
                    (self.sprites.get(sess_idx), self.animations.get(sess_idx))
                {
                    let session = &self.sessions[sess_idx];
                    let animated = animate_sprite(sprite, anim);
                    let palette = state_palette(session.state);
                    let (_sw, _sh) = sprite_cell_size(&animated);
                    render_sprite_to_buffer(&animated, &palette, creature_area, buf);

                    // Draw ▲ marker below selected session
                    if is_selected && selected_session_in_card == Some(si) {
                        let marker_y = inner.y + creature_render_h;
                        let center_x = creature_x + CREATURE_WIDTH / 2;
                        if marker_y < card_area.y + card_area.height
                            && center_x < card_area.x + card_area.width
                        {
                            let marker_style =
                                Style::default().fg(Color::Rgb(200, 200, 255));
                            draw_text(center_x, marker_y, "▲", marker_style, card_area, buf);
                        }
                    }
                }
            }

            // Footer: session count
            let footer_y = inner.y + inner.height.saturating_sub(1);
            let footer_text = format!("{} session{}", session_count, if session_count == 1 { "" } else { "s" });
            let footer_style = Style::default().fg(Color::Rgb(100, 100, 120));
            draw_text(inner.x, footer_y, &footer_text, footer_style, card_area, buf);
        }
    }
}
