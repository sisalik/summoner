use std::time::Duration;

use super::generate::{CellKind, Sprite};
use crate::session::SessionState;

pub struct AnimationState {
    state: SessionState,
    frame: usize,
    elapsed: Duration,
}

impl AnimationState {
    pub fn new(state: SessionState) -> Self {
        Self { state, frame: 0, elapsed: Duration::ZERO }
    }

    pub fn current_frame(&self) -> usize {
        self.frame
    }

    pub fn total_frames(&self) -> usize {
        frame_count(self.state)
    }

    pub fn set_state(&mut self, state: SessionState) {
        if state != self.state {
            self.state = state;
            self.frame = 0;
            self.elapsed = Duration::ZERO;
        }
    }

    pub fn tick(&mut self, dt: Duration) {
        let duration = frame_duration(self.state);
        if duration == Duration::ZERO { return; }
        self.elapsed += dt;
        if self.elapsed >= duration {
            self.elapsed -= duration;
            self.frame = (self.frame + 1) % frame_count(self.state);
        }
    }
}

fn frame_count(state: SessionState) -> usize {
    match state {
        SessionState::Working => 4,
        SessionState::Waiting => 3,
        SessionState::Idle => 2,
        SessionState::Sleeping => 3,
        SessionState::Disconnected => 1,
        SessionState::ShellOnly => 1,
    }
}

fn frame_duration(state: SessionState) -> Duration {
    match state {
        SessionState::Working => Duration::from_millis(200),
        SessionState::Waiting => Duration::from_millis(300),
        SessionState::Idle => Duration::from_millis(800),
        SessionState::Sleeping => Duration::from_millis(600),
        SessionState::Disconnected => Duration::ZERO,
        SessionState::ShellOnly => Duration::ZERO,
    }
}

pub fn animate_sprite(base: &Sprite, anim: &AnimationState) -> Sprite {
    match anim.state {
        SessionState::Working => animate_locomotion(base, anim.frame),
        SessionState::Waiting => animate_bounce(base, anim.frame),
        SessionState::Idle => animate_breathe(base, anim.frame),
        SessionState::Sleeping => animate_sleep(base, anim.frame),
        SessionState::Disconnected => base.clone(),
        SessionState::ShellOnly => base.clone(),
    }
}

fn animate_locomotion(base: &Sprite, frame: usize) -> Sprite {
    let offset: i32 = match frame { 0 => 0, 1 => 1, 2 => 0, 3 => -1, _ => 0 };
    shift_horizontal(base, offset)
}

fn animate_bounce(base: &Sprite, frame: usize) -> Sprite {
    let shift_up = match frame { 0 => 0, 1 => 2, 2 => 1, _ => 0 };
    shift_vertical(base, shift_up)
}

fn animate_breathe(base: &Sprite, frame: usize) -> Sprite {
    if frame == 0 {
        base.clone()
    } else {
        let mid = base.height / 2;
        let new_height = base.height + 1;
        let mut cells = Vec::with_capacity(base.width * new_height);
        for y in 0..new_height {
            let src_y = if y <= mid { y } else { y - 1 };
            for x in 0..base.width {
                cells.push(base.get(x, src_y));
            }
        }
        Sprite { width: base.width, height: new_height, cells }
    }
}

fn animate_sleep(base: &Sprite, frame: usize) -> Sprite {
    let start_row = 1.min(base.height.saturating_sub(1));
    let new_height = base.height - start_row;
    let mut cells = Vec::with_capacity(base.width * new_height);
    for y in start_row..base.height {
        for x in 0..base.width {
            cells.push(base.get(x, y));
        }
    }
    let _ = frame;
    Sprite { width: base.width, height: new_height, cells }
}

fn shift_horizontal(sprite: &Sprite, offset: i32) -> Sprite {
    let mut cells = vec![CellKind::Empty; sprite.width * sprite.height];
    for y in 0..sprite.height {
        for x in 0..sprite.width {
            let src_x = x as i32 - offset;
            if src_x >= 0 && (src_x as usize) < sprite.width {
                cells[y * sprite.width + x] = sprite.get(src_x as usize, y);
            }
        }
    }
    Sprite { width: sprite.width, height: sprite.height, cells }
}

fn shift_vertical(sprite: &Sprite, up_pixels: usize) -> Sprite {
    let new_height = sprite.height + up_pixels;
    let mut cells = vec![CellKind::Empty; sprite.width * new_height];
    for y in 0..sprite.height {
        for x in 0..sprite.width {
            cells[y * sprite.width + x] = sprite.get(x, y);
        }
    }
    Sprite { width: sprite.width, height: new_height, cells }
}
