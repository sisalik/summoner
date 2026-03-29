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
    let body = state.color();
    match state {
        SessionState::Working => Palette { body, border: Color::Rgb(0, 100, 60), highlight: Color::Rgb(100, 255, 180) },
        SessionState::Waiting => Palette { body, border: Color::Rgb(180, 100, 0), highlight: Color::Rgb(255, 220, 100) },
        SessionState::Idle => Palette { body, border: Color::Rgb(50, 60, 140), highlight: Color::Rgb(150, 170, 255) },
        SessionState::Sleeping => Palette { body, border: Color::Rgb(40, 45, 60), highlight: Color::Rgb(100, 110, 140) },
        SessionState::Disconnected => Palette { body, border: Color::Rgb(60, 60, 60), highlight: Color::Rgb(130, 130, 130) },
        SessionState::ShellOnly => Palette { body, border: Color::Rgb(140, 140, 150), highlight: Color::Rgb(240, 240, 255) },
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

pub fn shade_color(color: Color, offset: i16) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let clamp = |v: u8, o: i16| -> u8 {
                (v as i16 + o).clamp(0, 255) as u8
            };
            Color::Rgb(clamp(r, offset), clamp(g, offset), clamp(b, offset))
        }
        other => other,
    }
}

pub fn capsule_shade_offset(id: u8) -> i16 {
    match id {
        0 => 0,
        1 => 40,
        2..=99 => 0,
        100..=199 => {
            if id % 2 == 0 {
                -20 // upper limb (even)
            } else {
                -40 // lower limb (odd)
            }
        }
        200..=255 => 20,
    }
}

fn shaded_kind_color(kind: &CellKind, capsule_id: u8, palette: &Palette) -> Color {
    match kind {
        CellKind::Body => shade_color(palette.body, capsule_shade_offset(capsule_id)),
        CellKind::Border => palette.border,
        CellKind::Empty => Color::Reset,
    }
}

pub fn render_sprite_shaded(
    sprite: &Sprite,
    capsule_ids: &[u8],
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

            let upper_id = capsule_ids.get(upper_y * sprite.width + col).copied().unwrap_or(0);
            let lower_id = if lower_y < sprite.height {
                capsule_ids.get(lower_y * sprite.width + col).copied().unwrap_or(0)
            } else {
                0
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
                        cell.set_style(Style::default().fg(shaded_kind_color(&lower_kind, lower_id, palette)));
                    }
                    (upper_kind, CellKind::Empty) => {
                        cell.set_symbol(UPPER_HALF);
                        cell.set_style(Style::default().fg(shaded_kind_color(&upper_kind, upper_id, palette)));
                    }
                    (upper_kind, lower_kind) => {
                        let fg_color = shaded_kind_color(&lower_kind, lower_id, palette);
                        let bg_color = shaded_kind_color(&upper_kind, upper_id, palette);
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

pub fn sprite_cell_size(sprite: &Sprite) -> (u16, u16) {
    (sprite.width as u16, ((sprite.height + 1) / 2) as u16)
}

pub fn terminal_icon_sprite() -> Sprite {
    use CellKind::*;
    #[allow(non_snake_case)]
    let (B, O, E) = (Border, Body, Empty);
    // 18x14 — proportionally scaled from original 10x8
    let cells = vec![
        // Row 0: top frame
        B, B, B, B, B, B, B, B, B, B, B, B, B, B, B, B, B, B,
        // Row 1: title bar with traffic-light buttons
        B, E, O, O, E, O, O, E, O, O, E, E, E, E, E, E, E, B,
        // Row 2: separator
        B, B, B, B, B, B, B, B, B, B, B, B, B, B, B, B, B, B,
        // Row 3: empty
        B, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, B,
        // Row 4: prompt "> _"
        B, E, O, O, E, O, E, E, E, E, E, E, E, E, E, E, E, B,
        // Row 5: output text
        B, E, E, O, O, O, O, O, O, E, E, E, E, E, E, E, E, B,
        // Row 6: empty
        B, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, B,
        // Row 7: empty
        B, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, B,
        // Row 8: empty
        B, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, B,
        // Row 9: empty
        B, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, B,
        // Row 10: empty
        B, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, B,
        // Row 11: empty
        B, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, B,
        // Row 12: empty
        B, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, E, B,
        // Row 13: bottom frame
        B, B, B, B, B, B, B, B, B, B, B, B, B, B, B, B, B, B,
    ];
    Sprite { width: 18, height: 14, cells }
}
