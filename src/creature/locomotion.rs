use std::time::Duration;

use crate::session::SessionState;
use super::generate::Xorshift;
use super::physics::{apply_constraints, verlet_integrate};
use super::skeleton::{Skeleton, Vec2};

/// Per-creature walk personality, derived deterministically from the skeleton
/// so the same creature always moves the same way. Ranges are tuned so every
/// combination still reads as a walk at 18x24.
#[derive(Clone, Copy)]
struct GaitStyle {
    freq: f32,      // cycles per second
    duty: f32,      // stance fraction of the cycle
    stride_f: f32,  // stride as a fraction of total leg length
    lift: f32,      // swing foot peak lift
    bob: f32,       // pelvis bob amplitude
    lean: f32,      // forward lean per px above the hips
    arm_swing: f32, // hand x amplitude
    head_lag: f32,  // head follow-through, cycle fraction
    sway: f32,      // head micro-sway amplitude
    limp: f32,      // 0 = even gait; >0 = left leg drags (lower lift, shorter step)
    wobble: f32,    // cycle-rate irregularity amplitude
}

impl GaitStyle {
    /// Sample a personality from the creature's own geometry: hash the rest
    /// pose into a PRNG seed, then draw each parameter from its range.
    fn from_skeleton(skeleton: &Skeleton) -> Self {
        // FNV-1a over the rest pose bits — stable across runs for a given seed.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for pt in &skeleton.points {
            for v in [pt.pos.x, pt.pos.y, pt.width] {
                h = (h ^ v.to_bits() as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        let mut rng = Xorshift::new(h | 1); // xorshift dies on 0
        let mut pick = |lo: f32, hi: f32| {
            lo + (hi - lo) * ((rng.next_u64() % 1000) as f32 / 1000.0)
        };
        Self {
            freq: pick(1.1, 1.7),
            duty: pick(0.54, 0.66),
            stride_f: pick(0.45, 0.62),
            lift: pick(1.8, 3.2),
            bob: pick(0.6, 1.4),
            lean: pick(0.04, 0.20),
            arm_swing: pick(1.6, 3.0),
            head_lag: pick(0.05, 0.16),
            sway: pick(0.2, 0.7),
            // Most creatures walk evenly; ~30% get a hitch in their step.
            limp: if pick(0.0, 1.0) < 0.3 { pick(0.15, 0.4) } else { 0.0 },
            wobble: pick(0.0, 0.08),
        }
    }
}

pub struct LocomotionState {
    skeleton: Skeleton,
    base_widths: Vec<f32>,
    rest_positions: Vec<Vec2>,
    rest_effectors: Vec<Vec2>,
    rest_bend_dirs: Vec<f32>,
    rest_limb_widths: Vec<(f32, f32)>,
    gait: GaitStyle,
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
        let gait = GaitStyle::from_skeleton(&skeleton);
        let archetype = detect_archetype(&skeleton);
        let mut ls = Self {
            skeleton, base_widths, rest_positions, rest_effectors,
            rest_bend_dirs, rest_limb_widths, gait,
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
        }
        if state == SessionState::Disconnected {
            self.settle_disconnected();
        }
    }

    pub fn tick(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f32();
        self.elapsed += dt_secs;
        match self.state {
            SessionState::Working => self.drive_working(dt_secs),
            SessionState::Waiting => self.drive_waiting(dt_secs),
            SessionState::Idle => self.drive_idle(dt_secs),
            SessionState::Sleeping => self.drive_sleeping(dt_secs),
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

    // --- Working state: archetype-specific locomotion ---

    fn drive_working(&mut self, dt: f32) {
        match self.archetype {
            0 => self.drive_working_bipedal(dt),
            1 => self.drive_working_quadruped(dt),
            2 => self.drive_working_blob(dt),
            3 => self.drive_working_winged(dt),
            4 => self.drive_working_serpentine(dt),
            _ => self.drive_working_bipedal(dt),
        }
    }

    /// Side-profile treadmill walk (Richard Williams contact/down/passing/up
    /// poses, made continuous). Fully kinematic: unlike the other archetypes'
    /// drivers this writes every point directly each tick (prev_pos = pos, no
    /// verlet/constraints) so the pose is a pure function of `elapsed` and the
    /// stance foot stays planted instead of lagging behind its target.
    fn drive_working_bipedal(&mut self, _dt: f32) {
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

        // Body height: always crouched below rest (keeps knees bent, IK in
        // range), rising by up to `bob` at the passing poses (cyc .25/.75).
        let crouch = g.bob + 0.6;
        let bob = g.bob * 0.5 * (1.0 - (2.0 * TAU * cyc).cos());
        let body_drop = crouch - bob;

        // Spine re-pose to profile: forward lean (facing +x), ground-anchored bob.
        for i in 0..4 {
            let rest = self.rest_positions[i];
            let lean = g.lean * (hip_rest_y - rest.y);
            let drop = if i == 0 {
                // Head follow-through: its bob lags the pelvis slightly.
                let cyc_h = (cyc - g.head_lag).rem_euclid(1.0);
                crouch - g.bob * 0.5 * (1.0 - (2.0 * TAU * cyc_h).cos())
            } else {
                body_drop
            };
            let pt = &mut self.skeleton.points[i];
            pt.pos = Vec2::new(cx + lean, rest.y + drop);
            pt.prev_pos = pt.pos;
        }
        // Micro-sway on the head keeps the silhouette alive.
        self.skeleton.points[0].pos.x += (TAU * cyc).sin() * g.sway;
        self.skeleton.points[0].prev_pos = self.skeleton.points[0].pos;

        // Hips nearly overlap in profile.
        for (i, dx) in [(4usize, -0.5f32), (5, 0.5)] {
            let pt = &mut self.skeleton.points[i];
            pt.pos = Vec2::new(cx + dx, self.rest_positions[i].y + body_drop);
            pt.prev_pos = pt.pos;
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
        let leg_len = self.skeleton.limbs[2].upper_len + self.skeleton.limbs[2].lower_len;
        for leg_idx in 2..4 {
            let hitch = if leg_idx == 2 { 1.0 - g.limp } else { 1.0 };
            let stride = g.stride_f * leg_len * hitch;
            let lift = g.lift * hitch;
            let p = if leg_idx == 2 { cyc } else { (cyc + 0.5).fract() };
            let (x, y) = if p < g.duty {
                let u = p / g.duty;
                (cx + stride / 2.0 - u * stride, gy)
            } else {
                let v = (p - g.duty) / (1.0 - g.duty);
                (cx - stride / 2.0 + v * stride, gy - lift * (PI * v).sin())
            };
            let limb = &mut self.skeleton.limbs[leg_idx];
            limb.end_effector = Vec2::new(x, y);
            limb.bend_dir = 1.0; // knees forward (facing +x)
        }

        // Arms counter-swing the same-side leg; elbows bend backward.
        let shoulder_y = self.rest_positions[2].y + body_drop;
        for arm_idx in 0..2 {
            // armL (0) is in phase with legR, i.e. opposite legL.
            let p = if arm_idx == 0 { (cyc + 0.5).fract() } else { cyc };
            let swing = (TAU * p).sin();
            let hand_drop = self.rest_effectors[arm_idx].y - self.rest_positions[2].y;
            let limb = &mut self.skeleton.limbs[arm_idx];
            limb.end_effector = Vec2::new(
                cx + g.arm_swing * swing,
                // Hand rides the shoulder and rises a touch on the forward swing.
                shoulder_y + hand_drop - 0.4 * swing.max(0.0),
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

    fn drive_working_quadruped(&mut self, _dt: f32) {
        // Continuous diagonal gait: front-left+rear-right step together, then swap
        let walk_freq = 0.7;
        let stride = 2.0;
        let phase = self.elapsed * walk_freq * std::f32::consts::TAU;

        // Gentle head bob
        let cx = self.rest_center_x();
        let head_bob = (phase * 2.0).sin() * 0.3;
        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x = cx + head_bob;
        }

        verlet_integrate(&mut self.skeleton, 0.85, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);

        // Diagonal gait: legs 0,3 are in phase, legs 1,2 are offset by PI
        // 0=front-left, 1=front-right, 2=rear-left, 3=rear-right
        let gy = self.ground_y();
        let phase_offsets = [0.0, std::f32::consts::PI, std::f32::consts::PI, 0.0];
        for (li, &offset) in phase_offsets.iter().enumerate() {
            if li >= self.skeleton.limbs.len() { break; }
            let leg_phase = phase + offset;
            let rest_x = self.rest_effectors[li].x;
            let x_offset = leg_phase.sin() * stride;
            self.skeleton.limbs[li].end_effector.x = rest_x + x_offset;
            let lift = leg_phase.sin().max(0.0) * 1.5;
            self.skeleton.limbs[li].end_effector.y = gy - lift;
        }

        self.restore_vertical_center();
        self.clamp_to_bounds();
    }

    fn drive_working_blob(&mut self, _dt: f32) {
        // Pronounced undulating ring — traveling wave of width + position
        let phase = self.elapsed * 1.2 * std::f32::consts::TAU;
        let n = self.skeleton.points.len();
        for i in 0..n {
            let point_phase = phase + (i as f32 / n as f32) * std::f32::consts::TAU;
            // Width oscillation — more pronounced
            let squeeze = point_phase.sin() * 1.2;
            self.skeleton.points[i].width = (self.base_widths[i] + squeeze).max(0.5);
            // Position undulation — push points radially in/out
            let rest = self.rest_positions[i];
            let cx = 9.0;
            let cy = 12.0;
            let dx = rest.x - cx;
            let dy = rest.y - cy;
            let len = (dx * dx + dy * dy).sqrt().max(0.1);
            let radial_offset = point_phase.cos() * 1.0;
            self.skeleton.points[i].pos.x = rest.x + (dx / len) * radial_offset;
            self.skeleton.points[i].pos.y = rest.y + (dy / len) * radial_offset;
        }
        verlet_integrate(&mut self.skeleton, 0.85, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.restore_vertical_center();
        self.clamp_to_bounds();
    }

    fn drive_working_winged(&mut self, _dt: f32) {
        // Subtle head sway — much less than before
        let cx = self.rest_center_x();
        let offset = (self.elapsed * 0.5 * std::f32::consts::TAU).sin() * 0.4;
        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x = cx + offset;
        }

        // Wing flap: more pronounced, with cascading delay along wing segments
        let flap_phase = self.elapsed * 1.5 * std::f32::consts::TAU;
        let flap_amplitude = 3.0;
        for (i, pt) in self.skeleton.points.iter_mut().enumerate() {
            let name = pt.name;
            if name.starts_with("lwing") || name.starts_with("rwing") {
                // Extract segment number for cascading delay
                let seg_num = name.chars().last().and_then(|c| c.to_digit(10)).unwrap_or(1) as f32;
                let delay = seg_num * 0.3;
                let rest_y = self.rest_positions[i].y;
                let amplitude = flap_amplitude * (0.5 + seg_num * 0.3); // outer segments move more
                pt.pos.y = rest_y + (flap_phase - delay).sin() * amplitude;
            }
        }

        verlet_integrate(&mut self.skeleton, 0.85, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);

        // Simple leg positioning (no stepping, just hold position)
        let gy = self.ground_y();
        for li in 0..self.skeleton.limbs.len() {
            self.skeleton.limbs[li].end_effector.y = gy;
        }

        self.restore_vertical_center();
        self.clamp_to_bounds();
    }

    fn drive_working_serpentine(&mut self, _dt: f32) {
        // Sine wave propagation — all segments follow smoothly, no tail hooks
        let n = self.skeleton.points.len();
        let wave_speed = 1.2;
        let wave_amplitude = 1.5;
        // Amplitude tapers toward tail to prevent wild tail flailing
        for i in 0..n {
            let phase = self.elapsed * wave_speed * std::f32::consts::TAU
                - (i as f32 / n as f32) * std::f32::consts::TAU * 1.5;
            let taper = 1.0 - (i as f32 / n as f32) * 0.3; // tail has ~70% amplitude
            let rest_y = self.rest_positions[i].y;
            self.skeleton.points[i].pos.y = rest_y + phase.sin() * wave_amplitude * taper;
        }

        verlet_integrate(&mut self.skeleton, 0.85, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.restore_vertical_center();
        self.clamp_to_bounds();
    }

    // --- Waiting state: gentle idle look-around ---

    fn drive_waiting(&mut self, _dt: f32) {
        match self.archetype {
            0 => self.drive_waiting_bipedal(),
            2 => self.drive_waiting_blob(),
            4 => self.drive_waiting_serpentine(),
            _ => self.drive_waiting_default(),
        }
    }

    /// Expectant: bouncing on toes with arms half-raised, occasional hop.
    /// Kinematic (see drive_working_bipedal).
    fn drive_waiting_bipedal(&mut self) {
        use std::f32::consts::TAU;
        let t = self.elapsed;
        let cx = self.rest_center_x();
        let gy = self.ground_y();

        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 6 {
            return;
        }

        // Toe bounce: up-pause-up rhythm (half-rectified sine).
        let bounce = (TAU * 1.2 * t).sin().max(0.0) * 1.2;
        // Occasional full hop, gated by an incommensurate slow wave.
        let hop_gate = (TAU * 0.17 * t).sin();
        let hop = if hop_gate > 0.92 { 1.5 } else { 0.0 };
        let rise = bounce + hop;

        for i in 0..6 {
            let rest = self.rest_positions[i];
            let pt = &mut self.skeleton.points[i];
            pt.pos = Vec2::new(rest.x, rest.y - rise);
            pt.prev_pos = pt.pos;
        }
        // Head tilt: slow look-around on its own period.
        self.skeleton.points[0].pos.x = cx + (TAU * 0.23 * t).sin();
        self.skeleton.points[0].prev_pos = self.skeleton.points[0].pos;

        // Anticipation squash at the bottom of each bounce.
        let squash = (1.2 - bounce).max(0.0) * 0.2;
        self.skeleton.points[2].width = self.base_widths[2] + squash;

        // Feet stay on the ground line; on the hop the IK clamp carries them up.
        for leg_idx in 2..4 {
            let rest_x = self.rest_effectors[leg_idx].x;
            self.skeleton.limbs[leg_idx].end_effector = Vec2::new(rest_x, gy - hop);
        }

        // Arms half-raised, small eager sway in time with the bounce.
        let arm_sway = (TAU * 1.2 * t).sin() * 0.5;
        for arm_idx in 0..2 {
            let rest = self.rest_effectors[arm_idx];
            let out = if arm_idx == 0 { -arm_sway } else { arm_sway };
            self.skeleton.limbs[arm_idx].end_effector =
                Vec2::new(rest.x + out, rest.y - 2.0 - rise);
        }

        self.clamp_to_bounds();
    }

    fn drive_waiting_default(&mut self) {
        let cx = self.rest_center_x();
        let offset = (self.elapsed * 0.4 * std::f32::consts::TAU).sin() * 0.8;
        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x = cx + offset;
        }

        verlet_integrate(&mut self.skeleton, 0.85, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.restore_vertical_center();
        self.clamp_to_bounds();
    }

    fn drive_waiting_blob(&mut self) {
        // Gentle bouncing up and down
        let bounce_phase = self.elapsed * 0.6 * std::f32::consts::TAU;
        let bounce_offset = bounce_phase.sin() * 1.5;
        let n = self.skeleton.points.len();
        for i in 0..n {
            let rest = self.rest_positions[i];
            self.skeleton.points[i].pos.y = rest.y + bounce_offset;
            // Slight width pulse synchronized with bounce
            let squeeze = bounce_phase.cos() * 0.3;
            self.skeleton.points[i].width = (self.base_widths[i] + squeeze).max(0.5);
        }

        verlet_integrate(&mut self.skeleton, 0.85, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.restore_vertical_center();
        self.clamp_to_bounds();
    }

    fn drive_waiting_serpentine(&mut self) {
        // Gentle S-curve — static sine shape that slowly shifts
        let n = self.skeleton.points.len();
        let phase = self.elapsed * 0.3 * std::f32::consts::TAU;
        for i in 0..n {
            let wave = (phase + (i as f32 / n as f32) * std::f32::consts::TAU).sin();
            let rest_y = self.rest_positions[i].y;
            self.skeleton.points[i].pos.y = rest_y + wave * 0.8;
        }

        verlet_integrate(&mut self.skeleton, 0.85, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.restore_vertical_center();
        self.clamp_to_bounds();
    }

    // --- Idle state: subtle breathing ---

    fn drive_idle(&mut self, _dt: f32) {
        match self.archetype {
            0 => self.drive_idle_bipedal(),
            4 => self.drive_idle_serpentine(),
            _ => self.drive_idle_default(),
        }
    }

    /// Relaxed contrapposto: slow weight shift between legs, breathing, and an
    /// occasional glance. Kinematic (see drive_working_bipedal).
    fn drive_idle_bipedal(&mut self) {
        use std::f32::consts::TAU;
        let t = self.elapsed;
        let cx = self.rest_center_x();

        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 6 {
            return;
        }

        // Slight slouch keeps the knees soft so the weight shift reads in the
        // legs instead of the IK clamping straight.
        const SLOUCH: f32 = 0.5;
        // Weight shift: hips lead, shoulders follow at 60%, head counters.
        let shift = 1.5 * (TAU * 0.08 * t).sin();
        // Occasional glance: cubed sine dwells near zero, then darts.
        let s = (TAU * 0.043 * t).sin();
        let glance = 1.2 * s * s * s;

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
            let pt = &mut self.skeleton.points[i];
            pt.pos = Vec2::new(rest.x + dx, rest.y + SLOUCH);
            pt.prev_pos = pt.pos;
        }
        // Fix head x around center (rest.x == cx for spine, but be explicit).
        self.skeleton.points[0].pos.x = cx + offsets[0];
        self.skeleton.points[0].prev_pos = self.skeleton.points[0].pos;

        // Breathing.
        let breath = (TAU * 0.25 * t).sin();
        self.skeleton.points[2].width = self.base_widths[2] + breath * 0.25;

        // Feet planted at rest; arms hang, drifting with the shoulders.
        for leg_idx in 2..4 {
            self.skeleton.limbs[leg_idx].end_effector = self.rest_effectors[leg_idx];
        }
        for arm_idx in 0..2 {
            let rest = self.rest_effectors[arm_idx];
            self.skeleton.limbs[arm_idx].end_effector =
                Vec2::new(rest.x + 0.6 * shift, rest.y + SLOUCH);
        }

        self.clamp_to_bounds();
    }

    fn drive_idle_default(&mut self) {
        let breath = (self.elapsed * 0.5 * std::f32::consts::TAU).sin();
        for (i, pt) in self.skeleton.points.iter_mut().enumerate() {
            pt.width = self.base_widths[i] + breath * 0.2;
        }

        let cx = self.rest_center_x();
        let head_nudge = (self.elapsed * 0.2 * std::f32::consts::TAU).sin() * 0.3;
        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x = cx + head_nudge;
        }

        verlet_integrate(&mut self.skeleton, 0.80, Vec2::zero());
        apply_constraints(&mut self.skeleton, 2);
        self.restore_vertical_center();
        self.clamp_to_bounds();
    }

    fn drive_idle_serpentine(&mut self) {
        // Gentle coiled resting pose — slight S-curve with breathing
        let n = self.skeleton.points.len();
        let breath = (self.elapsed * 0.4 * std::f32::consts::TAU).sin();
        for i in 0..n {
            let t = i as f32 / n as f32;
            let wave = (t * std::f32::consts::TAU * 0.8).sin();
            let rest_y = self.rest_positions[i].y;
            self.skeleton.points[i].pos.y = rest_y + wave * 0.5;
            self.skeleton.points[i].width = self.base_widths[i] + breath * 0.15;
        }

        verlet_integrate(&mut self.skeleton, 0.80, Vec2::zero());
        apply_constraints(&mut self.skeleton, 2);
        self.restore_vertical_center();
        self.clamp_to_bounds();
    }

    // --- Sleeping state: very slow breathing ---

    fn drive_sleeping(&mut self, _dt: f32) {
        match self.archetype {
            0 => self.drive_sleeping_bipedal(),
            4 => self.drive_sleeping_serpentine(),
            _ => self.drive_sleeping_default(),
        }
    }

    /// Slumped standing doze: eases into a droop, then breathes on two
    /// incommensurate periods so the loop never reads as a loop.
    /// Kinematic (see drive_working_bipedal).
    fn drive_sleeping_bipedal(&mut self) {
        use std::f32::consts::TAU;
        let t = self.elapsed;

        if self.skeleton.limbs.len() < 4 || self.skeleton.points.len() < 6 {
            return;
        }

        // Smoothstep ease into the slump over the first 1.5s.
        let k = {
            let u = (t / 1.5).min(1.0);
            u * u * (3.0 - 2.0 * u)
        };
        let sag = [2.5, 1.8, 1.0, 0.3, 0.3, 0.3]; // head droops most
        let wobble = 0.3 * (TAU * 0.09 * t).sin();

        for (i, s) in sag.iter().enumerate() {
            let rest = self.rest_positions[i];
            let pt = &mut self.skeleton.points[i];
            pt.pos = Vec2::new(rest.x, rest.y + k * s + wobble * k);
            pt.prev_pos = pt.pos;
        }
        // Head lolls to one side.
        self.skeleton.points[0].pos.x -= 2.0 * k;
        self.skeleton.points[0].prev_pos = self.skeleton.points[0].pos;

        // Slow breathing.
        let breath = (TAU * 0.12 * t).sin();
        self.skeleton.points[2].width = self.base_widths[2] + breath * 0.15;

        // Arms drop limp to the sides; feet stay planted.
        let cx = self.rest_center_x();
        for arm_idx in 0..2 {
            let rest = self.rest_effectors[arm_idx];
            let side_x = if arm_idx == 0 { cx - 3.0 } else { cx + 3.0 };
            self.skeleton.limbs[arm_idx].end_effector = Vec2::new(
                rest.x + (side_x - rest.x) * k,
                rest.y + 1.5 * k,
            );
        }
        for leg_idx in 2..4 {
            self.skeleton.limbs[leg_idx].end_effector = self.rest_effectors[leg_idx];
        }

        self.clamp_to_bounds();
    }

    fn drive_sleeping_default(&mut self) {
        let breath = (self.elapsed * 0.3 * std::f32::consts::TAU).sin();
        for (i, pt) in self.skeleton.points.iter_mut().enumerate() {
            pt.width = self.base_widths[i] + breath * 0.1;
        }

        verlet_integrate(&mut self.skeleton, 0.75, Vec2::zero());
        apply_constraints(&mut self.skeleton, 2);
        self.restore_vertical_center();
        self.clamp_to_bounds();
    }

    fn drive_sleeping_serpentine(&mut self) {
        // Coiled resting — tighter S-curve, very slow breathing
        let n = self.skeleton.points.len();
        let breath = (self.elapsed * 0.2 * std::f32::consts::TAU).sin();
        for i in 0..n {
            let t = i as f32 / n as f32;
            // Tighter coil than idle
            let wave = (t * std::f32::consts::TAU * 1.2).sin();
            let rest_y = self.rest_positions[i].y;
            self.skeleton.points[i].pos.y = rest_y + wave * 0.7;
            self.skeleton.points[i].width = self.base_widths[i] + breath * 0.08;
        }

        verlet_integrate(&mut self.skeleton, 0.75, Vec2::zero());
        apply_constraints(&mut self.skeleton, 2);
        self.restore_vertical_center();
        self.clamp_to_bounds();
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


    /// Anchor the skeleton vertically to rest position.
    fn restore_vertical_center(&mut self) {
        let current_y: f32 = self.skeleton.points.iter()
            .map(|p| p.pos.y).sum::<f32>() / self.skeleton.points.len() as f32;
        let rest_y: f32 = self.rest_positions.iter()
            .map(|p| p.y).sum::<f32>() / self.rest_positions.len() as f32;
        let drift = current_y - rest_y;
        if drift.abs() > 0.01 {
            for pt in &mut self.skeleton.points {
                pt.pos.y -= drift;
                pt.prev_pos.y -= drift;
            }
        }
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
