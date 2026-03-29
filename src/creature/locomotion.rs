use std::time::Duration;

use crate::session::SessionState;
use super::physics::{apply_constraints, snap_to_grid, verlet_integrate};
use super::skeleton::{Skeleton, Vec2};

fn sigmoid(t: f32) -> f32 {
    1.0 / (1.0 + (-10.0 * (t - 0.5)).exp())
}

#[derive(Clone)]
struct FootStep {
    start_x: f32,
    target_x: f32,
    start_y: f32,
    arc_height: f32,
    progress: f32,
    stepping: bool,
}

impl FootStep {
    fn new() -> Self {
        Self { start_x: 0.0, target_x: 0.0, start_y: 0.0, arc_height: 1.5, progress: 0.0, stepping: false }
    }

    fn begin(&mut self, from_x: f32, to_x: f32, ground_y: f32) {
        self.start_x = from_x;
        self.target_x = to_x;
        self.start_y = ground_y;
        self.arc_height = 1.5;
        self.progress = 0.0;
        self.stepping = true;
    }

    fn advance(&mut self, dt: f32, speed: f32) -> (f32, f32) {
        if !self.stepping {
            return (self.target_x, self.start_y);
        }
        self.progress += dt * speed;
        if self.progress >= 1.0 {
            self.progress = 1.0;
            self.stepping = false;
        }
        let t = sigmoid(self.progress);
        let x = self.start_x + (self.target_x - self.start_x) * t;
        let arc = self.arc_height * 4.0 * t * (1.0 - t);
        let y = self.start_y - arc;
        (x, y)
    }
}

pub struct LocomotionState {
    skeleton: Skeleton,
    base_widths: Vec<f32>,
    rest_positions: Vec<Vec2>,  // initial positions for all chain points
    rest_effectors: Vec<Vec2>,  // initial end effector positions for all limbs
    state: SessionState,
    elapsed: f32,
    archetype: usize,
    foot_steps: Vec<FootStep>,
    settled: bool,
}

