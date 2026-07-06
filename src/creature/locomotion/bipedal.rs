//! Bipedal drivers: profile treadmill walk, expectant bounce, contrapposto
//! idle, slumped standing doze. All fully kinematic.

use super::*;

/// Per-creature walk personality, derived deterministically from the skeleton
/// so the same creature always moves the same way. Ranges are tuned so every
/// combination still reads as a walk at 18x24.
#[derive(Clone, Copy)]
pub(super) struct GaitStyle {
    freq: f32,      // cycles per second
    duty: f32,      // stance fraction of the cycle
    stride_f: f32,  // stride as a fraction of total leg length
    lift: f32,      // swing foot peak lift
    bob: f32,       // exaggeration of the inverted-pendulum bob
    lean: f32,      // forward lean per px above the hips
    arm_swing: f32, // hand x amplitude
    head_lag: f32,  // head follow-through, cycle fraction
    sway: f32,      // head micro-sway amplitude
    limp: f32,      // 0 = even gait; >0 = left leg drags (lower lift, shorter step)
    wobble: f32,    // cycle-rate irregularity amplitude
    knee_bend: f32, // permanent knee flex: near-straight strider -> soft-kneed skulk
    elbow_give: f32, // how much the elbow bends on the forward arm swing
    flail: f32,     // 0 = composed; >0 = loose, overswinging windmill arms
}

impl GaitStyle {
    /// Sample a personality from the creature's own geometry: hash the rest
    /// pose into a PRNG seed, then draw each parameter from its range.
    pub(super) fn from_skeleton(skeleton: &Skeleton) -> Self {
        let mut rng = style_rng(skeleton, 0);
        let mut pick = |lo: f32, hi: f32| pick(&mut rng, lo, hi);
        Self {
            freq: pick(0.7, 2.3),
            duty: pick(0.54, 0.66),
            stride_f: pick(0.32, 0.75),
            lift: pick(1.8, 3.2),
            bob: pick(0.6, 1.4),
            lean: pick(0.04, 0.20),
            arm_swing: pick(1.8, 4.4),
            head_lag: pick(0.05, 0.16),
            sway: pick(0.2, 0.7),
            // Most creatures walk evenly; ~30% get a hitch in their step.
            limp: if pick(0.0, 1.0) < 0.3 { pick(0.15, 0.4) } else { 0.0 },
            wobble: pick(0.0, 0.08),
            knee_bend: pick(0.1, 1.3),
            elbow_give: pick(0.0, 3.0),
            // ~20% of creatures flail their arms around while they work.
            flail: if pick(0.0, 1.0) < 0.2 { pick(0.4, 1.0) } else { 0.0 },
        }
    }
}

/// Per-creature personality for the non-walking bipedal states, sampled the
/// same way as GaitStyle but from an independently-salted hash so the two
/// vary independently.
#[derive(Clone, Copy)]
pub(super) struct IdleStyle {
    // Waiting: expectant bounce.
    bounce_freq: f32, // bounces per second
    bounce_amp: f32,  // toe-bounce height
    hop_freq: f32,    // how often the full hop comes around
    hop_amp: f32,     // hop height
    arm_raise: f32,   // how high the arms are held (cheerer vs cool customer)
    tilt: f32,        // head-tilt amplitude
    // Idle: relaxed contrapposto.
    shift_freq: f32,  // weight-shift period
    shift_amp: f32,   // weight-shift distance
    glance: f32,      // glance dart amplitude
    breath_freq: f32,
    breath_amp: f32,
    slouch: f32,      // standing knee softness
    tap: f32,         // 0 = still feet; >0 = occasional toe tap height
    // Sleeping: slump.
    slump: f32,             // slump depth multiplier
    droop_dir: f32,         // which way the head lolls (+/-1)
    sleep_breath_freq: f32,
    twitch: f32,            // 0 = sound sleeper; >0 = occasional head twitch
}

