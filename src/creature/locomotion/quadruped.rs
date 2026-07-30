//! Quadruped drivers: profile gait (walk/trot/pace variants) with per-girdle
//! inverted pendulums, excited tail-wag waiting, grazing idle, and a
//! lie-down sleep with dream paddling. All fully kinematic.
//!
//! Skeleton layout (profile, facing -x): 0 head, 1 neck, 2 front_body,
//! 3 rear_body, 4..6 tail. Limbs: 0 front-left, 1 front-right (anchor 2),
//! 2 rear-left, 3 rear-right (anchor 3).

use super::*;

/// Per-creature quadruped personality, hashed from the rest pose like the
/// bipedal styles (salted so it draws independently).
#[derive(Clone, Copy)]
pub(super) struct QuadStyle {
    // Working gait.
    freq: f32,      // strides per second
    duty: f32,      // stance fraction
    stride_f: f32,  // stride as fraction of leg length
    lift: f32,      // swing foot lift
    bob: f32,       // pendulum bob exaggeration
    gait_kind: u8,  // 0 = 4-beat walk, 1 = trot (diagonal), 2 = pace (waddle)
    limp: f32,      // ~25%: rear-left leg drags
    wobble: f32,    // bounded phase jitter
    nod: f32,       // head nod amplitude
    tail_freq: f32, // working tail wag
    tail_amp: f32,
    // Waiting: excited pup.
    wag_freq: f32,  // fast happy wag
    wag_amp: f32,
    paw: f32,       // alternating front paw lift height
    bow: f32,       // ~35%: play-bow depth, else 0
    tilt: f32,      // head tilt amplitude
    // Idle.
    breath_freq: f32,
    breath_amp: f32,
    sway: f32,      // lazy tail sway
    graze: f32,     // ~40%: occasional head-dip-to-ground depth, else 0
    // Sleeping.
    curl: f32,      // how far the head tucks toward the body
    twitch: f32,    // ~30%: dream paddling of a front paw, else 0
}

impl QuadStyle {
    pub(super) fn from_skeleton(skeleton: &Skeleton) -> Self {
        let mut rng = style_rng(skeleton, 0x9e37_79b9_7f4a_7c15);
        let mut pick = |lo: f32, hi: f32| pick(&mut rng, lo, hi);
        let gait_roll = pick(0.0, 1.0);
        Self {
            freq: pick(0.8, 1.7),
            duty: pick(0.56, 0.68),
            stride_f: pick(0.30, 0.55),
            lift: pick(1.0, 2.2),
            bob: pick(0.6, 1.4),
            // Walkers amble, trotters are brisk, pacers waddle.
            gait_kind: if gait_roll < 0.45 { 0 } else if gait_roll < 0.8 { 1 } else { 2 },
            limp: if pick(0.0, 1.0) < 0.25 { pick(0.15, 0.35) } else { 0.0 },
            wobble: pick(0.0, 0.06),
            nod: pick(0.3, 1.0),
            tail_freq: pick(0.5, 1.3),
            tail_amp: pick(0.7, 1.8),
            wag_freq: pick(1.8, 3.2),
            wag_amp: pick(1.2, 2.6),
            paw: pick(0.7, 1.6),
            bow: if pick(0.0, 1.0) < 0.35 { pick(1.5, 2.8) } else { 0.0 },
            tilt: pick(0.5, 1.4),
            breath_freq: pick(0.16, 0.30),
            breath_amp: pick(0.2, 0.45),
            sway: pick(0.5, 1.2),
            graze: if pick(0.0, 1.0) < 0.4 { pick(2.5, 4.5) } else { 0.0 },
            curl: pick(0.6, 1.4),
            twitch: if pick(0.0, 1.0) < 0.3 { pick(0.6, 1.2) } else { 0.0 },
        }
    }
}

/// Leg phase offsets per gait kind, order [FL, FR, RL, RR]. Every girdle's
/// pair is 0.5 apart, so with duty > 0.5 each girdle always has a stance leg
/// for its pendulum.
fn gait_phases(kind: u8) -> [f32; 4] {
    match kind {
        1 => [0.0, 0.5, 0.5, 0.0],   // trot: diagonal pairs
        2 => [0.0, 0.5, 0.0, 0.5],   // pace: lateral pairs (comedy waddle)
        _ => [0.0, 0.5, 0.75, 0.25], // 4-beat lateral-sequence walk
    }
}

