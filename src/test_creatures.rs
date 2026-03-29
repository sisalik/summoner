use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::DefaultTerminal;

use crate::creature::locomotion::LocomotionState;
use crate::creature::outline::rasterize_skeleton;
use crate::creature::render::{render_sprite_to_buffer, state_palette};
use crate::creature::skeleton::{Skeleton, ARCHETYPE_COUNT, archetype_name};
use crate::session::SessionState;

const CELL_W: u16 = 20;
const CELL_H: u16 = 14;

const STATES: &[SessionState] = &[
    SessionState::Working,
    SessionState::Waiting,
    SessionState::Idle,
    SessionState::Sleeping,
    SessionState::Disconnected,
];

struct TestGrid {
    seed: u64,
    locomotions: Vec<Vec<LocomotionState>>,
}

impl TestGrid {
    fn new(seed: u64) -> Self {
        let mut locomotions = Vec::new();
        for archetype in 0..ARCHETYPE_COUNT {
            let mut row = Vec::new();
            for &state in STATES {
                let skel = Skeleton::instantiate(archetype, seed);
                row.push(LocomotionState::new(skel, state));
            }
            locomotions.push(row);
        }
        Self { seed, locomotions }
    }

    fn randomize(&mut self) {
        self.seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        *self = Self::new(self.seed);
    }

    fn tick(&mut self, dt: Duration) {
        for row in &mut self.locomotions {
            for loco in row {
                loco.tick(dt);
            }
        }
    }
}

pub fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut grid = TestGrid::new(42);
    let mut last_tick = Instant::now();

    loop {
        let dt = last_tick.elapsed();
        last_tick = Instant::now();
        grid.tick(dt);

        terminal.draw(|frame| {
            let area = frame.area();
            let buf = frame.buffer_mut();

            let bg = Style::default().bg(Color::Rgb(20, 20, 30));
            for y in area.y..area.y + area.height {
                for x in area.x..area.x + area.width {
                    if let Some(cell) = buf.cell_mut(Position { x, y }) {
                        cell.set_symbol(" ");
                        cell.set_style(bg);
                    }
                }
            }

            let title = format!(" Creature Test Mode — seed: {} — [r] randomize  [q] quit ", grid.seed);
            let title_style = Style::default().fg(Color::Rgb(200, 200, 220)).bg(Color::Rgb(30, 30, 50));
            for (i, ch) in title.chars().enumerate() {
                let x = area.x + i as u16;
                if x >= area.x + area.width { break; }
                if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
                    cell.set_symbol(&ch.to_string());
                    cell.set_style(title_style);
                }
            }

            let grid_y = area.y + 2;
            let label_w: u16 = 12;

            for (col, &state) in STATES.iter().enumerate() {
                let x = area.x + label_w + col as u16 * CELL_W;
                let style = Style::default().fg(state.color());
                let label = state.label();
                for (i, ch) in label.chars().enumerate() {
                    let px = x + i as u16;
                    if px >= area.x + area.width { break; }
                    if let Some(cell) = buf.cell_mut(Position { x: px, y: grid_y }) {
                        cell.set_symbol(&ch.to_string());
                        cell.set_style(style);
                    }
                }
            }

            let content_y = grid_y + 1;

            for (row, archetype) in (0..ARCHETYPE_COUNT).enumerate() {
                let row_y = content_y + row as u16 * CELL_H;
                let name = archetype_name(archetype);
                let label_style = Style::default().fg(Color::Rgb(140, 140, 160));
                for (i, ch) in name.chars().enumerate() {
                    let x = area.x + i as u16;
                    if x >= area.x + area.width { break; }
                    let y = row_y + CELL_H / 2;
                    if y >= area.y + area.height { break; }
                    if let Some(cell) = buf.cell_mut(Position { x, y }) {
                        cell.set_symbol(&ch.to_string());
                        cell.set_style(label_style);
                    }
                }

                for (col, &state) in STATES.iter().enumerate() {
                    let cell_x = area.x + label_w + col as u16 * CELL_W;
                    let cell_y = row_y;
                    if cell_y + CELL_H > area.y + area.height { break; }
                    if cell_x + CELL_W > area.x + area.width { break; }

                    let creature_area = Rect {
                        x: cell_x + 1,
                        y: cell_y,
                        width: 18,
                        height: 12,
                    };

                    let loco = &grid.locomotions[archetype][col];
                    let sprite = rasterize_skeleton(loco.skeleton());
                    let palette = state_palette(state);
                    render_sprite_to_buffer(&sprite, &palette, creature_area, buf);
                }
            }
        })?;

        if event::poll(Duration::from_millis(33))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => grid.randomize(),
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