impl IdleStyle {
    pub(super) fn from_skeleton(skeleton: &Skeleton) -> Self {
        // Flipped bits going into the hash (== salt of all-ones), so idle
        // personality doesn't correlate with gait personality.
        let mut rng = style_rng(skeleton, u64::MAX);
        let mut pick = |lo: f32, hi: f32| pick(&mut rng, lo, hi);
        Self {
            bounce_freq: pick(0.8, 1.7),
            bounce_amp: pick(0.7, 1.8),
            hop_freq: pick(0.09, 0.23),
            hop_amp: pick(1.0, 2.2),
            arm_raise: pick(1.0, 3.2),
            tilt: pick(0.5, 1.5),
            shift_freq: pick(0.05, 0.12),
            shift_amp: pick(1.0, 2.2),
            glance: pick(0.8, 1.8),
            breath_freq: pick(0.18, 0.33),
            breath_amp: pick(0.15, 0.35),
            slouch: pick(0.3, 0.9),
            // ~40% of creatures tap a toe while they wait around.
            tap: if pick(0.0, 1.0) < 0.4 { pick(0.6, 1.2) } else { 0.0 },
            slump: pick(0.7, 1.3),
            droop_dir: if pick(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 },
            sleep_breath_freq: pick(0.08, 0.16),
            // ~25% twitch in their sleep.
            twitch: if pick(0.0, 1.0) < 0.25 { pick(0.5, 1.0) } else { 0.0 },
        }
    }
}