impl LocomotionState {
    /// Side-on depth read: the near legs (left, 0 and 2) draw in front of the
    /// body; the far legs (right, 1 and 3) are slimmed and occluded behind it.
    fn set_leg_depths(&mut self) {
        for li in [0usize, 2] {
            self.skeleton.limbs[li].depth = 1;
        }
        for li in [1usize, 3] {
            self.skeleton.limbs[li].depth = -1;
            self.skeleton.limbs[li].upper_width = self.rest_limb_widths[li].0 * 0.8;
            self.skeleton.limbs[li].lower_width = self.rest_limb_widths[li].1 * 0.8;
        }
    }

    /// Treadmill gait in profile. Like the bipedal walk, each girdle vaults
    /// over its stance leg (inverted pendulum), so the spine rocks
    /// front/rear as diagonal pairs land — no synthetic bob.
    pub(super) fn drive_working_quadruped(&mut self) {
        use std::f32::consts::{PI, TAU};
        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 7 {
            return;
        }
        let s = self.quad;
        let t = self.elapsed;
        let gy = self.ground_y();
        let dir = -1.0; // faces -x
        let jitter = s.wobble * (TAU * 0.31 * t).sin();
        let cyc = (t * s.freq + jitter).rem_euclid(1.0);
        let phases = gait_phases(s.gait_kind);

        let leg_len = self.skeleton.limbs[0].upper_len + self.skeleton.limbs[0].lower_len;
        let stride = s.stride_f * leg_len;
        let spine_rest_y = self.rest_positions[2].y;
        let flex = 0.5; // permanent slight crouch
        let l_eff = (gy - spine_rest_y) - flex;
        let hitch_of = |li: usize| if li == 2 { 1.0 - s.limp } else { 1.0 };

        // Drop below spine rest height for one girdle (its two legs).
        let girdle_drop = |legs: [usize; 2], c: f32| -> f32 {
            let mut support = 0.0f32;
            for li in legs {
                let p = (c + phases[li]).rem_euclid(1.0);
                if p < s.duty {
                    let st = stride * hitch_of(li);
                    let dx = st / 2.0 - (p / s.duty) * st;
                    support = support.max((l_eff * l_eff - dx * dx).max(0.0).sqrt());
                }
            }
            flex + (l_eff - support) * (1.0 + s.bob)
        };
        let front_drop = girdle_drop([0, 1], cyc);
        let rear_drop = girdle_drop([2, 3], cyc);

        // Spine: girdles carry their own drop, neck follows the front, the
        // head lags for follow-through and nods at stride frequency.
        let front_lag = girdle_drop([0, 1], (cyc - 0.08).rem_euclid(1.0));
        let nod = s.nod * (TAU * (2.0 * cyc - 0.15)).sin() * 0.5;
        let rp = self.rest_positions.clone();
        let r = |i: usize| rp[i];
        self.put_point(0, Vec2::new(r(0).x, r(0).y + front_lag + nod));
        self.put_point(1, Vec2::new(r(1).x, r(1).y + front_drop * 0.9));
        self.put_point(2, Vec2::new(r(2).x, r(2).y + front_drop));
        self.put_point(3, Vec2::new(r(3).x, r(3).y + rear_drop));

        // Tail: rides the rear girdle and wags on its own beat, tip most.
        for k in 1..=3usize {
            let rest = r(3 + k);
            let wag = s.tail_amp * (k as f32 / 3.0)
                * (TAU * s.tail_freq * t - k as f32 * 0.9).sin();
            self.put_point(3 + k, Vec2::new(rest.x, rest.y + rear_drop * 0.8 + wag));
        }

        // Slight load squash on the girdles.
        self.skeleton.points[2].width =
            self.base_widths[2] + 0.25 * (2.0 * TAU * cyc).cos();
        self.skeleton.points[3].width =
            self.base_widths[3] + 0.25 * (2.0 * TAU * cyc + PI).cos();

        // Legs: stance slides back linearly (treadmill), swing arcs forward.
        for (li, phase) in phases.iter().enumerate().take(4) {
            let hitch = hitch_of(li);
            let st = stride * hitch;
            let p = (cyc + phase).rem_euclid(1.0);
            let rest_x = self.rest_effectors[li].x;
            let (x_off, y) = if p < s.duty {
                (dir * (st / 2.0 - (p / s.duty) * st), gy)
            } else {
                let v = (p - s.duty) / (1.0 - s.duty);
                (dir * (v * st - st / 2.0), gy - s.lift * hitch * (PI * v).sin())
            };
            self.skeleton.limbs[li].end_effector = Vec2::new(rest_x + x_off, y);
        }

        self.set_leg_depths();

        self.clamp_to_bounds();
    }

