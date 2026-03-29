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
        Self { start_x: 0.0, target_x: 0.0, start_y: 0.0, arc_height: 2.0, progress: 0.0, stepping: false }
    }

    fn begin(&mut self, from_x: f32, to_x: f32, ground_y: f32) {
        self.start_x = from_x;
        self.target_x = to_x;
        self.start_y = ground_y;
        self.arc_height = 2.0;
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
    rest_y_positions: Vec<f32>, // initial Y positions to anchor against
    state: SessionState,
    elapsed: f32,
    archetype: usize,
    foot_steps: Vec<FootStep>,
    settled: bool,
}

impl LocomotionState {
    pub fn new(skeleton: Skeleton, state: SessionState) -> Self {
        let base_widths: Vec<f32> = skeleton.points.iter().map(|p| p.width).collect();
        let rest_y_positions: Vec<f32> = skeleton.points.iter().map(|p| p.pos.y).collect();
        let archetype = detect_archetype(&skeleton);
        let foot_steps = skeleton.limbs.iter().map(|_| FootStep::new()).collect();
        let mut ls = Self {
            skeleton, base_widths, rest_y_positions, state, elapsed: 0.0, archetype, foot_steps, settled: false,
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
        for (pt, &bw) in self.skeleton.points.iter_mut().zip(self.base_widths.iter()) {
            pt.width = bw;
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

    fn drive_working_bipedal(&mut self, dt: f32) {
        let offset = (self.elapsed * 2.0 * std::f32::consts::TAU).sin() * 3.0;
        if let Some(head) = self.skeleton.points.first_mut() { head.pos.x = 9.0 + offset; }
        verlet_integrate(&mut self.skeleton, 0.98, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.update_foot_stepping_with_arc(dt, 5.0);
        self.update_arm_swing();
        self.restore_vertical_center();
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_working_quadruped(&mut self, dt: f32) {
        let offset = (self.elapsed * 1.5 * std::f32::consts::TAU).sin() * 2.0;
        if let Some(head) = self.skeleton.points.first_mut() { head.pos.x += offset * dt * 3.0; }
        verlet_integrate(&mut self.skeleton, 0.98, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.update_diagonal_gait(dt);
        self.restore_vertical_center();
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_working_blob(&mut self, _dt: f32) {
        let phase = self.elapsed * 1.5 * std::f32::consts::TAU;
        let n = self.skeleton.points.len();
        for i in 0..n {
            let point_phase = phase + (i as f32 / n as f32) * std::f32::consts::TAU;
            let squeeze = point_phase.sin() * 1.5;
            let bw = self.base_widths[i];
            self.skeleton.points[i].width = (bw + squeeze * 0.3).max(0.5);
            self.skeleton.points[i].pos.x += point_phase.cos() * 0.05;
        }
        verlet_integrate(&mut self.skeleton, 0.98, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.restore_vertical_center();
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_working_winged(&mut self, dt: f32) {
        let offset = (self.elapsed * 1.8 * std::f32::consts::TAU).sin() * 2.5;
        if let Some(head) = self.skeleton.points.first_mut() { head.pos.x = 9.0 + offset; }
        let flap_phase = self.elapsed * 4.0 * std::f32::consts::TAU;
        for pt in &mut self.skeleton.points {
            if pt.name.starts_with("lwing") || pt.name.starts_with("rwing") {
                pt.pos.y += flap_phase.sin() * 2.0 * 0.1;
            }
        }
        verlet_integrate(&mut self.skeleton, 0.98, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.update_foot_stepping_with_arc(dt, 5.0);
        self.restore_vertical_center();
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_working_serpentine(&mut self, _dt: f32) {
        let n = self.skeleton.points.len();
        for i in 0..n {
            let phase = self.elapsed * 2.0 * std::f32::consts::TAU
                - (i as f32 / n as f32) * std::f32::consts::TAU * 1.5;
            self.skeleton.points[i].pos.y = 12.0 + phase.sin() * 2.0;
        }
        let drift = (self.elapsed * 0.5 * std::f32::consts::TAU).sin() * 2.0;
        if let Some(head) = self.skeleton.points.first_mut() { head.pos.x += drift * 0.02; }
        verlet_integrate(&mut self.skeleton, 0.98, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.restore_vertical_center();
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_waiting(&mut self, _dt: f32) {
        let offset = (self.elapsed * std::f32::consts::TAU).sin() * 1.5;
        if let Some(head) = self.skeleton.points.first_mut() { head.pos.x = 9.0 + offset; }
        verlet_integrate(&mut self.skeleton, 0.95, Vec2::zero());
        apply_constraints(&mut self.skeleton, 3);
        self.restore_vertical_center();
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_idle(&mut self, _dt: f32) {
        let breath = (self.elapsed * 0.8 * std::f32::consts::TAU).sin() * 0.3;
        for (pt, &bw) in self.skeleton.points.iter_mut().zip(self.base_widths.iter()) {
            pt.width = bw + breath * 0.3;
        }
        let head_nudge = (self.elapsed * 0.3 * std::f32::consts::TAU).sin() * 0.5;
        if let Some(head) = self.skeleton.points.first_mut() { head.pos.x = 9.0 + head_nudge; }
        verlet_integrate(&mut self.skeleton, 0.90, Vec2::zero());
        apply_constraints(&mut self.skeleton, 2);
        self.restore_vertical_center();
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_sleeping(&mut self, _dt: f32) {
        let breath = (self.elapsed * 0.4 * std::f32::consts::TAU).sin() * 0.2;
        for (pt, &bw) in self.skeleton.points.iter_mut().zip(self.base_widths.iter()) {
            pt.width = bw + breath * 0.15;
        }
        verlet_integrate(&mut self.skeleton, 0.85, Vec2::zero());
        apply_constraints(&mut self.skeleton, 2);
        self.restore_vertical_center();
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn settle_disconnected(&mut self) {
        for _ in 0..10 {
            verlet_integrate(&mut self.skeleton, 0.5, Vec2::new(0.0, 0.8));
            apply_constraints(&mut self.skeleton, 3);
            self.clamp_to_bounds();
        }
        for pt in &mut self.skeleton.points { pt.prev_pos = pt.pos; }
        self.settled = true;
    }

    fn update_foot_stepping_with_arc(&mut self, dt: f32, step_speed: f32) {
        if self.skeleton.limbs.is_empty() { return; }
        let com_x: f32 = self.skeleton.points.iter().map(|p| p.pos.x).sum::<f32>() / self.skeleton.points.len() as f32;
        for (i, limb) in self.skeleton.limbs.iter_mut().enumerate() {
            let anchor = self.skeleton.points[limb.anchor].pos;
            let ground_y = (anchor.y + limb.upper_len + limb.lower_len).min(23.0);
            if i >= self.foot_steps.len() { continue; }
            let step = &mut self.foot_steps[i];
            if !step.stepping {
                let drift = (com_x - limb.end_effector.x).abs();
                if drift > 2.5 {
                    let target_x = com_x + (com_x - limb.end_effector.x).signum() * 1.5;
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

    fn update_diagonal_gait(&mut self, dt: f32) {
        if self.skeleton.limbs.len() < 4 { return; }
        let com_x: f32 = self.skeleton.points.iter().map(|p| p.pos.x).sum::<f32>() / self.skeleton.points.len() as f32;
        let pairs = [(0, 3), (1, 2)];
        for &(a, b) in &pairs {
            for &li in &[a, b] {
                let limb = &self.skeleton.limbs[li];
                let anchor = self.skeleton.points[limb.anchor].pos;
                let ground_y = (anchor.y + limb.upper_len + limb.lower_len).min(23.0);
                let drift = (com_x - limb.end_effector.x).abs();
                if li < self.foot_steps.len() && !self.foot_steps[li].stepping && drift > 2.0 {
                    let target_x = com_x + (com_x - limb.end_effector.x).signum() * 1.0;
                    self.foot_steps[li].begin(limb.end_effector.x, target_x, ground_y);
                }
                if li < self.foot_steps.len() {
                    let step = &mut self.foot_steps[li];
                    if step.stepping {
                        let (fx, fy) = step.advance(dt, 4.0);
                        self.skeleton.limbs[li].end_effector.x = fx;
                        self.skeleton.limbs[li].end_effector.y = fy;
                    } else {
                        let anchor = self.skeleton.points[self.skeleton.limbs[li].anchor].pos;
                        let gy = (anchor.y + self.skeleton.limbs[li].upper_len + self.skeleton.limbs[li].lower_len).min(23.0);
                        self.skeleton.limbs[li].end_effector.y = gy;
                    }
                }
            }
        }
    }

    fn update_arm_swing(&mut self) {
        if self.skeleton.limbs.len() < 4 { return; }
        let leg_l_x = self.skeleton.limbs[2].end_effector.x;
        let leg_r_x = self.skeleton.limbs[3].end_effector.x;
        let anchor_l = self.skeleton.points[self.skeleton.limbs[0].anchor].pos;
        let anchor_r = self.skeleton.points[self.skeleton.limbs[1].anchor].pos;
        let arm_swing = 1.5;
        self.skeleton.limbs[0].end_effector.x = anchor_l.x - (leg_r_x - anchor_r.x).signum() * arm_swing;
        self.skeleton.limbs[0].end_effector.y = anchor_l.y + self.skeleton.limbs[0].upper_len + self.skeleton.limbs[0].lower_len * 0.5;
        self.skeleton.limbs[1].end_effector.x = anchor_r.x - (leg_l_x - anchor_l.x).signum() * arm_swing;
        self.skeleton.limbs[1].end_effector.y = anchor_r.y + self.skeleton.limbs[1].upper_len + self.skeleton.limbs[1].lower_len * 0.5;
    }

    /// Anchor the skeleton vertically: shift all points so the spine's average Y
    /// matches the rest position. This prevents drift from Verlet integration while
    /// still allowing relative vertical movement (breathing, bouncing).
    fn restore_vertical_center(&mut self) {
        let current_y: f32 = self.skeleton.points.iter()
            .map(|p| p.pos.y).sum::<f32>() / self.skeleton.points.len() as f32;
        let rest_y: f32 = self.rest_y_positions.iter().sum::<f32>()
            / self.rest_y_positions.len() as f32;
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
