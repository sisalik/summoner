//! Winged drivers: hovering flight while working, perched bounce while
//! waiting, folded-wing idle, head-under-wing sleep. All fully kinematic.
//!
//! Skeleton layout (front view): 0 head, 1 neck, 2 body, 3..5 left wing,
//! 6..8 right wing (inner to tip). Limbs: 0 left leg, 1 right leg.

use super::*;

/// Per-creature winged personality (salted rest-pose hash, like the others).
#[derive(Clone, Copy)]
pub(super) struct WingStyle {
    // Working: hover.
    flap_freq: f32, // beats per second
    flap_amp: f32,  // inner-segment amplitude; tips scale up from this
    fold: f32,      // horizontal wing pull-in on the upstroke
    bob: f32,       // body bob depth (downward-biased; no headroom above)
    glide: f32,     // ~30%: occasional glide pause depth, else 0
    // Waiting: perched excitement.
    bounce_freq: f32,
    bounce_amp: f32,
    hop_freq: f32,  // rare feet-tuck hop gate
    hop_amp: f32,
    flutter: f32,   // ~40%: burst of tiny fast flaps, else 0
    dart: f32,      // birdy head-dart amplitude
    // Idle.
    breath_freq: f32,
    breath_amp: f32,
    ruffle: f32,    // ~30%: occasional feather shake, else 0
    glance: f32,
    // Sleeping.
    tuck_dir: f32,  // which wing the head tucks toward (+/-1)
    droop: f32,     // wing droop depth
}

impl WingStyle {
    pub(super) fn from_skeleton(skeleton: &Skeleton) -> Self {
        let mut rng = style_rng(skeleton, 0x517c_c1b7_2722_0a95);
        let mut pick = |lo: f32, hi: f32| pick(&mut rng, lo, hi);
        Self {
            flap_freq: pick(1.2, 2.6),
            flap_amp: pick(1.2, 2.2),
            fold: pick(0.3, 1.0),
            bob: pick(0.8, 1.6),
            glide: if pick(0.0, 1.0) < 0.3 { pick(0.7, 1.0) } else { 0.0 },
            bounce_freq: pick(1.0, 2.0),
            bounce_amp: pick(0.6, 1.4),
            hop_freq: pick(0.09, 0.21),
            hop_amp: pick(1.2, 2.4),
            flutter: if pick(0.0, 1.0) < 0.4 { pick(0.8, 1.5) } else { 0.0 },
            dart: pick(0.8, 1.8),
            breath_freq: pick(0.18, 0.32),
            breath_amp: pick(0.15, 0.35),
            ruffle: if pick(0.0, 1.0) < 0.3 { pick(0.5, 0.9) } else { 0.0 },
            glance: pick(0.7, 1.6),
            tuck_dir: if pick(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 },
            droop: pick(1.5, 2.8),
        }
    }
}

impl LocomotionState {
    /// Hover: wings beat with a cascading phase delay (tips trail, move
    /// most), the body bobs *against* the beat — downward only, the head
    /// already rides near the canvas top — feet tuck up and dangle, and the
    /// head stays steadier than the body (birds stabilize their heads).
    /// Gliders pause mid-beat now and then, wings held high, sinking softly.
    pub(super) fn drive_working_winged(&mut self) {
        use std::f32::consts::TAU;
        if self.skeleton.points.len() < 9 || self.skeleton.limbs.len() < 2 {
            return;
        }
        let s = self.wing;
        let t = self.elapsed;
        let cx = self.rest_center_x();
        let rp = self.rest_positions.clone();
        let r = |i: usize| rp[i];

        // Glide gate: flap fades out, wings hold raised, body sinks.
        let glide_gate = (TAU * 0.07 * t + 0.9).sin();
        let glide = if s.glide > 0.0 && glide_gate > 0.86 {
            s.glide * ease((glide_gate - 0.86) / 0.14)
        } else {
            0.0
        };
        let flap = TAU * s.flap_freq * t;

        // Body bobs counter-phase to the wings, biased strictly downward.
        let bob = s.bob * (0.5 + 0.5 * (flap + TAU * 0.5).sin()) * (1.0 - glide)
            + 1.2 * glide;
        // Head stabilization: follows only a quarter of the body bob.
        self.put_point(0, Vec2::new(
            r(0).x + 0.3 * (TAU * 0.19 * t).sin(),
            r(0).y + bob * 0.25,
        ));
        self.put_point(1, Vec2::new(r(1).x, r(1).y + bob * 0.6));
        self.put_point(2, Vec2::new(r(2).x, r(2).y + bob));

        // Wings: amplitude and delay grow toward the tip; on the upstroke
        // the wing folds slightly inward. Gliding holds them up and out.
        for (base, side) in [(3usize, -1.0f32), (6, 1.0)] {
            for seg in 0..3usize {
                let i = base + seg;
                let rest = r(i);
                let kf = seg as f32 + 1.0;
                let amp = s.flap_amp * (0.35 + 0.55 * kf);
                let ph = (flap - kf * 0.45).sin();
                let y = rest.y + bob * 0.5
                    + (amp * ph) * (1.0 - glide)
                    - (1.2 + 0.8 * kf) * 0.8 * glide; // held high in a glide
                let inward = s.fold * 0.3 * kf * (0.5 - 0.5 * ph) * (1.0 - glide);
                self.put_point(i, Vec2::new(rest.x - side * inward, y.max(1.0)));
            }
        }

        // Feet tuck up and dangle with a lagged sway — airborne, not standing.
        let body_y = self.skeleton.points[2].pos.y;
        let leg_len = self.skeleton.limbs[0].upper_len + self.skeleton.limbs[0].lower_len;
        let dangle = body_y + leg_len * 0.55;
        let sway = 0.4 * (flap - 1.2).sin();
        for (li, side) in [(0usize, -1.0f32), (1, 1.0)] {
            self.skeleton.limbs[li].end_effector =
                Vec2::new(cx + side * 1.2 + sway, dangle);
            self.skeleton.limbs[li].bend_dir = side; // knees tuck outward
        }

        self.clamp_to_bounds();
    }