    /// Excited pup: fast tail wag, alternating front-paw lifts, head tilts,
    /// and (for bowers) an occasional play bow — front end dips, rump stays
    /// up, tail keeps going.
    pub(super) fn drive_waiting_quadruped(&mut self) {
        use std::f32::consts::{PI, TAU};
        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 7 {
            return;
        }
        let s = self.quad;
        let t = self.elapsed;
        let gy = self.ground_y();

        // Play bow on a slow gate, ramped inside the window to avoid pops.
        let bow_gate = (TAU * 0.11 * t).sin();
        let bow = if s.bow > 0.0 && bow_gate > 0.85 {
            s.bow * ease((bow_gate - 0.85) / 0.15)
        } else {
            0.0
        };
        // Body jiggles with the wag beat (half rate) — pure downward dips.
        let beat = TAU * s.wag_freq * 0.5 * t;
        let jig = 0.35 * beat.sin().max(0.0);

        let rp = self.rest_positions.clone();
        let r = |i: usize| rp[i];
        // Front end dips into the bow; head dives further and tilts.
        let tilt = s.tilt * (TAU * 0.21 * t).sin();
        self.put_point(0, Vec2::new(r(0).x - 0.4 * bow + tilt * 0.4, r(0).y + jig + bow * 1.5));
        self.put_point(1, Vec2::new(r(1).x, r(1).y + jig + bow * 1.2));
        self.put_point(2, Vec2::new(r(2).x, r(2).y + jig + bow));
        self.put_point(3, Vec2::new(r(3).x, r(3).y + jig * 0.5));

        // Happy tail: fast, big, whole tail whips with a phase lag.
        for k in 1..=3usize {
            let rest = r(3 + k);
            let wag = s.wag_amp * (0.3 + 0.7 * k as f32 / 3.0)
                * (TAU * s.wag_freq * t - k as f32 * 1.1).sin();
            self.put_point(3 + k, Vec2::new(rest.x, rest.y + wag));
        }

        // Front paws alternate little lifts in time with the jig; during a
        // bow both front feet stay planted (stretched forward).
        for li in 0..4 {
            let rest = self.rest_effectors[li];
            let mut foot = Vec2::new(rest.x, gy);
            if li < 2 && bow <= 0.01 {
                let ph = beat + if li == 0 { 0.0 } else { PI };
                foot.y -= s.paw * ph.sin().max(0.0);
            }
            if li < 2 && bow > 0.01 {
                foot.x -= bow * 0.6; // paws slide forward into the bow
            }
            self.skeleton.limbs[li].end_effector = foot;
        }

        self.set_leg_depths();
        self.clamp_to_bounds();
    }

