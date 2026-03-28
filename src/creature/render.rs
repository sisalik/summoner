use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};

use super::generate::{CellKind, Sprite};
use crate::session::SessionState;

pub struct Palette {
    pub body: Color,
    pub border: Color,
    pub highlight: Color,
}

pub fn state_palette(state: SessionState) -> Palette {
    match state {
        SessionState::Working => Palette {
            body: Color::Rgb(0, 200, 120),
            border: Color::Rgb(0, 100, 60),
            highlight: Color::Rgb(100, 255, 180),
        },
        SessionState::Waiting => Palette {
            body: Color::Rgb(255, 180, 50),
            border: Color::Rgb(180, 100, 0),
            highlight: Color::Rgb(255, 220, 100),
        },
        SessionState::Idle => Palette {
            body: Color::Rgb(100, 120, 220),
            border: Color::Rgb(50, 60, 140),
            highlight: Color::Rgb(150, 170, 255),
        },
        SessionState::Sleeping => Palette {
            body: Color::Rgb(80, 90, 120),
            border: Color::Rgb(40, 45, 60),
            highlight: Color::Rgb(100, 110, 140),
        },
        SessionState::Disconnected => Palette {
            body: Color::Rgb(100, 100, 100),
            border: Color::Rgb(60, 60, 60),
            highlight: Color::Rgb(130, 130, 130),
        },
        SessionState::ShellOnly => Palette {
            body: Color::Rgb(200, 200, 210),
            border: Color::Rgb(140, 140, 150),
            highlight: Color::Rgb(240, 240, 255),
        },
    }
}

const UPPER_HALF: &str = "\u{2580}";
const LOWER_HALF: &str = "\u{2584}";
const FULL_BLOCK: &str = "\u{2588}";

pub fn render_sprite_to_buffer(
    sprite: &Sprite,
    palette: &Palette,
    area: Rect,
    buf: &mut Buffer,
) {
    let rows = (sprite.height + 1) / 2;

    for row in 0..rows.min(area.height as usize) {
        for col in 0..sprite.width.min(area.width as usize) {
            let upper_y = row * 2;
            let lower_y = row * 2 + 1;

            let upper = sprite.get(col, upper_y);
            let lower = if lower_y < sprite.height {
                sprite.get(col, lower_y)
            } else {
                CellKind::Empty
            };

            let pos = Position {
                x: area.x + col as u16,
                y: area.y + row as u16,
            };

            if let Some(cell) = buf.cell_mut(pos) {
                match (upper, lower) {
                    (CellKind::Empty, CellKind::Empty) => {}
                    (CellKind::Empty, lower_kind) => {
                        cell.set_symbol(LOWER_HALF);
                        cell.set_style(Style::default().fg(kind_color(&lower_kind, palette)));
                    }
                    (upper_kind, CellKind::Empty) => {
                        cell.set_symbol(UPPER_HALF);
                        cell.set_style(Style::default().fg(kind_color(&upper_kind, palette)));
                    }
                    (upper_kind, lower_kind) => {
                        let fg_color = kind_color(&lower_kind, palette);
                        let bg_color = kind_color(&upper_kind, palette);
                        if fg_color == bg_color {
                            cell.set_symbol(FULL_BLOCK);
                            cell.set_style(Style::default().fg(fg_color));
                        } else {
                            cell.set_symbol(LOWER_HALF);
                            cell.set_style(Style::default().fg(fg_color).bg(bg_color));
                        }
                    }
                }
            }
        }
    }
}

fn kind_color(kind: &CellKind, palette: &Palette) -> Color {
    match kind {
        CellKind::Body => palette.body,
        CellKind::Border => palette.border,
        CellKind::Empty => Color::Reset,
    }
}

pub fn sprite_cell_size(sprite: &Sprite) -> (u16, u16) {
    (sprite.width as u16, ((sprite.height + 1) / 2) as u16)
}

pub fn terminal_icon_sprite() -> Sprite {
    use CellKind::*;
    let cells = vec![
        // Row 0: title bar
        Border, Border, Border, Border, Border, Border, Border, Border, Border, Border,
        // Row 1: title bar content (window buttons)
        Border, Body,   Body,   Body,   Empty,  Empty,  Empty,  Empty,  Empty,  Border,
        // Row 2: separator
        Border, Border, Border, Border, Border, Border, Border, Border, Border, Border,
        // Row 3: prompt line "> _"
        Border, Empty,  Body,   Empty,  Body,   Empty,  Empty,  Empty,  Empty,  Border,
        // Row 4: text line
        Border, Empty,  Empty,  Empty,  Empty,  Empty,  Empty,  Empty,  Empty,  Border,
        // Row 5: empty
        Border, Empty,  Empty,  Empty,  Empty,  Empty,  Empty,  Empty,  Empty,  Border,
        // Row 6: empty
        Border, Empty,  Empty,  Empty,  Empty,  Empty,  Empty,  Empty,  Empty,  Border,
        // Row 7: bottom border
        Border, Border, Border, Border, Border, Border, Border, Border, Border, Border,
    ];
    Sprite { width: 10, height: 8, cells }
}