impl LocomotionState {
    /// Side-profile treadmill walk (Richard Williams contact/down/passing/up
    /// poses, made continuous). Fully kinematic: every point is written
    /// directly each tick (prev_pos = pos, no verlet/constraints) so the pose
    /// is a pure function of `elapsed` and the stance foot stays planted
    /// instead of lagging behind its target.
    pub(super) fn drive_working_bipedal(&mut self) {
        use std::f32::consts::{PI, TAU};
        const SQUASH: f32 = 0.35; // torso width modulation

        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 6 {
            return;
        }

        let g = self.gait;
        let cx = self.rest_center_x();
        let gy = self.ground_y();
        // Timing wobble: a bounded phase jitter on a slow, incommensurate
        // period speeds strides up and down slightly, so no two are identical.
        let jitter = g.wobble * (TAU * 0.31 * self.elapsed).sin();
        let cyc = (self.elapsed * g.freq + jitter).rem_euclid(1.0);
        let hip_rest_y = self.rest_positions[3].y;

        // Inverted-pendulum body height: the hips vault over the stance leg,
        // so the knee straightens (up to a small personality flex) at the
        // passing pose and only bends at contact and in swing — a constant
        // crouch reads as a robot-dog skulk. During double support the higher
        // of the two pendulums carries the body.
        let leg_len = self.skeleton.limbs[2].upper_len + self.skeleton.limbs[2].lower_len;
        let reach = gy - hip_rest_y; // rest hip-to-ground (legs straight)
        let flex = 0.2 + 0.5 * g.knee_bend; // permanent slight knee flex
        let l_eff = reach - flex;
        // Foot x offset from the hips for a leg at phase p in its own cycle.
        let stance_dx = |p: f32, stride: f32| -> f32 { stride / 2.0 - (p / g.duty) * stride };
        // Drop below rest height as a function of cycle position (also used
        // at a lagged position for the head's follow-through).
        let body_drop_at = |c: f32| -> f32 {
            let mut support = 0.0f32;
            for (offset, hitch) in [(0.0, 1.0 - g.limp), (0.5, 1.0)] {
                let p = (c + offset).rem_euclid(1.0);
                if p < g.duty {
                    let dx = stance_dx(p, g.stride_f * leg_len * hitch);
                    support = support.max((l_eff * l_eff - dx * dx).max(0.0).sqrt());
                }
            }
            // Exaggerate the (physically small) pendulum bob so it reads at
            // 18x24; `bob` is the exaggeration personality.
            flex + (l_eff - support) * (1.0 + g.bob)
        };
        let body_drop = body_drop_at(cyc);

        // Spine re-pose to profile: forward lean (facing +x), ground-anchored bob.
        for i in 0..4 {
            let rest = self.rest_positions[i];
            let lean = g.lean * (hip_rest_y - rest.y);
            let drop = if i == 0 {
                // Head follow-through: its bob lags the pelvis slightly.
                body_drop_at((cyc - g.head_lag).rem_euclid(1.0))
            } else {
                body_drop
            };
            self.put_point(i, Vec2::new(cx + lean, rest.y + drop));
        }
        // Micro-sway on the head keeps the silhouette alive.
        let head = self.skeleton.points[0].pos + Vec2::new((TAU * cyc).sin() * g.sway, 0.0);
        self.put_point(0, head);

        // Hips nearly overlap in profile.
        for (i, dx) in [(4usize, -0.5f32), (5, 0.5)] {
            self.put_point(i, Vec2::new(cx + dx, self.rest_positions[i].y + body_drop));
        }

        // Profile is narrower than the front view — shrink the torso so limbs
        // read against it — plus squash & stretch: widest at contact (body
        // lowest), stretched at passing.
        const PROFILE_W: f32 = 0.65;
        let squash = SQUASH * (2.0 * TAU * cyc).cos();
        self.skeleton.points[1].width = self.base_widths[1] * PROFILE_W;
        self.skeleton.points[2].width = self.base_widths[2] * PROFILE_W + squash;
        self.skeleton.points[3].width = self.base_widths[3] * PROFILE_W + squash * 0.6;

        // Legs (limbs 2,3): stance foot planted, sliding back linearly
        // (treadmill); swing foot arcs forward with a sine lift. A limp
        // shortens and flattens the left leg's step.
        for leg_idx in 2..4 {
            let hitch = if leg_idx == 2 { 1.0 - g.limp } else { 1.0 };
            let stride = g.stride_f * leg_len * hitch;
            let lift = g.lift * hitch;
            let p = if leg_idx == 2 { cyc } else { (cyc + 0.5).fract() };
            let (x, y) = if p < g.duty {
                (cx + stance_dx(p, stride), gy)
            } else {
                let v = (p - g.duty) / (1.0 - g.duty);
                (cx - stride / 2.0 + v * stride, gy - lift * (PI * v).sin())
            };
            let limb = &mut self.skeleton.limbs[leg_idx];
            limb.end_effector = Vec2::new(x, y);
            limb.bend_dir = 1.0; // knees forward (facing +x)
        }

        // Arms: pendulums from the shoulder, counter-swinging the same-side
        // leg. The hand traces an arc so the arm hangs near-straight through
        // the swing (like the pendulum legs — constant elbow bend reads
        // robotic); the elbow gives only as the arm comes forward. Flailers
        // overswing with a loose second harmonic on top.
        let shoulder = self.skeleton.points[2].pos;
        let arm_len = self.skeleton.limbs[0].upper_len + self.skeleton.limbs[0].lower_len;
        let theta_max = (g.arm_swing / arm_len) * (1.0 + 0.6 * g.flail);
        for arm_idx in 0..2 {
            // armL (0) is in phase with legR, i.e. opposite legL.
            let p = if arm_idx == 0 { (cyc + 0.5).fract() } else { cyc };
            let swing = (TAU * p).sin();
            let theta = theta_max * swing + 0.35 * g.flail * (2.0 * TAU * p + 1.3).sin();
            let give = (0.2 + 0.35 * g.elbow_give) * (1.0 + 2.0 * g.flail);
            let hand_r = arm_len - 0.15 - give * swing.max(0.0);
            let limb = &mut self.skeleton.limbs[arm_idx];
            limb.end_effector = Vec2::new(
                shoulder.x + hand_r * theta.sin(),
                shoulder.y + hand_r * theta.cos(),
            );
            limb.bend_dir = -1.0;
        }

        // Far-side limbs (right: 1, 3) drawn slimmer for a depth read.
        for li in [1usize, 3] {
            self.skeleton.limbs[li].upper_width = self.rest_limb_widths[li].0 * 0.85;
            self.skeleton.limbs[li].lower_width = self.rest_limb_widths[li].1 * 0.85;
        }

        self.clamp_to_bounds();
    }