    /// Calm stand: breathing, lazy tail sway, a slow fore-aft weight rock,
    /// and (for grazers) an occasional nose-to-the-ground dip.
    pub(super) fn drive_idle_quadruped(&mut self) {
        use std::f32::consts::TAU;
        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 7 {
            return;
        }
        let s = self.quad;
        let t = self.elapsed;
        let gy = self.ground_y();

        // Fore-aft rock on a slow period; legs stay planted so they lean.
        let rock = 0.4 * (TAU * 0.06 * t).sin();
        // Graze: slow gate, head and neck dip toward the ground and back.
        let graze_gate = (TAU * 0.045 * t + 0.7).sin();
        let graze = if s.graze > 0.0 && graze_gate > 0.8 {
            s.graze * ease((graze_gate - 0.8) / 0.2)
        } else {
            0.0
        };
        let glance = 0.5 * (TAU * 0.13 * t).sin();

        let rp = self.rest_positions.clone();
        let r = |i: usize| rp[i];
        self.put_point(0, Vec2::new(r(0).x + rock - 0.5 * graze * 0.3 + glance, r(0).y + graze));
        self.put_point(1, Vec2::new(r(1).x + rock, r(1).y + graze * 0.45));
        self.put_point(2, Vec2::new(r(2).x + rock, r(2).y));
        self.put_point(3, Vec2::new(r(3).x + rock * 0.6, r(3).y));

        // Lazy tail sway on two incommensurate periods.
        for k in 1..=3usize {
            let rest = r(3 + k);
            let sway = s.sway * (k as f32 / 3.0)
                * ((TAU * 0.17 * t - k as f32 * 0.7).sin()
                    + 0.3 * (TAU * 0.29 * t).sin());
            self.put_point(3 + k, Vec2::new(rest.x, rest.y + sway));
        }

        // Breathing in the chest.
        let breath = (TAU * s.breath_freq * t).sin();
        self.skeleton.points[2].width = self.base_widths[2] + breath * s.breath_amp;

        for li in 0..4 {
            let rest = self.rest_effectors[li];
            self.skeleton.limbs[li].end_effector = Vec2::new(rest.x, gy);
        }

        self.set_leg_depths();
        self.clamp_to_bounds();
    }

    /// Lies down to sleep: belly sinks to the ground, legs fold under, the
    /// head settles low, tail curls around the body. Dream-paddlers twitch a
    /// front paw now and then.
    pub(super) fn drive_sleeping_quadruped(&mut self) {
        use std::f32::consts::TAU;
        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 7 {
            return;
        }
        let s = self.quad;
        let t = self.elapsed;
        let gy = self.ground_y();
        let k = ease(t / 2.0);

        let rp = self.rest_positions.clone();
        let r = |i: usize| rp[i];
        let rear_x = r(3).x;
        // Lying targets: spine just above the ground, head tucked back
        // toward the body, tail swept down and around.
        let targets = [
            // Head stays a lump above the body line so the silhouette still
            // reads "animal with its chin down", not a single mound.
            Vec2::new(r(0).x + 1.0 * s.curl, gy - 3.6), // head
            Vec2::new(r(1).x + 0.5 * s.curl, gy - 2.8), // neck
            Vec2::new(r(2).x, gy - 2.2),                // front body
            Vec2::new(r(3).x, gy - 2.2),                // rear body
            Vec2::new(rear_x + 1.4, gy - 1.4),          // tail_1
            Vec2::new(rear_x + 0.4, gy - 0.9),          // tail_2
            Vec2::new(rear_x - 1.0, gy - 0.7),          // tail_3 tucked in
        ];
        for (i, tgt) in targets.iter().enumerate() {
            let rest = r(i);
            self.put_point(
                i,
                Vec2::new(lerp(rest.x, tgt.x, k), lerp(rest.y, tgt.y, k)),
            );
        }

        // Slow breathing once settled.
        let breath = (TAU * s.breath_freq * 0.5 * t).sin();
        self.skeleton.points[2].width = self.base_widths[2] + breath * 0.2 * k;
        self.skeleton.points[3].width = self.base_widths[3] + breath * 0.12 * k;

        // Legs fold under the body. Dream paddle: a front paw pumps briefly
        // on a rare slow gate.
        let paddle_gate = (TAU * 0.05 * t + 1.7).sin();
        let paddle = if s.twitch > 0.0 && paddle_gate > 0.94 {
            s.twitch * ease((paddle_gate - 0.94) / 0.06)
        } else {
            0.0
        };
        for li in 0..4 {
            let rest = self.rest_effectors[li];
            let anchor_x = if li < 2 { r(2).x } else { r(3).x };
            let mut foot = Vec2::new(
                lerp(rest.x, lerp(rest.x, anchor_x, 0.5), k),
                lerp(rest.y.min(gy), gy - 0.5, k),
            );
            if li == 0 && paddle > 0.0 {
                foot.x -= paddle * 0.9 * (TAU * 3.5 * t).sin().abs();
                foot.y -= paddle * 0.5;
            }
            self.skeleton.limbs[li].end_effector = foot;
        }

        self.set_leg_depths();
        self.clamp_to_bounds();
    }
}
