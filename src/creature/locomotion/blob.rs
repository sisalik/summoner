//! Blob drivers: rolling-boil work, real squash-launch-land hops while
//! waiting, breathing idle, and a melt-to-puddle sleep. All fully kinematic —
//! the old verlet pass only smeared the wave (see docs).
//!
//! Skeleton: a closed ring of points around a center, no limbs.

use super::*;

/// Per-creature blob personality (salted rest-pose hash).
#[derive(Clone, Copy)]
pub(super) struct BlobStyle {
    // Working: traveling bulge.
    wave_freq: f32, // revolutions of the bulge per second
    wave_amp: f32,  // radial bulge size
    lobes: f32,     // 2 or 3 bulges around the ring
    squish: f32,    // width modulation riding the wave
    lean: f32,      // slow whole-body lean
    // Waiting: hops.
    hop_freq: f32,  // hops per second
    hop_amp: f32,   // requested hop height (clamped to canvas headroom)
    squash: f32,    // crouch/land squash depth
    // Idle.
    breath_freq: f32,
    breath_amp: f32, // radial breathing, fraction
    jiggle: f32,     // ~30%: occasional jelly shiver, else 0
    // Sleeping.
    flatten: f32,   // how much height the puddle loses
    ripple: f32,    // sleepy surface ripple
}

impl BlobStyle {
    pub(super) fn from_skeleton(skeleton: &Skeleton) -> Self {
        let mut rng = style_rng(skeleton, 0x2545_f491_4f6c_dd1d);
        let mut pick = |lo: f32, hi: f32| pick(&mut rng, lo, hi);
        Self {
            wave_freq: pick(0.7, 1.5),
            wave_amp: pick(0.8, 1.5),
            lobes: if pick(0.0, 1.0) < 0.5 { 2.0 } else { 3.0 },
            squish: pick(0.5, 1.1),
            lean: pick(0.3, 0.9),
            hop_freq: pick(0.55, 1.05),
            hop_amp: pick(1.5, 3.2),
            squash: pick(0.22, 0.38),
            breath_freq: pick(0.14, 0.28),
            breath_amp: pick(0.04, 0.09),
            jiggle: if pick(0.0, 1.0) < 0.3 { pick(0.5, 0.9) } else { 0.0 },
            flatten: pick(0.35, 0.55),
            ripple: pick(0.1, 0.25),
        }
    }
}

impl LocomotionState {
    /// Mean rest center of the ring.
    fn blob_center(&self) -> Vec2 {
        let n = self.rest_positions.len().max(1) as f32;
        let sum = self.rest_positions.iter()
            .fold(Vec2::zero(), |acc, p| acc + *p);
        sum * (1.0 / n)
    }

    /// Lowest rest point — the blob's ground contact line.
    fn blob_bottom(&self) -> f32 {
        self.rest_positions.iter().map(|p| p.y).fold(0.0f32, f32::max)
    }

    /// Highest rest point — headroom limit for hops.
    fn blob_top(&self) -> f32 {
        self.rest_positions.iter().map(|p| p.y).fold(24.0f32, f32::min)
    }

    /// Rolling boil: a bulge travels around the ring (radial push + width
    /// pulse in quadrature) over a slow squash-stretch and lean, all on
    /// incommensurate periods so the composite never visibly loops.
    pub(super) fn drive_working_blob(&mut self) {
        use std::f32::consts::TAU;
        let s = self.blob;
        let t = self.elapsed;
        let c = self.blob_center();
        let n = self.skeleton.points.len();

        // Global squash & stretch, area-conserving-ish.
        let sy = 1.0 + 0.07 * (TAU * 0.43 * t).sin();
        let sx = 1.0 / sy;
        let lean = s.lean * (TAU * 0.23 * t).sin();

        for i in 0..n {
            let rest = self.rest_positions[i];
            let d = rest - c;
            let ang = d.y.atan2(d.x);
            // Traveling wave: bulge phase advances around the ring.
            let phase = ang * s.lobes - TAU * s.wave_freq * t;
            let radial = s.wave_amp * phase.sin();
            let len = d.length().max(0.1);
            let dir = d * (1.0 / len);
            let pos = Vec2::new(
                c.x + (d.x + dir.x * radial) * sx + lean,
                c.y + (d.y + dir.y * radial) * sy,
            );
            self.put_point(i, pos);
            // Width pulses a quarter turn behind the bulge.
            self.skeleton.points[i].width =
                (self.base_widths[i] + s.squish * (phase + TAU * 0.25).sin()).max(0.5);
        }

        self.clamp_to_bounds();
    }