    /// Expectant: bouncing on toes with arms half-raised, occasional hop.
    /// Amplitudes and rhythms come from IdleStyle so each creature waits its
    /// own way.
    pub(super) fn drive_waiting_bipedal(&mut self) {
        use std::f32::consts::TAU;
        let s = self.idle;
        let t = self.elapsed;
        let cx = self.rest_center_x();
        let gy = self.ground_y();

        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 6 {
            return;
        }

        // Eager knee-dip bounce. The head already touches the canvas top at
        // rest, so the body can't rise — it dips down and rebounds instead
        // (knees flex via IK), which reads as bouncing on the spot.
        let dip = (TAU * s.bounce_freq * t).sin().max(0.0) * s.bounce_amp;
        // Occasional hop, gated by an incommensurate slow wave: a cartoon
        // knee-tuck — the feet leave the ground, the body stays put.
        let hop_gate = (TAU * s.hop_freq * t).sin();
        let hop = if hop_gate > 0.92 { s.hop_amp } else { 0.0 };

        for i in 0..6 {
            let rest = self.rest_positions[i];
            self.put_point(i, Vec2::new(rest.x, rest.y + dip));
        }
        // Head tilt: slow look-around on its own period.
        let head_x = cx + (TAU * 0.23 * t).sin() * s.tilt;
        let head_y = self.skeleton.points[0].pos.y;
        self.put_point(0, Vec2::new(head_x, head_y));

        // Anticipation squash at the bottom of each dip.
        let squash = dip * 0.25;
        self.skeleton.points[2].width = self.base_widths[2] + squash;

        // Feet stay on the ground line except during the hop tuck.
        for leg_idx in 2..4 {
            let rest_x = self.rest_effectors[leg_idx].x;
            self.skeleton.limbs[leg_idx].end_effector = Vec2::new(rest_x, gy - hop);
        }

        // Arms held up, small eager sway in time with the bounce. High
        // arm_raise reads as a cheerer, low as a cool customer.
        let arm_sway = (TAU * s.bounce_freq * t).sin() * 0.5;
        for arm_idx in 0..2 {
            let rest = self.rest_effectors[arm_idx];
            let out = if arm_idx == 0 { -arm_sway } else { arm_sway };
            self.skeleton.limbs[arm_idx].end_effector =
                Vec2::new(rest.x + out, rest.y - s.arm_raise + dip);
        }

        self.clamp_to_bounds();
    }

