use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::DefaultTerminal;

use crate::creature::generate::{CellKind, Sprite};
use crate::creature::locomotion::LocomotionState;
use crate::creature::outline::{rasterize_skeleton, rasterize_skeleton_scaled, rasterize_skeleton_wireframe};
use crate::creature::render::{render_sprite_shaded, state_palette};
use crate::creature::skeleton::{Skeleton, ARCHETYPE_COUNT, archetype_name};
use crate::session::SessionState;

const CELL_W: u16 = 20;
const CELL_H: u16 = 14;

const ANIM_STATES: &[SessionState] = &[
    SessionState::Working,
    SessionState::Waiting,
    SessionState::Idle,
    SessionState::Sleeping,
    SessionState::Disconnected,
];

/// Extended state labels for the test grid, including "Rest" pseudo-state.
const STATE_COUNT: usize = 6; // 5 real + 1 Rest

fn state_label(idx: usize) -> &'static str {
    if idx < ANIM_STATES.len() {
        ANIM_STATES[idx].label()
    } else {
        "Rest"
    }
}

fn state_color(idx: usize) -> Color {
    if idx < ANIM_STATES.len() {
        ANIM_STATES[idx].color()
    } else {
        Color::Rgb(180, 180, 200)
    }
}

/// Distinctive colors for component-ID debug rendering.
const COMPONENT_COLORS: &[Color] = &[
    Color::Rgb(255, 100, 100), // 1: head — red
    Color::Rgb(100, 255, 100), // 2: spine seg 0 — green
    Color::Rgb(100, 100, 255), // 3: spine seg 1 — blue
    Color::Rgb(255, 255, 100), // 4: spine seg 2 — yellow
    Color::Rgb(255, 100, 255), // 5: spine seg 3 — magenta
    Color::Rgb(100, 255, 255), // 6: spine seg 4 — cyan
    Color::Rgb(255, 180, 100), // 7: spine seg 5 — orange
    Color::Rgb(180, 100, 255), // 8: spine seg 6 — purple
    Color::Rgb(100, 200, 150), // 9: spine seg 7 — teal
    Color::Rgb(200, 200, 100), // 10: spine seg 8 — olive
];

const LIMB_COLORS: &[Color] = &[
    Color::Rgb(255, 80, 80),   // limb 0 — bright red
    Color::Rgb(80, 200, 255),  // limb 1 — sky blue
    Color::Rgb(255, 200, 80),  // limb 2 — gold
    Color::Rgb(80, 255, 160),  // limb 3 — mint
];

fn component_color(id: u8) -> Color {
    if id == 0 {
        Color::Reset
    } else if id >= 100 {
        let li = (id - 100) as usize;
        LIMB_COLORS[li % LIMB_COLORS.len()]
    } else {
        let i = (id as usize).saturating_sub(1);
        COMPONENT_COLORS[i % COMPONENT_COLORS.len()]
    }
}

// --- Grid mode ---

struct TestGrid {
    seed: u64,
    // [archetype][state_idx] — state_idx 0..4 are animated, 5 is rest (None locomotion)
    locomotions: Vec<Vec<Option<LocomotionState>>>,
}