    /// Proper hops: crouch-squash into the ground, launch with a stretch,
    /// land with an impact squash — bottom-anchored throughout, widths
    /// swell as the body flattens (volume conservation sells it).
    pub(super) fn drive_waiting_blob(&mut self) {
        use std::f32::consts::PI;
        let s = self.blob;
        let t = self.elapsed;
        let c = self.blob_center();
        let bottom = self.blob_bottom();
        // The ring top must stay on-canvas at the hop apex.
        let hop = s.hop_amp.min((self.blob_top() - 1.5).max(0.0));

        let p = (t * s.hop_freq).rem_euclid(1.0);
        // Piecewise cycle: [0,0.25) anticipation crouch, [0.25,0.65) airborne,
        // [0.65,1.0) landing squash and recovery. Continuous at the seams.
        let (lift, sy) = if p < 0.25 {
            (0.0, 1.0 - s.squash * ease(p / 0.25))
        } else if p < 0.65 {
            let v = (p - 0.25) / 0.4;
            let air_sy = 1.0 + 0.55 * s.squash * (PI * v).sin();
            // Blend out of the crouch squash in the first instants of flight.
            let sy = lerp(1.0 - s.squash, air_sy, (v / 0.15).min(1.0));
            (hop * (PI * v).sin(), sy)
        } else {
            let w = (p - 0.65) / 0.35;
            (0.0, 1.0 - 0.85 * s.squash * (PI * w).sin())
        };
        let sx = 1.0 + (1.0 - sy) * 0.6; // squash widens, stretch narrows

        let n = self.skeleton.points.len();
        for i in 0..n {
            let rest = self.rest_positions[i];
            let d = rest - c;
            self.put_point(i, Vec2::new(
                c.x + d.x * sx,
                bottom - (bottom - rest.y) * sy - lift,
            ));
            self.skeleton.points[i].width =
                (self.base_widths[i] * (1.0 + (1.0 - sy) * 0.5)).max(0.5);
        }

        self.clamp_to_bounds();
    }

    /// Resting jelly: slow bottom-anchored breathing over a couple of faint
    /// incommensurate wobbles; jigglers shiver briefly on a rare gate.
    pub(super) fn drive_idle_blob(&mut self) {
        use std::f32::consts::TAU;
        let s = self.blob;
        let t = self.elapsed;
        let c = self.blob_center();
        let bottom = self.blob_bottom();

        let breath = s.breath_amp * (TAU * s.breath_freq * t).sin();
        let jig_gate = (TAU * 0.13 * t + 3.3).sin();
        let jiggle = if s.jiggle > 0.0 && jig_gate > 0.9 {
            s.jiggle * ease((jig_gate - 0.9) / 0.1)
        } else {
            0.0
        };

        let n = self.skeleton.points.len();
        for i in 0..n {
            let rest = self.rest_positions[i];
            let d = rest - c;
            let ang = d.y.atan2(d.x);
            // Faint asymmetric wobble so the surface never sits dead still.
            let wob = 0.15 * (TAU * 0.21 * t + ang).sin()
                + 0.1 * (TAU * 0.34 * t + ang * 2.0).sin()
                + jiggle * (TAU * 4.5 * t + ang * 2.0).sin() * 0.8;
            self.put_point(i, Vec2::new(
                c.x + d.x * (1.0 + breath * 0.6 + wob * 0.1),
                bottom - (bottom - rest.y) * (1.0 + breath) + wob * 0.5,
            ));
            self.skeleton.points[i].width = self.base_widths[i];
        }

        self.clamp_to_bounds();
    }

    /// Melts into a puddle: height eases away into extra width, then the
    /// surface barely ripples with slow breathing.
    pub(super) fn drive_sleeping_blob(&mut self) {
        use std::f32::consts::TAU;
        let s = self.blob;
        let t = self.elapsed;
        let c = self.blob_center();
        let bottom = self.blob_bottom();

        let k = ease(t / 2.5);
        let breath = 0.03 * (TAU * 0.08 * t).sin();
        let sy = lerp(1.0, 1.0 - s.flatten, k) + breath;
        let sx = lerp(1.0, 1.0 + s.flatten * 0.9, k);

        let n = self.skeleton.points.len();
        for i in 0..n {
            let rest = self.rest_positions[i];
            let d = rest - c;
            let ang = d.y.atan2(d.x);
            let ripple = s.ripple * k * (TAU * 0.11 * t + ang * 2.0).sin();
            self.put_point(i, Vec2::new(
                c.x + d.x * sx,
                bottom - (bottom - rest.y) * sy + ripple,
            ));
            self.skeleton.points[i].width =
                (self.base_widths[i] * (1.0 + 0.3 * s.flatten * k)).max(0.5);
        }

        self.clamp_to_bounds();
    }
}