    /// Relaxed contrapposto: slow weight shift between legs, breathing, an
    /// occasional glance, and (for some creatures) an idle toe tap.
    pub(super) fn drive_idle_bipedal(&mut self) {
        use std::f32::consts::TAU;
        let s = self.idle;
        let t = self.elapsed;
        let cx = self.rest_center_x();

        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 6 {
            return;
        }

        // Slouch keeps the knees soft so the weight shift reads in the legs
        // instead of the IK clamping straight.
        let slouch = s.slouch;
        // Weight shift: hips lead, shoulders follow at 60%, head counters.
        let shift = s.shift_amp * (TAU * s.shift_freq * t).sin();
        // Occasional glance: cubed sine dwells near zero, then darts.
        let gl = (TAU * 0.043 * t).sin();
        let glance = s.glance * gl * gl * gl;

        let offsets = [
            -0.3 * shift + glance, // head (contrapposto counter + glance)
            0.2 * shift,           // neck
            0.6 * shift,           // upper_body
            shift,                 // lower_body
            shift,                 // hip_l
            shift,                 // hip_r
        ];
        for (i, dx) in offsets.iter().enumerate() {
            let rest = self.rest_positions[i];
            self.put_point(i, Vec2::new(rest.x + dx, rest.y + slouch));
        }
        // Fix head x around center (rest.x == cx for spine, but be explicit).
        let head_y = self.skeleton.points[0].pos.y;
        self.put_point(0, Vec2::new(cx + offsets[0], head_y));

        // Breathing.
        let breath = (TAU * s.breath_freq * t).sin();
        self.skeleton.points[2].width = self.base_widths[2] + breath * s.breath_amp;

        // Feet planted at rest — except tappers, whose unweighted foot (the
        // one the hips shifted away from) taps on its own beat.
        let tap_gate = (TAU * 0.19 * t).sin();
        for leg_idx in 2..4 {
            let mut foot = self.rest_effectors[leg_idx];
            let unweighted = (leg_idx == 2) == (shift > 0.0);
            if s.tap > 0.0 && unweighted && tap_gate > 0.55 {
                foot.y -= s.tap * ((tap_gate - 0.55) / 0.45);
            }
            self.skeleton.limbs[leg_idx].end_effector = foot;
        }
        for arm_idx in 0..2 {
            let rest = self.rest_effectors[arm_idx];
            self.skeleton.limbs[arm_idx].end_effector =
                Vec2::new(rest.x + 0.6 * shift, rest.y + slouch);
        }

        self.clamp_to_bounds();
    }

    /// Slumped standing doze: eases into a droop, then breathes on two
    /// incommensurate periods so the loop never reads as a loop. Slump depth,
    /// droop side, breath rate, and sleep twitches vary per creature.
    pub(super) fn drive_sleeping_bipedal(&mut self) {
        use std::f32::consts::TAU;
        let s = self.idle;
        let t = self.elapsed;

        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 6 {
            return;
        }

        // Smoothstep ease into the slump over the first 1.5s.
        let k = ease(t / 1.5);
        let sag = [2.5, 1.8, 1.0, 0.3, 0.3, 0.3]; // head droops most
        let wobble = 0.3 * (TAU * 0.09 * t).sin();

        for (i, sg) in sag.iter().enumerate() {
            let rest = self.rest_positions[i];
            self.put_point(i, Vec2::new(rest.x, rest.y + k * sg * s.slump + wobble * k));
        }
        // Head lolls to one side; twitchy sleepers jerk it briefly now and
        // then before settling back.
        let twitch_gate = (TAU * 0.07 * t + 2.1).sin();
        let twitch = if s.twitch > 0.0 && twitch_gate > 0.96 {
            s.twitch * ((twitch_gate - 0.96) / 0.04)
        } else {
            0.0
        };
        let head = self.skeleton.points[0].pos
            + Vec2::new((-2.0 * k + twitch) * s.droop_dir, 0.0);
        self.put_point(0, head);

        // Slow breathing.
        let breath = (TAU * s.sleep_breath_freq * t).sin();
        self.skeleton.points[2].width = self.base_widths[2] + breath * 0.15;

        // Arms drop limp to the sides; feet stay planted.
        let cx = self.rest_center_x();
        for arm_idx in 0..2 {
            let rest = self.rest_effectors[arm_idx];
            let side_x = if arm_idx == 0 { cx - 3.0 } else { cx + 3.0 };
            self.skeleton.limbs[arm_idx].end_effector = Vec2::new(
                rest.x + (side_x - rest.x) * k,
                rest.y + 1.5 * k * s.slump,
            );
        }
        for leg_idx in 2..4 {
            self.skeleton.limbs[leg_idx].end_effector = self.rest_effectors[leg_idx];
        }

        self.clamp_to_bounds();
    }
}