    /// Perched and keen: bouncy knee-dips, quick birdy head darts, wing
    /// flutter bursts, and the odd feet-tuck hop.
    pub(super) fn drive_waiting_winged(&mut self) {
        use std::f32::consts::TAU;
        if self.skeleton.points.len() < 9 || self.skeleton.limbs.len() < 2 {
            return;
        }
        let s = self.wing;
        let t = self.elapsed;
        let gy = self.ground_y();
        let rp = self.rest_positions.clone();
        let r = |i: usize| rp[i];

        // Downward knee-dip bounce (no headroom to rise).
        let dip = (TAU * s.bounce_freq * t).sin().max(0.0) * s.bounce_amp;
        // Head darts: cubed sine dwells, then flicks — very bird.
        let d = (TAU * 0.16 * t).sin();
        let dart = s.dart * d * d * d;
        // Flutter burst on its own slow gate.
        let flut_gate = (TAU * 0.13 * t + 2.3).sin();
        let flutter = if s.flutter > 0.0 && flut_gate > 0.8 {
            s.flutter * ease((flut_gate - 0.8) / 0.2)
        } else {
            0.0
        };
        // Rare hop: feet tuck, body holds (cartoon knee-tuck hop).
        let hop_gate = (TAU * s.hop_freq * t).sin();
        let hop = if hop_gate > 0.92 { s.hop_amp } else { 0.0 };

        self.put_point(0, Vec2::new(r(0).x + dart, r(0).y + dip));
        self.put_point(1, Vec2::new(r(1).x + dart * 0.4, r(1).y + dip));
        self.put_point(2, Vec2::new(r(2).x, r(2).y + dip));

        // Wings ride the body; flutter adds tiny fast beats at the tips.
        for (base, _side) in [(3usize, -1.0f32), (6, 1.0)] {
            for seg in 0..3usize {
                let i = base + seg;
                let rest = r(i);
                let kf = seg as f32 + 1.0;
                let fl = flutter * kf * 0.5 * (TAU * 7.0 * t - kf * 0.5).sin();
                self.put_point(i, Vec2::new(rest.x, rest.y + dip * 0.8 + fl));
            }
        }

        // Anticipation squash at the bottom of the dip.
        self.skeleton.points[2].width = self.base_widths[2] + dip * 0.25;

        for li in 0..2 {
            let rest = self.rest_effectors[li];
            self.skeleton.limbs[li].end_effector = Vec2::new(rest.x, gy - hop);
        }

        self.clamp_to_bounds();
    }

