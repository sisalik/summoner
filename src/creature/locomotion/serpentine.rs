//! Serpentine drivers: traveling-wave slither (amplitude grows toward the
//! tail, head stays steady), a snake-charmer periscope while waiting, a lazy
//! S-drift idle, and a sleep that eases into a real spiral coil. All fully
//! kinematic.
//!
//! Skeleton: horizontal chain, 0 head (small x, facing -x) .. n-1 tail.

use super::*;

/// Per-creature serpentine personality (salted rest-pose hash).
#[derive(Clone, Copy)]
pub(super) struct SnakeStyle {
    // Working: slither.
    freq: f32,       // wave passes per second
    wavelength: f32, // waves along the body
    amp: f32,        // peak amplitude at the tail
    // Waiting: periscope.
    bend: f32,       // total neck curl at the head, radians
    scan_freq: f32,  // side-to-side scanning
    scan_amp: f32,   // scanning angle, radians
    // Idle.
    drift: f32,      // lazy S amplitude
    flick: f32,      // ~35%: quick head-flick quirk, else 0
    breath_freq: f32,
    // Sleeping: coil.
    coil_r: f32,     // outer coil radius
    pitch: f32,      // how fast the spiral tightens
    twitch: f32,     // ~25%: sleepy tail-tip twitch, else 0
}

impl SnakeStyle {
    pub(super) fn from_skeleton(skeleton: &Skeleton) -> Self {
        let mut rng = style_rng(skeleton, 0xd6e8_feb8_6659_fd93);
        let mut pick = |lo: f32, hi: f32| pick(&mut rng, lo, hi);
        Self {
            freq: pick(0.7, 1.4),
            wavelength: pick(1.2, 1.9),
            amp: pick(1.2, 2.0),
            bend: pick(0.7, 1.2),
            scan_freq: pick(0.14, 0.28),
            scan_amp: pick(0.25, 0.5),
            drift: pick(0.4, 0.9),
            flick: if pick(0.0, 1.0) < 0.35 { pick(0.8, 1.4) } else { 0.0 },
            breath_freq: pick(0.15, 0.3),
            coil_r: pick(3.0, 3.8),
            pitch: pick(0.16, 0.22),
            twitch: if pick(0.0, 1.0) < 0.25 { pick(0.4, 0.8) } else { 0.0 },
        }
    }
}

impl LocomotionState {
    /// Slither: a wave travels head-to-tail, growing as it goes (real snakes
    /// amplify posteriorly — the old even taper read as a wobbling stick).
    /// A small quadrature x component rolls each point in a shallow ellipse,
    /// which sells the wave as pushing, not just bobbing.
    pub(super) fn drive_working_serpentine(&mut self) {
        use std::f32::consts::TAU;
        let s = self.snake;
        let t = self.elapsed;
        let n = self.skeleton.points.len();

        for i in 0..n {
            let u = i as f32 / (n - 1).max(1) as f32;
            let phase = TAU * s.freq * t - u * TAU * s.wavelength;
            let grow = 0.3 + 0.7 * u; // head steady, tail whips
            let rest = self.rest_positions[i];
            self.put_point(i, Vec2::new(
                rest.x + 0.45 * grow * (phase + TAU * 0.25).sin(),
                rest.y + s.amp * grow * phase.sin(),
            ));
            self.skeleton.points[i].width = self.base_widths[i];
        }

        self.clamp_to_bounds();
    }

    /// Periscope: the front of the chain curls up off the ground segment by
    /// segment (exact segment lengths — no stretch) and the raised head
    /// sways side to side, scanning. The rest of the body keeps a faint
    /// wave so it doesn't die.
    pub(super) fn drive_waiting_serpentine(&mut self) {
        use std::f32::consts::{PI, TAU};
        let s = self.snake;
        let t = self.elapsed;
        let n = self.skeleton.points.len();
        if n < 5 {
            return;
        }
        let k = ease(t / 1.2);

        // Body behind the neck: gentle settled wave.
        for i in 3..n {
            let u = i as f32 / (n - 1) as f32;
            let rest = self.rest_positions[i];
            let wave = 0.4 * (TAU * 0.22 * t - u * TAU).sin();
            self.put_point(i, Vec2::new(rest.x, rest.y + wave * (0.3 + 0.7 * u)));
        }

        // Raise head + first two segments as a rigid-length chain from point
        // 3, curling from horizontal toward vertical; scanning adds a slow
        // angle sway shared down the neck (snake-charmer style).
        let scan = s.scan_amp * (TAU * s.scan_freq * t).sin() * k;
        let base = self.skeleton.points[3].pos;
        let mut pos = base;
        for (step, i) in [2usize, 1, 0].iter().enumerate().map(|(j, &i)| (j + 1, i)) {
            let seg_len = (self.rest_positions[i] - self.rest_positions[i + 1]).length();
            let curl = k * s.bend * (step as f32 / 3.0) + scan * (step as f32 / 3.0);
            let ang = PI + curl; // PI = flat toward -x; +curl lifts (y-down)
            pos = pos + Vec2::new(ang.cos() * seg_len, ang.sin() * seg_len);
            self.put_point(i, pos);
        }

        self.clamp_to_bounds();
    }

