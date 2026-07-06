use std::ops::{Add, Sub, Mul, AddAssign, SubAssign};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn normalized(&self) -> Self {
        let len = self.length();
        if len < 1e-10 {
            Self::zero()
        } else {
            Self { x: self.x / len, y: self.y / len }
        }
    }

    pub fn perpendicular(&self) -> Self {
        Self { x: -self.y, y: self.x }
    }

    pub fn dot(&self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self { x: self.x - rhs.x, y: self.y - rhs.y }
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self { x: self.x * rhs, y: self.y * rhs }
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

use crate::creature::generate::Xorshift;

pub const ARCHETYPE_COUNT: usize = 5;

/// Archetypes enabled for new session assignment.
pub const ENABLED_ARCHETYPES: &[usize] = &[0, 1, 2, 3, 4]; // all of them

pub fn archetype_name(index: usize) -> &'static str {
    match index % ARCHETYPE_COUNT {
        0 => "bipedal",
        1 => "quadruped",
        2 => "blob",
        3 => "winged",
        4 => "serpentine",
        _ => unreachable!(),
    }
}

pub fn archetype_index(name: &str) -> usize {
    match name {
        "bipedal" => 0,
        "quadruped" => 1,
        "blob" => 2,
        "winged" => 3,
        "serpentine" => 4,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub struct ChainPoint {
    pub name: &'static str,
    pub pos: Vec2,
    pub prev_pos: Vec2,
    pub width: f32,
    pub pinned: bool,
}

#[derive(Debug, Clone)]
pub struct Constraint {
    pub a: usize,
    pub b: usize,
    pub rest_length: f32,
}

#[derive(Debug, Clone)]
pub struct Limb {
    pub anchor: usize,
    pub upper_len: f32,
    pub lower_len: f32,
    pub upper_width: f32,
    pub lower_width: f32,
    pub end_effector: Vec2,
    pub side: Side,
    /// Which side the IK joint (knee/elbow) bends toward: with y-down and the
    /// target below the anchor, +1.0 bends toward +x, -1.0 toward -x.
    pub bend_dir: f32,
}

#[derive(Debug, Clone)]
pub struct Skeleton {
    pub points: Vec<ChainPoint>,
    pub constraints: Vec<Constraint>,
    pub limbs: Vec<Limb>,
}

impl Skeleton {
    pub fn instantiate(archetype: usize, seed: u64) -> Self {
        let mut rng = Xorshift::new(seed);
        match archetype % ARCHETYPE_COUNT {
            0 => Self::build_bipedal(&mut rng),
            1 => Self::build_quadruped(&mut rng),
            2 => Self::build_blob(&mut rng),
            3 => Self::build_winged(&mut rng),
            4 => Self::build_serpentine(&mut rng),
            _ => unreachable!(),
        }
    }

    fn vary(rng: &mut Xorshift, base: f32, range: f32) -> f32 {
        let t = (rng.next_u64() % 1000) as f32 / 1000.0;
        base + (t * 2.0 - 1.0) * range
    }

    fn build_bipedal(rng: &mut Xorshift) -> Self {
        let cx = 9.0;
        let head_neck = Self::vary(rng, 1.2, 0.3);    // short neck
        let neck_upper = Self::vary(rng, 1.6, 0.4);   // shoulders sit high, just under the head
        let upper_lower = Self::vary(rng, 5.7, 1.0);  // torso; hips land ~y10.5 leaving room for legs
        let head_w = Self::vary(rng, 4.0, 1.6);
        let neck_w = Self::vary(rng, 2.5, 1.0);
        let upper_w = Self::vary(rng, 7.0, 2.0);      // wider shoulders
        let lower_w = Self::vary(rng, 5.0, 2.0);
        let head_y = 2.0;
        let neck_y = head_y + head_neck;
        let upper_y = neck_y + neck_upper;
        let lower_y = upper_y + upper_lower;

        let points = vec![
            ChainPoint { name: "head",       pos: Vec2::new(cx, head_y),  prev_pos: Vec2::new(cx, head_y),  width: head_w.max(2.0),  pinned: false },
            ChainPoint { name: "neck",       pos: Vec2::new(cx, neck_y),  prev_pos: Vec2::new(cx, neck_y),  width: neck_w.max(1.5),  pinned: false },
            ChainPoint { name: "upper_body", pos: Vec2::new(cx, upper_y), prev_pos: Vec2::new(cx, upper_y), width: upper_w.max(3.0), pinned: false },
            ChainPoint { name: "lower_body", pos: Vec2::new(cx, lower_y), prev_pos: Vec2::new(cx, lower_y), width: lower_w.max(2.5), pinned: false },
            ChainPoint { name: "hip_l",      pos: Vec2::new(cx - 2.0, lower_y), prev_pos: Vec2::new(cx - 2.0, lower_y), width: 1.0, pinned: false },
            ChainPoint { name: "hip_r",      pos: Vec2::new(cx + 2.0, lower_y), prev_pos: Vec2::new(cx + 2.0, lower_y), width: 1.0, pinned: false },
        ];
        let constraints = vec![
            Constraint { a: 0, b: 1, rest_length: head_neck },
            Constraint { a: 1, b: 2, rest_length: neck_upper },
            Constraint { a: 2, b: 3, rest_length: upper_lower },
            Constraint { a: 3, b: 4, rest_length: 2.0 },
            Constraint { a: 3, b: 5, rest_length: 2.0 },
        ];
        // Legs roughly half of total height (human-ish); long arms hang from
        // the high shoulders down to hip level for expressive swings.
        let arm_upper = Self::vary(rng, 4.2, 0.6).max(3.2);
        let arm_lower = Self::vary(rng, 3.8, 0.5).max(2.8);
        let leg_upper = Self::vary(rng, 5.5, 0.8).max(4.5);
        let leg_lower = Self::vary(rng, 5.0, 0.8).max(4.0);
        let arm_width_upper = Self::vary(rng, 1.2, 0.3).max(0.8);
        let arm_width_lower = Self::vary(rng, 1.0, 0.2).max(0.6);
        let leg_width_upper = Self::vary(rng, 1.4, 0.3).max(0.9);
        let leg_width_lower = Self::vary(rng, 1.1, 0.2).max(0.6);
        // Keep feet at 22.5 max so the 1px SDF border stays on-canvas.
        let foot_y = (lower_y + leg_upper + leg_lower).min(22.5);
        let hand_y = upper_y + arm_upper + arm_lower * 0.6;
        let limbs = vec![
            Limb { anchor: 2, upper_len: arm_upper, lower_len: arm_lower, upper_width: arm_width_upper, lower_width: arm_width_lower, end_effector: Vec2::new(cx - 4.0, hand_y), side: Side::Left, bend_dir: -1.0 },
            Limb { anchor: 2, upper_len: arm_upper, lower_len: arm_lower, upper_width: arm_width_upper, lower_width: arm_width_lower, end_effector: Vec2::new(cx + 4.0, hand_y), side: Side::Right, bend_dir: 1.0 },
            Limb { anchor: 4, upper_len: leg_upper, lower_len: leg_lower, upper_width: leg_width_upper, lower_width: leg_width_lower, end_effector: Vec2::new(cx - 2.5, foot_y), side: Side::Left, bend_dir: -1.0 },
            Limb { anchor: 5, upper_len: leg_upper, lower_len: leg_lower, upper_width: leg_width_upper, lower_width: leg_width_lower, end_effector: Vec2::new(cx + 2.5, foot_y), side: Side::Right, bend_dir: 1.0 },
        ];
        Self { points, constraints, limbs }
    }

    /// Profile view facing -x: raised head on a diagonal neck, level spine,
    /// perky up-curved tail, two-bone legs under each girdle.
    fn build_quadruped(rng: &mut Xorshift) -> Self {
        let cy = Self::vary(rng, 12.0, 0.8);
        let head_lift = Self::vary(rng, 4.2, 1.0);   // head height above the spine
        let neck_lift = Self::vary(rng, 2.0, 0.5);
        let front_rear = Self::vary(rng, 5.5, 1.2);  // spine length
        let head_w = Self::vary(rng, 3.4, 1.2);
        let neck_w = Self::vary(rng, 2.6, 0.9);
        let front_w = Self::vary(rng, 5.0, 1.6);
        let rear_w = Self::vary(rng, 4.6, 1.6);
        let head_x = 3.0;
        let neck_x = 4.6;
        let front_x = 6.4;
        let rear_x = (front_x + front_rear).min(13.5);
        let tail_lift = Self::vary(rng, 1.2, 0.5);

        let p = |x: f32, y: f32| Vec2::new(x.clamp(0.0, 17.0), y.clamp(0.0, 23.0));
        let positions = [
            ("head",       p(head_x, cy - head_lift), head_w.max(2.0)),
            ("neck",       p(neck_x, cy - neck_lift), neck_w.max(1.8)),
            ("front_body", p(front_x, cy),            front_w.max(3.0)),
            ("rear_body",  p(rear_x, cy),             rear_w.max(2.5)),
            ("tail_1",     p(rear_x + 1.6, cy - tail_lift),       1.6),
            ("tail_2",     p(rear_x + 2.9, cy - tail_lift * 2.2), 1.1),
            ("tail_3",     p(rear_x + 3.8, cy - tail_lift * 3.4), 0.6),
        ];
        let points: Vec<ChainPoint> = positions.iter()
            .map(|(name, pos, w)| ChainPoint { name, pos: *pos, prev_pos: *pos, width: *w, pinned: false })
            .collect();
        // Rest lengths from the actual (diagonal) point spacing so the
        // disconnected settle doesn't yank the pose.
        let constraints: Vec<Constraint> = (0..points.len() - 1)
            .map(|i| Constraint {
                a: i, b: i + 1,
                rest_length: (points[i].pos - points[i + 1].pos).length(),
            })
            .collect();

        let leg_upper = Self::vary(rng, 4.0, 0.6).max(3.2);
        let leg_lower = Self::vary(rng, 3.8, 0.6).max(3.0);
        let leg_width_upper = Self::vary(rng, 1.3, 0.3).max(0.8);
        let leg_width_lower = Self::vary(rng, 1.0, 0.2).max(0.6);
        let foot_y = (cy + leg_upper + leg_lower).min(22.5);
        // Facing -x: front knees bend backward (+x), rear stifles forward (-x).
        let leg = |anchor: usize, x: f32, side: Side, bend_dir: f32| Limb {
            anchor, upper_len: leg_upper, lower_len: leg_lower,
            upper_width: leg_width_upper, lower_width: leg_width_lower,
            end_effector: Vec2::new(x, foot_y), side, bend_dir,
        };
        let limbs = vec![
            leg(2, front_x - 0.8, Side::Left, 1.0),
            leg(2, front_x + 0.8, Side::Right, 1.0),
            leg(3, rear_x - 0.8, Side::Left, -1.0),
            leg(3, rear_x + 0.8, Side::Right, -1.0),
        ];
        Self { points, constraints, limbs }
    }

    fn build_blob(rng: &mut Xorshift) -> Self {
        let cx = 9.0;
        let cy = 12.0;
        let n = 8;
        let base_radius = Self::vary(rng, 6.0, 2.0).max(3.0);
        let mut points = Vec::with_capacity(n);
        let mut constraints = Vec::with_capacity(n);
        for i in 0..n {
            let angle = (i as f32 / n as f32) * std::f32::consts::TAU;
            let r = base_radius + Self::vary(rng, 0.0, 1.0);
            let w = Self::vary(rng, 2.0, 0.8).max(1.0);
            let pos = Vec2::new(cx + angle.cos() * r, cy + angle.sin() * r);
            points.push(ChainPoint {
                name: match i { 0 => "ring_0", 1 => "ring_1", 2 => "ring_2", 3 => "ring_3",
                                 4 => "ring_4", 5 => "ring_5", 6 => "ring_6", _ => "ring_7" },
                pos, prev_pos: pos, width: w, pinned: false,
            });
        }
        for i in 0..n {
            let next = (i + 1) % n;
            let rest = (points[i].pos - points[next].pos).length();
            constraints.push(Constraint { a: i, b: next, rest_length: rest });
        }
        Self { points, constraints, limbs: Vec::new() }
    }

    /// Front view: round body, two wing chains sweeping up-and-out, two legs.
    fn build_winged(rng: &mut Xorshift) -> Self {
        let cx = 9.0;
        let head_neck = Self::vary(rng, 2.2, 0.6);
        let neck_body = Self::vary(rng, 4.2, 1.0);
        let head_w = Self::vary(rng, 3.5, 1.4);
        let neck_w = Self::vary(rng, 2.5, 1.0);
        let body_w = Self::vary(rng, 5.5, 2.2);
        let head_y = 4.0;
        let neck_y = head_y + head_neck;
        let body_y = neck_y + neck_body;
        let wing_seg = Self::vary(rng, 2.6, 0.6);

        let p = |x: f32, y: f32| Vec2::new(x.clamp(0.5, 17.0), y.clamp(1.0, 23.0));
        let positions = [
            ("head",    p(cx, head_y), head_w.max(2.0)),
            ("neck",    p(cx, neck_y), neck_w.max(1.5)),
            ("body",    p(cx, body_y), body_w.max(3.0)),
            ("lwing_1", p(cx - wing_seg, body_y - 1.2),        1.5),
            ("lwing_2", p(cx - wing_seg * 1.9, body_y - 2.6),  1.0),
            ("lwing_3", p(cx - wing_seg * 2.6, body_y - 3.8),  0.5),
            ("rwing_1", p(cx + wing_seg, body_y - 1.2),        1.5),
            ("rwing_2", p(cx + wing_seg * 1.9, body_y - 2.6),  1.0),
            ("rwing_3", p(cx + wing_seg * 2.6, body_y - 3.8),  0.5),
        ];
        let points: Vec<ChainPoint> = positions.iter()
            .map(|(name, pos, w)| ChainPoint { name, pos: *pos, prev_pos: *pos, width: *w, pinned: false })
            .collect();
        // Rest lengths from actual spacing (the wing chains are diagonal).
        let pairs = [(0usize, 1usize), (1, 2), (2, 3), (3, 4), (4, 5), (2, 6), (6, 7), (7, 8)];
        let constraints: Vec<Constraint> = pairs.iter()
            .map(|&(a, b)| Constraint {
                a, b,
                rest_length: (points[a].pos - points[b].pos).length(),
            })
            .collect();

        let leg_upper = Self::vary(rng, 4.0, 0.6).max(3.2);
        let leg_lower = Self::vary(rng, 3.8, 0.6).max(3.0);
        let leg_width_upper = Self::vary(rng, 1.3, 0.3).max(0.8);
        let leg_width_lower = Self::vary(rng, 1.0, 0.2).max(0.6);
        let foot_y = (body_y + leg_upper + leg_lower).min(22.5);
        let limbs = vec![
            Limb { anchor: 2, upper_len: leg_upper, lower_len: leg_lower, upper_width: leg_width_upper, lower_width: leg_width_lower, end_effector: Vec2::new(cx - 1.8, foot_y), side: Side::Left, bend_dir: -1.0 },
            Limb { anchor: 2, upper_len: leg_upper, lower_len: leg_lower, upper_width: leg_width_upper, lower_width: leg_width_lower, end_effector: Vec2::new(cx + 1.8, foot_y), side: Side::Right, bend_dir: 1.0 },
        ];
        Self { points, constraints, limbs }
    }

    fn build_serpentine(rng: &mut Xorshift) -> Self {
        let n = 8;
        let seg_len = Self::vary(rng, 2.5, 0.75);
        let head_w = Self::vary(rng, 3.5, 1.4);
        let body_w = Self::vary(rng, 3.0, 1.2);
        let tail_w = Self::vary(rng, 1.5, 0.6);
        let start_x = 4.0;
        let cy = 12.0;
        let mut points = Vec::with_capacity(n);
        let mut constraints = Vec::with_capacity(n - 1);
        for i in 0..n {
            let x = start_x + i as f32 * seg_len;
            let w = if i == 0 { head_w.max(2.0) } else if i < n - 2 { body_w.max(1.5) } else { tail_w.max(0.5) };
            let name: &'static str = match i {
                0 => "head", 1 => "seg_1", 2 => "seg_2", 3 => "seg_3",
                4 => "seg_4", 5 => "seg_5", 6 => "seg_6", _ => "tail",
            };
            let pos = Vec2::new(x.min(17.0), cy);
            points.push(ChainPoint { name, pos, prev_pos: pos, width: w, pinned: false });
        }
        for i in 0..n - 1 {
            constraints.push(Constraint { a: i, b: i + 1, rest_length: seg_len });
        }
        Self { points, constraints, limbs: Vec::new() }
    }
}