    /// Calm perch: wings ease in against the body, chest breathes, the head
    /// glances about; rufflers shake their feathers out now and then.
    pub(super) fn drive_idle_winged(&mut self) {
        use std::f32::consts::TAU;
        if self.skeleton.points.len() < 9 || self.skeleton.limbs.len() < 2 {
            return;
        }
        let s = self.wing;
        let t = self.elapsed;
        let gy = self.ground_y();
        let cx = self.rest_center_x();
        let rp = self.rest_positions.clone();
        let r = |i: usize| rp[i];

        let settle = ease(t / 1.5); // wings fold in over the first moments
        let breath = (TAU * s.breath_freq * t).sin();
        // Glance: cubed sine dart on a slow period.
        let g = (TAU * 0.041 * t).sin();
        let glance = s.glance * g * g * g;
        // Ruffle: quick shiver through the wings on a rare gate.
        let ruf_gate = (TAU * 0.06 * t + 4.1).sin();
        let ruffle = if s.ruffle > 0.0 && ruf_gate > 0.9 {
            s.ruffle * ease((ruf_gate - 0.9) / 0.1)
        } else {
            0.0
        };

        self.put_point(0, Vec2::new(r(0).x + glance, r(0).y));
        self.put_point(1, Vec2::new(r(1).x + glance * 0.3, r(1).y));
        self.put_point(2, Vec2::new(r(2).x, r(2).y));

        let body_y = r(2).y;
        for (base, side) in [(3usize, -1.0f32), (6, 1.0)] {
            for seg in 0..3usize {
                let i = base + seg;
                let rest = r(i);
                let kf = seg as f32 + 1.0;
                // Folded target: tips pull in beside the body, slightly low.
                let tgt = Vec2::new(
                    cx + side * (1.6 + 0.8 * kf) * 0.8,
                    body_y - 0.4 + 0.5 * kf * 0.4,
                );
                let x = lerp(rest.x, tgt.x, 0.55 * settle);
                let y = lerp(rest.y, tgt.y, 0.55 * settle)
                    + breath * 0.2 * kf * 0.4
                    + ruffle * kf * 0.4 * (TAU * 9.0 * t + kf).sin();
                self.put_point(i, Vec2::new(x, y));
            }
        }

        self.skeleton.points[2].width = self.base_widths[2] + breath * s.breath_amp;

        for li in 0..2 {
            let rest = self.rest_effectors[li];
            self.skeleton.limbs[li].end_effector = Vec2::new(rest.x, gy);
        }

        self.clamp_to_bounds();
    }

    /// Roost: the head sinks and tucks toward one wing, wings droop low
    /// around the body, slow breathing with an occasional shuffle.
    pub(super) fn drive_sleeping_winged(&mut self) {
        use std::f32::consts::TAU;
        if self.skeleton.points.len() < 9 || self.skeleton.limbs.len() < 2 {
            return;
        }
        let s = self.wing;
        let t = self.elapsed;
        let gy = self.ground_y();
        let rp = self.rest_positions.clone();
        let r = |i: usize| rp[i];

        let k = ease(t / 2.0);
        let breath = (TAU * 0.09 * t).sin();
        // Occasional sleepy shuffle: whole body sways a touch.
        let shuffle = 0.25 * (TAU * 0.05 * t + 1.1).sin() * k;

        // Head tucks down and sideways toward the chosen wing.
        self.put_point(0, Vec2::new(
            r(0).x + (2.2 * s.tuck_dir) * k + shuffle,
            r(0).y + 2.0 * k,
        ));
        self.put_point(1, Vec2::new(r(1).x + 1.0 * s.tuck_dir * k + shuffle, r(1).y + 1.2 * k));
        self.put_point(2, Vec2::new(r(2).x + shuffle, r(2).y + 0.8 * k));

        // Wings droop, tips most; they also settle slightly inward.
        let cx = self.rest_center_x();
        for (base, side) in [(3usize, -1.0f32), (6, 1.0)] {
            for seg in 0..3usize {
                let i = base + seg;
                let rest = r(i);
                let kf = seg as f32 + 1.0;
                let droop = s.droop * (0.4 + 0.35 * kf) * k;
                let inward = 0.25 * kf * k;
                self.put_point(i, Vec2::new(
                    lerp(rest.x, cx + side * (rest.x - cx).abs() * 0.85, k) - side * inward,
                    rest.y + droop + breath * 0.1 * kf * k,
                ));
            }
        }

        // Slow chest breathing; legs stay planted (knees flex as body sags).
        self.skeleton.points[2].width = self.base_widths[2] + breath * 0.15;
        for li in 0..2 {
            let rest = self.rest_effectors[li];
            self.skeleton.limbs[li].end_effector = Vec2::new(rest.x, gy);
        }

        self.clamp_to_bounds();
    }
}