    /// Lazy S: a slow, low drift wave, breathing widths, and for some a
    /// sudden little head flick (tongue-tasting the air) on a rare gate.
    pub(super) fn drive_idle_serpentine(&mut self) {
        use std::f32::consts::TAU;
        let s = self.snake;
        let t = self.elapsed;
        let n = self.skeleton.points.len();

        let flick_gate = (TAU * 0.055 * t + 2.7).sin();
        let flick = if s.flick > 0.0 && flick_gate > 0.93 {
            s.flick * ease((flick_gate - 0.93) / 0.07) * (TAU * 5.0 * t).sin().abs()
        } else {
            0.0
        };
        let breath = (TAU * s.breath_freq * t).sin();

        for i in 0..n {
            let u = i as f32 / (n - 1).max(1) as f32;
            let rest = self.rest_positions[i];
            // Two incommensurate slow waves so the drift never loops.
            let wave = s.drift
                * ((TAU * 0.11 * t - u * TAU * 0.9).sin()
                    + 0.4 * (TAU * 0.19 * t - u * TAU * 1.3).sin());
            let head_flick = if i == 0 { -flick } else if i == 1 { -flick * 0.4 } else { 0.0 };
            self.put_point(i, Vec2::new(rest.x + head_flick, rest.y + wave));
            self.skeleton.points[i].width = self.base_widths[i] + breath * 0.15;
        }

        self.clamp_to_bounds();
    }

    /// Coils up to sleep: every point eases from the resting line onto an
    /// arc-length-preserving spiral (tail outermost, head settling in the
    /// middle on top), then breathes almost imperceptibly.
    pub(super) fn drive_sleeping_serpentine(&mut self) {
        use std::f32::consts::TAU;
        let s = self.snake;
        let t = self.elapsed;
        let n = self.skeleton.points.len();
        if n < 3 {
            return;
        }
        let k = ease(t / 2.5);
        let breath = (TAU * 0.07 * t).sin();

        // Build the spiral from the tail inward, stepping theta by arc
        // length so consecutive points keep their rest spacing.
        let center = Vec2::new(9.5, 16.0);
        let mut targets = vec![Vec2::zero(); n];
        let mut radius = s.coil_r;
        let mut theta: f32 = 1.2;
        targets[n - 1] = center + Vec2::new(theta.cos() * radius, theta.sin() * radius * 0.85);
        for i in (0..n - 1).rev() {
            let seg_len = (self.rest_positions[i] - self.rest_positions[i + 1]).length();
            theta += seg_len / radius.max(0.8);
            radius = (radius - seg_len * s.pitch).max(1.2);
            targets[i] = center + Vec2::new(theta.cos() * radius, theta.sin() * radius * 0.85);
        }

        // Sleepy tail-tip twitch on a rare gate, only once coiled.
        let tw_gate = (TAU * 0.045 * t + 0.4).sin();
        let twitch = if s.twitch > 0.0 && tw_gate > 0.95 {
            s.twitch * ease((tw_gate - 0.95) / 0.05) * k
        } else {
            0.0
        };

        for i in 0..n {
            let rest = self.rest_positions[i];
            let mut pos = Vec2::new(
                lerp(rest.x, targets[i].x, k),
                lerp(rest.y, targets[i].y, k) + breath * 0.15 * k,
            );
            if i == n - 1 {
                pos.y -= twitch;
            }
            self.put_point(i, pos);
            self.skeleton.points[i].width = self.base_widths[i] + breath * 0.1 * k;
        }

        self.clamp_to_bounds();
    }
}
