use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::DefaultTerminal;

use crate::creature::locomotion::LocomotionState;
use crate::creature::outline::{rasterize_skeleton, rasterize_skeleton_scaled};
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

// --- Grid mode ---

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

// --- Zoom mode ---

struct ZoomView {
    seed: u64,
    archetype: usize,
    state_idx: usize,
    loco: LocomotionState,
}

impl ZoomView {
    fn new(seed: u64, archetype: usize, state_idx: usize) -> Self {
        let skel = Skeleton::instantiate(archetype, seed);
        let loco = LocomotionState::new(skel, STATES[state_idx]);
        Self { seed, archetype, state_idx, loco }
    }

    fn rebuild(&mut self) {
        let skel = Skeleton::instantiate(self.archetype, self.seed);
        self.loco = LocomotionState::new(skel, STATES[self.state_idx]);
    }

    fn tick(&mut self, dt: Duration) {
        self.loco.tick(dt);
    }
}

fn draw_text(x: u16, y: u16, text: &str, style: Style, area: Rect, buf: &mut ratatui::buffer::Buffer) {
    for (i, ch) in text.chars().enumerate() {
        let px = x + i as u16;
        if px >= area.x + area.width { break; }
        if y >= area.y + area.height { break; }
        if let Some(cell) = buf.cell_mut(Position { x: px, y }) {
            cell.set_symbol(&ch.to_string());
            cell.set_style(style);
        }
    }
}

fn clear(area: Rect, buf: &mut ratatui::buffer::Buffer) {
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

// --- Main entry ---

enum Mode {
    Grid(TestGrid),
    Zoom(ZoomView),
}

pub fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let seed = 42u64;
    let mut mode = Mode::Grid(TestGrid::new(seed));
    let mut last_tick = Instant::now();

    loop {
        let dt = last_tick.elapsed();
        last_tick = Instant::now();

        match &mut mode {
            Mode::Grid(grid) => grid.tick(dt),
            Mode::Zoom(zoom) => zoom.tick(dt),
        }

        terminal.draw(|frame| {
            let area = frame.area();
            let buf = frame.buffer_mut();
            clear(area, buf);

            match &mode {
                Mode::Grid(grid) => render_grid(grid, area, buf),
                Mode::Zoom(zoom) => render_zoom(zoom, area, buf),
            }
        })?;

        if event::poll(Duration::from_millis(33))? {
            if let Event::Key(key) = event::read()? {
                match &mut mode {
                    Mode::Grid(grid) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('r') => grid.randomize(),
                        KeyCode::Enter | KeyCode::Char('z') => {
                            mode = Mode::Zoom(ZoomView::new(grid.seed, 0, 0));
                        }
                        _ => {}
                    },
                    Mode::Zoom(zoom) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            mode = Mode::Grid(TestGrid::new(zoom.seed));
                        }
                        KeyCode::Char('r') => {
                            zoom.seed = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos() as u64;
                            zoom.rebuild();
                        }
                        KeyCode::Up => {
                            zoom.archetype = (zoom.archetype + ARCHETYPE_COUNT - 1) % ARCHETYPE_COUNT;
                            zoom.rebuild();
                        }
                        KeyCode::Down => {
                            zoom.archetype = (zoom.archetype + 1) % ARCHETYPE_COUNT;
                            zoom.rebuild();
                        }
                        KeyCode::Left => {
                            zoom.state_idx = (zoom.state_idx + STATES.len() - 1) % STATES.len();
                            zoom.rebuild();
                        }
                        KeyCode::Right => {
                            zoom.state_idx = (zoom.state_idx + 1) % STATES.len();
                            zoom.rebuild();
                        }
                        _ => {}
                    },
                }
            }
        }
    }

    Ok(())
}

fn render_grid(grid: &TestGrid, area: Rect, buf: &mut ratatui::buffer::Buffer) {
    let title = format!(
        " Creature Test — seed: {} — [r] randomize  [z/Enter] zoom  [q] quit ",
        grid.seed
    );
    let title_style = Style::default().fg(Color::Rgb(200, 200, 220)).bg(Color::Rgb(30, 30, 50));
    draw_text(area.x, area.y, &title, title_style, area, buf);

    let grid_y = area.y + 2;
    let label_w: u16 = 12;

    for (col, &state) in STATES.iter().enumerate() {
        let x = area.x + label_w + col as u16 * CELL_W;
        let style = Style::default().fg(state.color());
        draw_text(x, grid_y, state.label(), style, area, buf);
    }

    let content_y = grid_y + 1;

    for (row, archetype) in (0..ARCHETYPE_COUNT).enumerate() {
        let row_y = content_y + row as u16 * CELL_H;
        let name = archetype_name(archetype);
        let label_style = Style::default().fg(Color::Rgb(140, 140, 160));
        let label_y = row_y + CELL_H / 2;
        if label_y < area.y + area.height {
            draw_text(area.x, label_y, name, label_style, area, buf);
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
}

fn render_zoom(zoom: &ZoomView, area: Rect, buf: &mut ratatui::buffer::Buffer) {
    let state = STATES[zoom.state_idx];

    // Title bar
    let title = format!(
        " ZOOM: {} / {} — seed: {} — [arrows] navigate  [r] randomize  [q/Esc] back ",
        archetype_name(zoom.archetype),
        state.label(),
        zoom.seed,
    );
    let title_style = Style::default().fg(Color::Rgb(200, 200, 220)).bg(Color::Rgb(30, 30, 50));
    draw_text(area.x, area.y, &title, title_style, area, buf);

    // Compute the rasterization scale that fits the terminal.
    // At scale N, the sprite is (18*N) wide x (24*N) tall sub-pixels,
    // which renders to (18*N) cols x (12*N) terminal rows via half-blocks.
    let avail_w = area.width.saturating_sub(4) as usize;
    let avail_h = area.height.saturating_sub(4) as usize; // title + bottom hint
    let scale_x = avail_w / 18;
    let scale_y = avail_h / 12;
    let scale = scale_x.min(scale_y).max(1);

    // Rasterize at higher resolution
    let sprite = rasterize_skeleton_scaled(zoom.loco.skeleton(), scale);
    let palette = state_palette(state);

    // Render 1:1 with the standard half-block renderer, centered
    let rendered_w = sprite.width as u16;    // 18 * scale
    let rendered_h = ((sprite.height + 1) / 2) as u16; // 12 * scale
    let offset_x = area.x + (area.width.saturating_sub(rendered_w)) / 2;
    let offset_y = area.y + 2 + (avail_h as u16).saturating_sub(rendered_h) / 2;

    let creature_area = Rect {
        x: offset_x,
        y: offset_y,
        width: rendered_w,
        height: rendered_h,
    };
    render_sprite_to_buffer(&sprite, &palette, creature_area, buf);

    // Navigation hints at bottom
    let hint = format!(
        " \u{2191}\u{2193} archetype ({}/{})  \u{2190}\u{2192} state ({}/{})  resolution: {}x ({}x{} sub-px) ",
        zoom.archetype + 1, ARCHETYPE_COUNT,
        zoom.state_idx + 1, STATES.len(),
        scale, sprite.width, sprite.height,
    );
    let hint_style = Style::default().fg(Color::Rgb(120, 120, 140));
    let hint_y = area.y + area.height - 1;
    draw_text(area.x, hint_y, &hint, hint_style, area, buf);
}