impl LocomotionState {
    pub fn new(skeleton: Skeleton, state: SessionState) -> Self {
        let base_widths: Vec<f32> = skeleton.points.iter().map(|p| p.width).collect();
        let rest_positions: Vec<Vec2> = skeleton.points.iter().map(|p| p.pos).collect();
        let rest_effectors: Vec<Vec2> = skeleton.limbs.iter().map(|l| l.end_effector).collect();
        let archetype = detect_archetype(&skeleton);
        let foot_steps = skeleton.limbs.iter().map(|_| FootStep::new()).collect();
        let mut ls = Self {
            skeleton, base_widths, rest_positions, rest_effectors,
            state, elapsed: 0.0, archetype, foot_steps, settled: false,
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

    fn drive_working_bipedal(&mut self, _dt: f32) {
        // Cyclical walk: legs alternate in a continuous sine-driven gait
        let cx = self.rest_center_x();
        let walk_freq = 0.8; // cycles per second
        let stride = 2.5;    // how far each foot moves
        let phase = self.elapsed * walk_freq * std::f32::consts::TAU;

        // Gentle head sway (small, follows walk rhythm)
        let head_sway = phase.sin() * 0.6;
        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x = cx + head_sway;
        }

        verlet_integrate(&mut self.skeleton, 0.85, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);

        // Continuous leg cycling — legs 2,3 (indices into limbs)
        let gy = self.ground_y();
        if self.skeleton.limbs.len() >= 4 {
            for leg_idx in 2..4 {
                let leg_phase = phase + if leg_idx == 2 { 0.0 } else { std::f32::consts::PI };
                let rest_x = self.rest_effectors[leg_idx].x;
                let x_offset = leg_phase.sin() * stride;
                self.skeleton.limbs[leg_idx].end_effector.x = rest_x + x_offset;
                // Lift foot during forward swing (when sin > 0)
                let lift = (leg_phase.sin().max(0.0)) * 2.0;
                self.skeleton.limbs[leg_idx].end_effector.y = gy - lift;
            }
        }

        // Arm swing in opposition to legs
        if self.skeleton.limbs.len() >= 4 {
            for arm_idx in 0..2 {
                let arm_phase = phase + if arm_idx == 0 { std::f32::consts::PI } else { 0.0 };
                let rest = self.rest_effectors[arm_idx];
                self.skeleton.limbs[arm_idx].end_effector.x = rest.x + arm_phase.sin() * 1.5;
                self.skeleton.limbs[arm_idx].end_effector.y = rest.y;
            }
        }

        self.restore_vertical_center();
        snap_to_grid(&mut self.skeleton, 0.7);
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
        snap_to_grid(&mut self.skeleton, 0.7);
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
        snap_to_grid(&mut self.skeleton, 0.7);
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
        snap_to_grid(&mut self.skeleton, 0.7);
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
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    // --- Waiting state: gentle idle look-around ---

    fn drive_waiting(&mut self, _dt: f32) {
        match self.archetype {
            2 => self.drive_waiting_blob(),
            4 => self.drive_waiting_serpentine(),
            _ => self.drive_waiting_default(),
        }
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
        snap_to_grid(&mut self.skeleton, 0.7);
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
        snap_to_grid(&mut self.skeleton, 0.7);
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
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    // --- Idle state: subtle breathing ---

    fn drive_idle(&mut self, _dt: f32) {
        match self.archetype {
            4 => self.drive_idle_serpentine(),
            _ => self.drive_idle_default(),
        }
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
        snap_to_grid(&mut self.skeleton, 0.7);
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
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    // --- Sleeping state: very slow breathing ---

    fn drive_sleeping(&mut self, _dt: f32) {
        match self.archetype {
            4 => self.drive_sleeping_serpentine(),
            _ => self.drive_sleeping_default(),
        }
    }

    fn drive_sleeping_default(&mut self) {
        let breath = (self.elapsed * 0.3 * std::f32::consts::TAU).sin();
        for (i, pt) in self.skeleton.points.iter_mut().enumerate() {
            pt.width = self.base_widths[i] + breath * 0.1;
        }

        verlet_integrate(&mut self.skeleton, 0.75, Vec2::zero());
        apply_constraints(&mut self.skeleton, 2);
        self.restore_vertical_center();
        snap_to_grid(&mut self.skeleton, 0.7);
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
        snap_to_grid(&mut self.skeleton, 0.7);
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

    // --- Foot stepping: only for leg limbs, with fixed ground ---

    fn update_leg_stepping(&mut self, dt: f32, step_speed: f32, ground_y: f32) {
        if self.skeleton.limbs.is_empty() { return; }

        let com_x: f32 = self.skeleton.points.iter()
            .map(|p| p.pos.x).sum::<f32>() / self.skeleton.points.len() as f32;

        // For bipedal: limbs 0,1 are arms, 2,3 are legs
        // For winged: limbs 0,1 are legs (only 2 limbs)
        // We step all limbs that are legs (anchored to lower body / hips)
        for (i, limb) in self.skeleton.limbs.iter_mut().enumerate() {
            if i >= self.foot_steps.len() { continue; }

            // Skip arms (bipedal limbs 0,1 anchored to upper_body idx 2)
            // Legs are anchored to hip points (idx 4,5) or lower body
            if self.archetype == 0 && i < 2 { continue; }

            let step = &mut self.foot_steps[i];
            if !step.stepping {
                let drift = (com_x - limb.end_effector.x).abs();
                if drift > 2.0 {
                    let target_x = com_x + (com_x - limb.end_effector.x).signum() * 1.0;
                    step.begin(limb.end_effector.x, target_x, ground_y);
                }
            }

            if step.stepping {
                let (fx, fy) = step.advance(dt, step_speed);
                limb.end_effector.x = fx;
                limb.end_effector.y = fy;
            } else {
                limb.end_effector.y = ground_y;
            }
        }
    }

    /// Quadruped diagonal gait with fixed ground level.
    fn update_diagonal_gait(&mut self, dt: f32, ground_y: f32) {
        if self.skeleton.limbs.len() < 4 { return; }

        let com_x: f32 = self.skeleton.points.iter()
            .map(|p| p.pos.x).sum::<f32>() / self.skeleton.points.len() as f32;

        for li in 0..self.skeleton.limbs.len() {
            if li >= self.foot_steps.len() { continue; }

            let drift = (com_x - self.skeleton.limbs[li].end_effector.x).abs();
            if !self.foot_steps[li].stepping && drift > 1.5 {
                let target_x = com_x + (com_x - self.skeleton.limbs[li].end_effector.x).signum() * 0.8;
                self.foot_steps[li].begin(self.skeleton.limbs[li].end_effector.x, target_x, ground_y);
            }

            let step = &mut self.foot_steps[li];
            if step.stepping {
                let (fx, fy) = step.advance(dt, 3.0);
                self.skeleton.limbs[li].end_effector.x = fx;
                self.skeleton.limbs[li].end_effector.y = fy;
            } else {
                self.skeleton.limbs[li].end_effector.y = ground_y;
            }
        }
    }

    /// Smooth sine-based arm swing for bipedal (limbs 0,1 = arms).
    fn update_arm_swing_smooth(&mut self) {
        if self.skeleton.limbs.len() < 4 || self.archetype != 0 { return; }

        let swing = (self.elapsed * 0.8 * std::f32::consts::TAU).sin() * 1.0;

        for i in 0..2 {
            let rest = self.rest_effectors[i];
            let sign = if i == 0 { 1.0 } else { -1.0 };
            self.skeleton.limbs[i].end_effector.x = rest.x + swing * sign;
            self.skeleton.limbs[i].end_effector.y = rest.y;
        }
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
