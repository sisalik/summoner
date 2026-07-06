//! Per-session-state animation drivers, one submodule per archetype.
//!
//! Every archetype driver is fully kinematic: point positions are written
//! directly each tick with `prev_pos = pos` (no verlet, no constraint solve,
//! no vertical re-centering) so the pose is a pure function of `elapsed`.
//! See docs/creature-animation.md for the reasoning.

use std::time::Duration;

use crate::session::SessionState;
use super::generate::Xorshift;
use super::physics::{apply_constraints, verlet_integrate};
use super::skeleton::{Skeleton, Vec2};

mod bipedal;
mod blob;
mod quadruped;
mod serpentine;
mod winged;

use bipedal::{GaitStyle, IdleStyle};
use blob::BlobStyle;
use quadruped::QuadStyle;
use serpentine::SnakeStyle;
use winged::WingStyle;

/// Hash the rest pose into a personality PRNG: FNV-1a over the position/width
/// bits, salted so each style struct draws independently. Deterministic per
/// creature across restarts with no seed plumbing.
fn style_rng(skeleton: &Skeleton, salt: u64) -> Xorshift {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ salt;
    for pt in &skeleton.points {
        for v in [pt.pos.x, pt.pos.y, pt.width] {
            h = (h ^ v.to_bits() as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    Xorshift::new(h | 1) // xorshift dies on 0
}

fn pick(rng: &mut Xorshift, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * ((rng.next_u64() % 1000) as f32 / 1000.0)
}

/// Smoothstep ease of `u` clamped to [0, 1].
fn ease(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub struct LocomotionState {
    skeleton: Skeleton,
    base_widths: Vec<f32>,
    rest_positions: Vec<Vec2>,
    rest_effectors: Vec<Vec2>,
    rest_bend_dirs: Vec<f32>,
    rest_limb_widths: Vec<(f32, f32)>,
    rest_depths: Vec<i8>,
    gait: GaitStyle,
    idle: IdleStyle,
    quad: QuadStyle,
    wing: WingStyle,
    blob: BlobStyle,
    snake: SnakeStyle,
    state: SessionState,
    elapsed: f32,
    archetype: usize,
    settled: bool,
}

impl LocomotionState {
    pub fn new(skeleton: Skeleton, state: SessionState) -> Self {
        let base_widths: Vec<f32> = skeleton.points.iter().map(|p| p.width).collect();
        let rest_positions: Vec<Vec2> = skeleton.points.iter().map(|p| p.pos).collect();
        let rest_effectors: Vec<Vec2> = skeleton.limbs.iter().map(|l| l.end_effector).collect();
        let rest_bend_dirs: Vec<f32> = skeleton.limbs.iter().map(|l| l.bend_dir).collect();
        let rest_limb_widths: Vec<(f32, f32)> =
            skeleton.limbs.iter().map(|l| (l.upper_width, l.lower_width)).collect();
        let rest_depths: Vec<i8> = skeleton.limbs.iter().map(|l| l.depth).collect();
        let gait = GaitStyle::from_skeleton(&skeleton);
        let idle = IdleStyle::from_skeleton(&skeleton);
        let quad = QuadStyle::from_skeleton(&skeleton);
        let wing = WingStyle::from_skeleton(&skeleton);
        let blob = BlobStyle::from_skeleton(&skeleton);
        let snake = SnakeStyle::from_skeleton(&skeleton);
        let archetype = detect_archetype(&skeleton);
        let mut ls = Self {
            skeleton, base_widths, rest_positions, rest_effectors,
            rest_bend_dirs, rest_limb_widths, rest_depths, gait, idle,
            quad, wing, blob, snake,
            state, elapsed: 0.0, archetype, settled: false,
        };
        if state == SessionState::Disconnected {
            ls.settle_disconnected();
        }
        ls
    }

    pub fn state(&self) -> SessionState { self.state }
    pub fn skeleton(&self) -> &Skeleton { &self.skeleton }

    pub fn set_state(&mut self, state: SessionState) {
        if state == self.state { return; }
        self.state = state;
        self.elapsed = 0.0;
        self.settled = false;
        // Restore base widths and positions when changing state
        for (i, pt) in self.skeleton.points.iter_mut().enumerate() {
            pt.width = self.base_widths[i];
            pt.pos = self.rest_positions[i];
            pt.prev_pos = self.rest_positions[i];
        }
        for (i, limb) in self.skeleton.limbs.iter_mut().enumerate() {
            limb.end_effector = self.rest_effectors[i];
            limb.bend_dir = self.rest_bend_dirs[i];
            limb.upper_width = self.rest_limb_widths[i].0;
            limb.lower_width = self.rest_limb_widths[i].1;
            limb.depth = self.rest_depths[i];
        }
        if state == SessionState::Disconnected {
            self.settle_disconnected();
        }
    }

    pub fn tick(&mut self, dt: Duration) {
        self.elapsed += dt.as_secs_f32();
        match self.state {
            SessionState::Working => match self.archetype {
                1 => self.drive_working_quadruped(),
                2 => self.drive_working_blob(),
                3 => self.drive_working_winged(),
                4 => self.drive_working_serpentine(),
                _ => self.drive_working_bipedal(),
            },
            SessionState::Waiting => match self.archetype {
                1 => self.drive_waiting_quadruped(),
                2 => self.drive_waiting_blob(),
                3 => self.drive_waiting_winged(),
                4 => self.drive_waiting_serpentine(),
                _ => self.drive_waiting_bipedal(),
            },
            SessionState::Idle => match self.archetype {
                1 => self.drive_idle_quadruped(),
                2 => self.drive_idle_blob(),
                3 => self.drive_idle_winged(),
                4 => self.drive_idle_serpentine(),
                _ => self.drive_idle_bipedal(),
            },
            SessionState::Sleeping => match self.archetype {
                1 => self.drive_sleeping_quadruped(),
                2 => self.drive_sleeping_blob(),
                3 => self.drive_sleeping_winged(),
                4 => self.drive_sleeping_serpentine(),
                _ => self.drive_sleeping_bipedal(),
            },
            SessionState::Disconnected => {}
            SessionState::ShellOnly => {}
        }
    }

    /// The rest X center of the skeleton (head rest position).
    fn rest_center_x(&self) -> f32 {
        self.rest_positions[0].x
    }

    /// Compute a fixed ground Y for limbed archetypes from rest positions.
    /// Uses the lowest rest effector Y as the ground line.
    fn ground_y(&self) -> f32 {
        self.rest_effectors.iter()
            .map(|e| e.y)
            .fold(0.0f32, f32::max)
            .min(23.0)
    }

    /// Write a point kinematically: position set directly, no residual verlet
    /// velocity.
    fn put_point(&mut self, i: usize, pos: Vec2) {
        let pt = &mut self.skeleton.points[i];
        pt.pos = pos;
        pt.prev_pos = pos;
    }

    // --- Disconnected: settle and freeze ---

    fn settle_disconnected(&mut self) {
        for _ in 0..10 {
            verlet_integrate(&mut self.skeleton, 0.5, Vec2::new(0.0, 0.8));
            apply_constraints(&mut self.skeleton, 3);
            self.clamp_to_bounds();
        }
        for pt in &mut self.skeleton.points { pt.prev_pos = pt.pos; }
        self.settled = true;
    }

    fn clamp_to_bounds(&mut self) {
        for pt in &mut self.skeleton.points {
            pt.pos.x = pt.pos.x.clamp(0.0, 17.0);
            pt.pos.y = pt.pos.y.clamp(0.0, 23.0);
        }
        for limb in &mut self.skeleton.limbs {
            limb.end_effector.x = limb.end_effector.x.clamp(0.0, 17.0);
            limb.end_effector.y = limb.end_effector.y.clamp(0.0, 23.0);
        }
    }
}

fn detect_archetype(skeleton: &Skeleton) -> usize {
    let is_ring = skeleton.limbs.is_empty() && skeleton.constraints.iter().any(|c| {
        let n = skeleton.points.len();
        (c.a == n - 1 && c.b == 0) || (c.a == 0 && c.b == n - 1)
    });
    if is_ring { return 2; }
    if skeleton.limbs.is_empty() && skeleton.points.len() >= 6 { return 4; }
    if skeleton.points.iter().any(|p| p.name.starts_with("lwing")) { return 3; }
    if skeleton.limbs.len() == 4 && skeleton.points.iter().any(|p| p.name == "tail_1") { return 1; }
    0
}