impl TestGrid {
    fn new(seed: u64) -> Self {
        let mut locomotions = Vec::new();
        for archetype in 0..ARCHETYPE_COUNT {
            let mut row = Vec::new();
            for state_idx in 0..STATE_COUNT {
                if state_idx < ANIM_STATES.len() {
                    let skel = Skeleton::instantiate(archetype, seed);
                    row.push(Some(LocomotionState::new(skel, ANIM_STATES[state_idx])));
                } else {
                    row.push(None); // Rest — static skeleton
                }
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
            for loco in row.iter_mut().flatten() {
                loco.tick(dt);
            }
        }
    }

    fn skeleton_for(&self, archetype: usize, state_idx: usize) -> Skeleton {
        if let Some(Some(loco)) = self.locomotions.get(archetype).and_then(|r| r.get(state_idx)) {
            loco.skeleton().clone()
        } else {
            // Rest state — fresh skeleton
            Skeleton::instantiate(archetype, self.seed)
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ColorMode {
    Shaded,    // state palette + per-capsule shading
    Limb,      // rainbow colors by component ID
    Wireframe, // skeleton lines only
}

impl ColorMode {
    fn next(self) -> Self {
        match self {
            Self::Shaded => Self::Limb,
            Self::Limb => Self::Wireframe,
            Self::Wireframe => Self::Shaded,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Shaded => "shaded",
            Self::Limb => "limb colors",
            Self::Wireframe => "wireframe",
        }
    }
}

// --- Zoom mode ---

struct ZoomView {
    seed: u64,
    archetype: usize,
    state_idx: usize, // 0..5 (5 = Rest)
    loco: Option<LocomotionState>,
    color_mode: ColorMode,
    paused: bool,
    tick_count: u32,
}

impl ZoomView {
    fn new(seed: u64, archetype: usize, state_idx: usize) -> Self {
        let loco = if state_idx < ANIM_STATES.len() {
            let skel = Skeleton::instantiate(archetype, seed);
            Some(LocomotionState::new(skel, ANIM_STATES[state_idx]))
        } else {
            None
        };
        Self { seed, archetype, state_idx, loco, color_mode: ColorMode::Shaded, paused: false, tick_count: 0 }
    }

    fn rebuild(&mut self) {
        self.loco = if self.state_idx < ANIM_STATES.len() {
            let skel = Skeleton::instantiate(self.archetype, self.seed);
            Some(LocomotionState::new(skel, ANIM_STATES[self.state_idx]))
        } else {
            None
        };
        self.tick_count = 0;
    }

    fn tick(&mut self, dt: Duration) {
        if self.paused { return; }
        if let Some(loco) = &mut self.loco {
            loco.tick(dt);
            self.tick_count += 1;
        }
    }

    fn step_forward(&mut self) {
        if let Some(loco) = &mut self.loco {
            loco.tick(Duration::from_millis(33));
            self.tick_count += 1;
        }
    }

    fn step_backward(&mut self) {
        if self.tick_count == 0 { return; }
        let target = self.tick_count - 1;
        self.rebuild();
        for _ in 0..target {
            if let Some(loco) = &mut self.loco {
                loco.tick(Duration::from_millis(33));
            }
        }
        self.tick_count = target;
    }

    fn current_skeleton(&self) -> Skeleton {
        if let Some(loco) = &self.loco {
            loco.skeleton().clone()
        } else {
            Skeleton::instantiate(self.archetype, self.seed)
        }
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
                        KeyCode::Enter | KeyCode::Char('z') | KeyCode::Char('Z') => {
                            mode = Mode::Zoom(ZoomView::new(grid.seed, 0, 0));
                        }
                        _ => {}
                    },
                    Mode::Zoom(zoom) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc
                        | KeyCode::Char('z') | KeyCode::Char('Z') => {
                            mode = Mode::Grid(TestGrid::new(zoom.seed));
                        }
                        KeyCode::Char('r') => {
                            zoom.seed = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos() as u64;
                            zoom.rebuild();
                        }
                        KeyCode::Char(' ') => { zoom.paused = !zoom.paused; }
                        KeyCode::Char('.') => { if zoom.paused { zoom.step_forward(); } }
                        KeyCode::Char(',') => { if zoom.paused { zoom.step_backward(); } }
                        KeyCode::Char('c') => {
                            zoom.color_mode = zoom.color_mode.next();
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
                            zoom.state_idx = (zoom.state_idx + STATE_COUNT - 1) % STATE_COUNT;
                            zoom.rebuild();
                        }
                        KeyCode::Right => {
                            zoom.state_idx = (zoom.state_idx + 1) % STATE_COUNT;
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
        " Creature Test — seed: {} — [r] randomize  [z] zoom  [q] quit ",
        grid.seed
    );
    let title_style = Style::default().fg(Color::Rgb(200, 200, 220)).bg(Color::Rgb(30, 30, 50));
    draw_text(area.x, area.y, &title, title_style, area, buf);

    let grid_y = area.y + 2;
    let label_w: u16 = 12;

    // Column headers — 6 columns (5 states + Rest)
    for col in 0..STATE_COUNT {
        let x = area.x + label_w + col as u16 * CELL_W;
        let style = Style::default().fg(state_color(col));
        draw_text(x, grid_y, state_label(col), style, area, buf);
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

        for col in 0..STATE_COUNT {
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

            let skel = grid.skeleton_for(archetype, col);
            let raster = rasterize_skeleton(&skel);
            let palette = if col < ANIM_STATES.len() {
                state_palette(ANIM_STATES[col])
            } else {
                state_palette(SessionState::Idle)
            };
            render_sprite_shaded(&raster.sprite, &raster.capsule_ids, &palette, creature_area, buf);
        }
    }
}

fn render_zoom(zoom: &ZoomView, area: Rect, buf: &mut ratatui::buffer::Buffer) {
    let state_name = state_label(zoom.state_idx);

    let pause_str = if zoom.paused { " PAUSED" } else { "" };
    let title = format!(
        " ZOOM: {} / {} [{}]{} — seed: {} — [space] pause  [,/.] step  [arrows] nav  [c] color  [r] seed  [z] back ",
        archetype_name(zoom.archetype), state_name, zoom.color_mode.label(), pause_str, zoom.seed,
    );
    let title_style = Style::default().fg(Color::Rgb(200, 200, 220)).bg(Color::Rgb(30, 30, 50));
    draw_text(area.x, area.y, &title, title_style, area, buf);

    let avail_w = area.width.saturating_sub(4) as usize;
    let avail_h = area.height.saturating_sub(4) as usize;
    let scale_x = avail_w / 18;
    let scale_y = avail_h / 12;
    let scale = scale_x.min(scale_y).max(1);

    let skel = zoom.current_skeleton();

    let raster = match zoom.color_mode {
        ColorMode::Shaded | ColorMode::Limb => rasterize_skeleton_scaled(&skel, scale),
        ColorMode::Wireframe => rasterize_skeleton_wireframe(&skel, scale),
    };

    let rendered_w = raster.sprite.width as u16;
    let rendered_h = ((raster.sprite.height + 1) / 2) as u16;
    let offset_x = area.x + (area.width.saturating_sub(rendered_w)) / 2;
    let offset_y = area.y + 2 + (avail_h as u16).saturating_sub(rendered_h) / 2;

    let creature_area = Rect {
        x: offset_x, y: offset_y, width: rendered_w, height: rendered_h,
    };

    match zoom.color_mode {
        ColorMode::Shaded => {
            let palette = if zoom.state_idx < ANIM_STATES.len() {
                state_palette(ANIM_STATES[zoom.state_idx])
            } else {
                state_palette(SessionState::Idle)
            };
            render_sprite_shaded(&raster.sprite, &raster.capsule_ids, &palette, creature_area, buf);
        }
        ColorMode::Limb | ColorMode::Wireframe => {
            render_sprite_with_components(&raster.sprite, &raster.capsule_ids, creature_area, buf);
        }
    }

    let hint = format!(
        " \u{2191}\u{2193} archetype ({}/{})  \u{2190}\u{2192} state ({}/{})  {}x ",
        zoom.archetype + 1, ARCHETYPE_COUNT,
        zoom.state_idx + 1, STATE_COUNT,
        scale,
    );
    let hint_style = Style::default().fg(Color::Rgb(120, 120, 140));
    draw_text(area.x, area.y + area.height - 1, &hint, hint_style, area, buf);
}

/// Render a sprite using the component ID map for coloring.
fn render_sprite_with_components(
    sprite: &Sprite,
    comp: &[u8],
    area: Rect,
    buf: &mut ratatui::buffer::Buffer,
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

            let upper_id = comp[upper_y * sprite.width + col];
            let lower_id = if lower_y < sprite.height {
                comp[lower_y * sprite.width + col]
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
                    (CellKind::Empty, _) => {
                        cell.set_symbol("\u{2584}");
                        cell.set_style(Style::default().fg(component_color(lower_id)));
                    }
                    (_, CellKind::Empty) => {
                        cell.set_symbol("\u{2580}");
                        cell.set_style(Style::default().fg(component_color(upper_id)));
                    }
                    (_, _) => {
                        let fg = component_color(lower_id);
                        let bg = component_color(upper_id);
                        if fg == bg {
                            cell.set_symbol("\u{2588}");
                            cell.set_style(Style::default().fg(fg));
                        } else {
                            cell.set_symbol("\u{2584}");
                            cell.set_style(Style::default().fg(fg).bg(bg));
                        }
                    }
                }
            }
        }
    }
}
