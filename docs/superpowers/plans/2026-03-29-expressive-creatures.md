# Expressive Creature System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace mask-based sprite generation with point-chain physics producing organic, skeleton-aware procedural animation at ~18x12 terminal cells.

**Architecture:** Five archetype skeleton topologies (bipedal, quadruped, blob, winged, serpentine) are instantiated from PRNG-seeded parameters. Verlet integration + distance constraints drive chain points; 2-bone IK plants limbs. The chain state is rasterized to a `Sprite` via outline → fill → edge-detect, then fed into the existing half-block renderer unchanged.

**Tech Stack:** Rust, ratatui, existing `Sprite`/`CellKind`/`Palette` types from `creature/render.rs`. No new crate dependencies — all math is `f32` Vec2 ops + `std::f32::consts`.

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `src/creature/mod.rs` | Modify | Add new module declarations (`physics`, `outline`, `skeleton`, `locomotion`) |
| `src/creature/generate.rs` | Modify | Keep `Xorshift`, `CellKind`, `Sprite` (with `set` made `pub`). Remove `generate_sprite`, `neighbors` (moved to `outline.rs`) |
| `src/creature/skeleton.rs` | Create | `Skeleton`, `ChainPoint`, `Constraint`, `Limb`, `Side`, `Vec2` structs. `Skeleton::instantiate(archetype, seed)` constructor. `ARCHETYPE_COUNT` constant. `archetype_name()`/`archetype_index()` helpers. |
| `src/creature/physics.rs` | Create | `verlet_integrate()`, `apply_constraints()`, `solve_two_bone_ik()`, `snap_to_grid()`. Pure functions operating on `&mut Skeleton`. |
| `src/creature/outline.rs` | Create | `rasterize_skeleton() -> Sprite`. Bresenham line, filled circle, scanline fill, edge detection. |
| `src/creature/locomotion.rs` | Create | `LocomotionState` struct (replaces `AnimationState`). Per-state drive functions: `drive_working()`, `drive_waiting()`, `drive_idle()`, `drive_sleeping()`, `drive_disconnected()`. Foot stepping logic with sigmoid easing. |
| `src/creature/templates.rs` | Remove | Replaced by archetype definitions in `skeleton.rs` |
| `src/creature/animate.rs` | Remove | Replaced by `physics.rs` + `locomotion.rs` |
| `src/creature/render.rs` | Modify | Update `sprite_cell_size` default. Keep everything else (palettes, half-block renderer, `terminal_icon_sprite`). |
| `src/app.rs` | Modify | Replace `AnimationState`/`Sprite` usage with `Skeleton`/`LocomotionState`. Update `spawn_session`, `App::new`, `tick_animations`. |
| `src/ui/dashboard.rs` | Modify | Update `CREATURE_WIDTH`/`CREATURE_HEIGHT` constants to 18/24. |
| `src/main.rs` | Modify | Parse `--test-creatures` flag, dispatch to test mode. |
| `src/test_creatures.rs` | Create | Standalone Ratatui event loop rendering 5×5 grid (archetypes × states). |
| `tests/creature_test.rs` | Rewrite | Tests for new skeleton, physics, outline, locomotion modules. |

---

### Task 1: Vec2 and Xorshift — Extract Foundation Types

**Files:**
- Modify: `src/creature/generate.rs`
- Create: `src/creature/skeleton.rs`
- Modify: `src/creature/mod.rs`
- Test: `tests/creature_test.rs`

- [ ] **Step 1: Write failing tests for Vec2 arithmetic**

In `tests/creature_test.rs`, replace the file contents with the new test foundation. We keep the Xorshift tests (they still apply) and add Vec2 tests:

```rust
use summoner::creature::generate::{CellKind, Xorshift};
use summoner::creature::skeleton::Vec2;

#[test]
fn xorshift_is_deterministic() {
    let mut rng1 = Xorshift::new(42);
    let mut rng2 = Xorshift::new(42);
    let vals1: Vec<u64> = (0..100).map(|_| rng1.next()).collect();
    let vals2: Vec<u64> = (0..100).map(|_| rng2.next()).collect();
    assert_eq!(vals1, vals2);
}

#[test]
fn xorshift_different_seeds_produce_different_output() {
    let mut rng1 = Xorshift::new(42);
    let mut rng2 = Xorshift::new(99);
    let vals1: Vec<u64> = (0..10).map(|_| rng1.next()).collect();
    let vals2: Vec<u64> = (0..10).map(|_| rng2.next()).collect();
    assert_ne!(vals1, vals2);
}

#[test]
fn vec2_add_sub() {
    let a = Vec2::new(3.0, 4.0);
    let b = Vec2::new(1.0, 2.0);
    let sum = a + b;
    assert!((sum.x - 4.0).abs() < 1e-6);
    assert!((sum.y - 6.0).abs() < 1e-6);
    let diff = a - b;
    assert!((diff.x - 2.0).abs() < 1e-6);
    assert!((diff.y - 2.0).abs() < 1e-6);
}

#[test]
fn vec2_scale() {
    let v = Vec2::new(3.0, 4.0);
    let scaled = v * 2.0;
    assert!((scaled.x - 6.0).abs() < 1e-6);
    assert!((scaled.y - 8.0).abs() < 1e-6);
}

#[test]
fn vec2_length() {
    let v = Vec2::new(3.0, 4.0);
    assert!((v.length() - 5.0).abs() < 1e-6);
}

#[test]
fn vec2_perpendicular() {
    let v = Vec2::new(1.0, 0.0);
    let perp = v.perpendicular();
    // Perpendicular to (1,0) is (0,-1) or (0,1)
    assert!((perp.x).abs() < 1e-6);
    assert!((perp.y.abs() - 1.0).abs() < 1e-6);
}

#[test]
fn vec2_normalize() {
    let v = Vec2::new(3.0, 4.0);
    let n = v.normalized();
    assert!((n.length() - 1.0).abs() < 1e-6);
    assert!((n.x - 0.6).abs() < 1e-6);
    assert!((n.y - 0.8).abs() < 1e-6);
}

#[test]
fn vec2_normalize_zero_returns_zero() {
    let v = Vec2::new(0.0, 0.0);
    let n = v.normalized();
    assert!((n.x).abs() < 1e-6);
    assert!((n.y).abs() < 1e-6);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test creature_test`
Expected: compilation error — `skeleton` module and `Vec2` don't exist yet.

- [ ] **Step 3: Create `skeleton.rs` with Vec2**

Create `src/creature/skeleton.rs`:

```rust
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
```

- [ ] **Step 4: Update `mod.rs` to declare `skeleton` module**

Change `src/creature/mod.rs` to:

```rust
pub mod generate;
pub mod templates;
pub mod animate;
pub mod render;
pub mod skeleton;
```

- [ ] **Step 5: Make `Sprite::set` public in `generate.rs`**

In `src/creature/generate.rs`, change `fn set(` to `pub fn set(` (line 48). The outline rasterizer will need to write cells into a sprite.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test creature_test`
Expected: all 8 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/creature/skeleton.rs src/creature/mod.rs src/creature/generate.rs tests/creature_test.rs
git commit -m "feat(creature): add Vec2 type and prepare skeleton module"
```

---

### Task 2: Skeleton Data Model and Archetype Definitions

**Files:**
- Modify: `src/creature/skeleton.rs`
- Test: `tests/creature_test.rs`

- [ ] **Step 1: Write failing tests for skeleton instantiation**

Append to `tests/creature_test.rs`:

```rust
use summoner::creature::skeleton::{Skeleton, ARCHETYPE_COUNT, archetype_name, archetype_index};

#[test]
fn skeleton_instantiate_bipedal_has_correct_topology() {
    let skel = Skeleton::instantiate(0, 42);
    // Bipedal: 4 spine points + 2 hip points = 6 chain points
    assert!(skel.points.len() >= 6, "Bipedal should have at least 6 chain points, got {}", skel.points.len());
    // 2 arms + 2 legs = 4 limbs
    assert_eq!(skel.limbs.len(), 4, "Bipedal should have 4 limbs");
    // Should have constraints connecting spine points
    assert!(skel.constraints.len() >= 5, "Bipedal should have at least 5 constraints");
}

#[test]
fn skeleton_instantiate_quadruped_has_correct_topology() {
    let skel = Skeleton::instantiate(1, 42);
    // Quadruped: 4 spine + 3 tail = 7 chain points
    assert!(skel.points.len() >= 7);
    // 4 legs
    assert_eq!(skel.limbs.len(), 4);
}

#[test]
fn skeleton_instantiate_blob_has_no_limbs() {
    let skel = Skeleton::instantiate(2, 42);
    // Blob: ring of 6-8 points, no limbs
    assert!(skel.points.len() >= 6);
    assert_eq!(skel.limbs.len(), 0);
}

#[test]
fn skeleton_instantiate_winged_has_wings_and_legs() {
    let skel = Skeleton::instantiate(3, 42);
    // Winged: 3 spine + 6 wing points = 9+ chain points
    assert!(skel.points.len() >= 9);
    // 2 legs (wings are chain points, not IK limbs)
    assert_eq!(skel.limbs.len(), 2);
}

#[test]
fn skeleton_instantiate_serpentine_has_long_chain() {
    let skel = Skeleton::instantiate(4, 42);
    // 6-8 chain points, no limbs
    assert!(skel.points.len() >= 6);
    assert_eq!(skel.limbs.len(), 0);
}

#[test]
fn different_seeds_produce_different_skeletons() {
    let s1 = Skeleton::instantiate(0, 100);
    let s2 = Skeleton::instantiate(0, 200);
    // Same topology (point count, limb count) but different widths/lengths
    assert_eq!(s1.points.len(), s2.points.len());
    assert_eq!(s1.limbs.len(), s2.limbs.len());
    // At least one body width should differ
    let widths_differ = s1.points.iter().zip(s2.points.iter())
        .any(|(a, b)| (a.width - b.width).abs() > 0.01);
    assert!(widths_differ, "Different seeds should produce different body proportions");
}

#[test]
fn same_seed_produces_identical_skeleton() {
    let s1 = Skeleton::instantiate(0, 42);
    let s2 = Skeleton::instantiate(0, 42);
    for (a, b) in s1.points.iter().zip(s2.points.iter()) {
        assert!((a.width - b.width).abs() < 1e-6);
        assert!((a.pos.x - b.pos.x).abs() < 1e-6);
        assert!((a.pos.y - b.pos.y).abs() < 1e-6);
    }
}

#[test]
fn archetype_count_is_five() {
    assert_eq!(ARCHETYPE_COUNT, 5);
}

#[test]
fn archetype_name_roundtrips() {
    for i in 0..ARCHETYPE_COUNT {
        let name = archetype_name(i);
        assert_eq!(archetype_index(name), i);
    }
}

#[test]
fn skeleton_points_fit_within_sprite_bounds() {
    // All archetypes should place their initial points within the 18x24 sub-pixel grid
    for archetype in 0..ARCHETYPE_COUNT {
        let skel = Skeleton::instantiate(archetype, 42);
        for pt in &skel.points {
            assert!(pt.pos.x >= 0.0 && pt.pos.x <= 18.0,
                "Archetype {} point {} x={} out of bounds", archetype, pt.name, pt.pos.x);
            assert!(pt.pos.y >= 0.0 && pt.pos.y <= 24.0,
                "Archetype {} point {} y={} out of bounds", archetype, pt.name, pt.pos.y);
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test creature_test`
Expected: compilation error — `Skeleton`, `ARCHETYPE_COUNT`, etc. don't exist.

- [ ] **Step 3: Add skeleton data model and archetype definitions to `skeleton.rs`**

Append to `src/creature/skeleton.rs` (after the existing `Vec2` code):

```rust
use crate::creature::generate::Xorshift;

pub const ARCHETYPE_COUNT: usize = 5;

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
    pub end_effector: Vec2,
    pub side: Side,
}

#[derive(Debug, Clone)]
pub struct Skeleton {
    pub points: Vec<ChainPoint>,
    pub constraints: Vec<Constraint>,
    pub limbs: Vec<Limb>,
}

impl Skeleton {
    /// Instantiate a skeleton for the given archetype index and seed.
    /// The seed controls body proportions (widths, lengths) within archetype bounds.
    /// Points are placed in a default rest pose within the 18x24 sub-pixel grid.
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
        let t = (rng.next() % 1000) as f32 / 1000.0; // 0.0 ..= ~1.0
        base + (t * 2.0 - 1.0) * range
    }

    fn build_bipedal(rng: &mut Xorshift) -> Self {
        let cx = 9.0; // center x of 18-wide grid

        // Vary segment lengths (±30% of defaults)
        let head_neck = Self::vary(rng, 3.0, 0.9);
        let neck_upper = Self::vary(rng, 4.0, 1.2);
        let upper_lower = Self::vary(rng, 3.5, 1.0);

        // Vary widths (±40%)
        let head_w = Self::vary(rng, 4.0, 1.6);
        let neck_w = Self::vary(rng, 2.5, 1.0);
        let upper_w = Self::vary(rng, 6.0, 2.4);
        let lower_w = Self::vary(rng, 5.0, 2.0);

        // Place spine top-down from y=2
        let head_y = 2.0;
        let neck_y = head_y + head_neck;
        let upper_y = neck_y + neck_upper;
        let lower_y = upper_y + upper_lower;

        let points = vec![
            ChainPoint { name: "head",       pos: Vec2::new(cx, head_y),  prev_pos: Vec2::new(cx, head_y),  width: head_w.max(2.0),  pinned: false },
            ChainPoint { name: "neck",       pos: Vec2::new(cx, neck_y),  prev_pos: Vec2::new(cx, neck_y),  width: neck_w.max(1.5),  pinned: false },
            ChainPoint { name: "upper_body", pos: Vec2::new(cx, upper_y), prev_pos: Vec2::new(cx, upper_y), width: upper_w.max(3.0), pinned: false },
            ChainPoint { name: "lower_body", pos: Vec2::new(cx, lower_y), prev_pos: Vec2::new(cx, lower_y), width: lower_w.max(2.5), pinned: false },
            ChainPoint { name: "hip_l",      pos: Vec2::new(cx - 1.5, lower_y), prev_pos: Vec2::new(cx - 1.5, lower_y), width: 1.0, pinned: false },
            ChainPoint { name: "hip_r",      pos: Vec2::new(cx + 1.5, lower_y), prev_pos: Vec2::new(cx + 1.5, lower_y), width: 1.0, pinned: false },
        ];

        let constraints = vec![
            Constraint { a: 0, b: 1, rest_length: head_neck },
            Constraint { a: 1, b: 2, rest_length: neck_upper },
            Constraint { a: 2, b: 3, rest_length: upper_lower },
            Constraint { a: 3, b: 4, rest_length: 1.5 },
            Constraint { a: 3, b: 5, rest_length: 1.5 },
        ];

        // Limb lengths (±25%)
        let arm_upper = Self::vary(rng, 2.5, 0.6);
        let arm_lower = Self::vary(rng, 2.5, 0.6);
        let leg_upper = Self::vary(rng, 3.0, 0.75);
        let leg_lower = Self::vary(rng, 3.0, 0.75);

        let foot_y = lower_y + leg_upper + leg_lower;
        let limbs = vec![
            Limb { anchor: 2, upper_len: arm_upper.max(1.5), lower_len: arm_lower.max(1.5), end_effector: Vec2::new(cx - 3.0, upper_y + arm_upper + arm_lower), side: Side::Left },
            Limb { anchor: 2, upper_len: arm_upper.max(1.5), lower_len: arm_lower.max(1.5), end_effector: Vec2::new(cx + 3.0, upper_y + arm_upper + arm_lower), side: Side::Right },
            Limb { anchor: 4, upper_len: leg_upper.max(2.0), lower_len: leg_lower.max(2.0), end_effector: Vec2::new(cx - 2.0, foot_y.min(23.0)), side: Side::Left },
            Limb { anchor: 5, upper_len: leg_upper.max(2.0), lower_len: leg_lower.max(2.0), end_effector: Vec2::new(cx + 2.0, foot_y.min(23.0)), side: Side::Right },
        ];

        Self { points, constraints, limbs }
    }

    fn build_quadruped(rng: &mut Xorshift) -> Self {
        let cy = 10.0; // vertical center — quadruped is horizontal

        let head_neck = Self::vary(rng, 3.0, 0.9);
        let neck_front = Self::vary(rng, 3.5, 1.0);
        let front_rear = Self::vary(rng, 5.0, 1.5);
        let tail1 = Self::vary(rng, 2.0, 0.6);
        let tail2 = Self::vary(rng, 2.0, 0.6);
        let tail3 = Self::vary(rng, 1.5, 0.4);

        let head_w = Self::vary(rng, 3.5, 1.4);
        let neck_w = Self::vary(rng, 3.0, 1.2);
        let front_w = Self::vary(rng, 5.0, 2.0);
        let rear_w = Self::vary(rng, 4.5, 1.8);

        // Place spine left-to-right
        let head_x = 3.0;
        let neck_x = head_x + head_neck;
        let front_x = neck_x + neck_front;
        let rear_x = front_x + front_rear;

        let points = vec![
            ChainPoint { name: "head",       pos: Vec2::new(head_x, cy - 1.0),  prev_pos: Vec2::new(head_x, cy - 1.0),  width: head_w.max(2.0),  pinned: false },
            ChainPoint { name: "neck",       pos: Vec2::new(neck_x, cy),         prev_pos: Vec2::new(neck_x, cy),         width: neck_w.max(2.0),  pinned: false },
            ChainPoint { name: "front_body", pos: Vec2::new(front_x, cy),        prev_pos: Vec2::new(front_x, cy),        width: front_w.max(3.0), pinned: false },
            ChainPoint { name: "rear_body",  pos: Vec2::new(rear_x, cy),         prev_pos: Vec2::new(rear_x, cy),         width: rear_w.max(2.5),  pinned: false },
            ChainPoint { name: "tail_1",     pos: Vec2::new(rear_x + tail1, cy), prev_pos: Vec2::new(rear_x + tail1, cy), width: 1.5,              pinned: false },
            ChainPoint { name: "tail_2",     pos: Vec2::new(rear_x + tail1 + tail2, cy), prev_pos: Vec2::new(rear_x + tail1 + tail2, cy), width: 1.0, pinned: false },
            ChainPoint { name: "tail_3",     pos: Vec2::new(rear_x + tail1 + tail2 + tail3, cy), prev_pos: Vec2::new(rear_x + tail1 + tail2 + tail3, cy), width: 0.5, pinned: false },
        ];

        let constraints = vec![
            Constraint { a: 0, b: 1, rest_length: head_neck },
            Constraint { a: 1, b: 2, rest_length: neck_front },
            Constraint { a: 2, b: 3, rest_length: front_rear },
            Constraint { a: 3, b: 4, rest_length: tail1 },
            Constraint { a: 4, b: 5, rest_length: tail2 },
            Constraint { a: 5, b: 6, rest_length: tail3 },
        ];

        let leg_upper = Self::vary(rng, 3.0, 0.75);
        let leg_lower = Self::vary(rng, 3.0, 0.75);
        let foot_y = cy + leg_upper + leg_lower;

        let limbs = vec![
            Limb { anchor: 2, upper_len: leg_upper.max(2.0), lower_len: leg_lower.max(2.0), end_effector: Vec2::new(front_x - 1.0, foot_y.min(23.0)), side: Side::Left },
            Limb { anchor: 2, upper_len: leg_upper.max(2.0), lower_len: leg_lower.max(2.0), end_effector: Vec2::new(front_x + 1.0, foot_y.min(23.0)), side: Side::Right },
            Limb { anchor: 3, upper_len: leg_upper.max(2.0), lower_len: leg_lower.max(2.0), end_effector: Vec2::new(rear_x - 1.0, foot_y.min(23.0)), side: Side::Left },
            Limb { anchor: 3, upper_len: leg_upper.max(2.0), lower_len: leg_lower.max(2.0), end_effector: Vec2::new(rear_x + 1.0, foot_y.min(23.0)), side: Side::Right },
        ];

        Self { points, constraints, limbs }
    }

    fn build_blob(rng: &mut Xorshift) -> Self {
        let cx = 9.0;
        let cy = 12.0;
        let n = 8; // ring points
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
                pos,
                prev_pos: pos,
                width: w,
                pinned: false,
            });
        }

        // Connect ring in a loop
        for i in 0..n {
            let next = (i + 1) % n;
            let rest = (points[i].pos - points[next].pos).length();
            constraints.push(Constraint { a: i, b: next, rest_length: rest });
        }

        Self { points, constraints, limbs: Vec::new() }
    }

    fn build_winged(rng: &mut Xorshift) -> Self {
        let cx = 9.0;

        let head_neck = Self::vary(rng, 2.5, 0.75);
        let neck_body = Self::vary(rng, 4.0, 1.2);

        let head_w = Self::vary(rng, 3.5, 1.4);
        let neck_w = Self::vary(rng, 2.5, 1.0);
        let body_w = Self::vary(rng, 5.5, 2.2);

        let head_y = 4.0;
        let neck_y = head_y + head_neck;
        let body_y = neck_y + neck_body;

        // Wing chain points (3 per wing, spreading outward and up)
        let wing_seg = Self::vary(rng, 3.0, 0.9);

        let points = vec![
            ChainPoint { name: "head",       pos: Vec2::new(cx, head_y),              prev_pos: Vec2::new(cx, head_y),              width: head_w.max(2.0),  pinned: false },
            ChainPoint { name: "neck",       pos: Vec2::new(cx, neck_y),              prev_pos: Vec2::new(cx, neck_y),              width: neck_w.max(1.5),  pinned: false },
            ChainPoint { name: "body",       pos: Vec2::new(cx, body_y),              prev_pos: Vec2::new(cx, body_y),              width: body_w.max(3.0),  pinned: false },
            // Left wing
            ChainPoint { name: "lwing_1",    pos: Vec2::new(cx - wing_seg, body_y - 1.0),           prev_pos: Vec2::new(cx - wing_seg, body_y - 1.0),           width: 1.5, pinned: false },
            ChainPoint { name: "lwing_2",    pos: Vec2::new(cx - wing_seg * 2.0, body_y - 2.0),     prev_pos: Vec2::new(cx - wing_seg * 2.0, body_y - 2.0),     width: 1.0, pinned: false },
            ChainPoint { name: "lwing_3",    pos: Vec2::new(cx - wing_seg * 2.5, body_y - 3.0),     prev_pos: Vec2::new(cx - wing_seg * 2.5, body_y - 3.0),     width: 0.5, pinned: false },
            // Right wing
            ChainPoint { name: "rwing_1",    pos: Vec2::new(cx + wing_seg, body_y - 1.0),           prev_pos: Vec2::new(cx + wing_seg, body_y - 1.0),           width: 1.5, pinned: false },
            ChainPoint { name: "rwing_2",    pos: Vec2::new(cx + wing_seg * 2.0, body_y - 2.0),     prev_pos: Vec2::new(cx + wing_seg * 2.0, body_y - 2.0),     width: 1.0, pinned: false },
            ChainPoint { name: "rwing_3",    pos: Vec2::new(cx + wing_seg * 2.5, body_y - 3.0),     prev_pos: Vec2::new(cx + wing_seg * 2.5, body_y - 3.0),     width: 0.5, pinned: false },
        ];

        let constraints = vec![
            Constraint { a: 0, b: 1, rest_length: head_neck },
            Constraint { a: 1, b: 2, rest_length: neck_body },
            // Left wing chain
            Constraint { a: 2, b: 3, rest_length: wing_seg },
            Constraint { a: 3, b: 4, rest_length: wing_seg },
            Constraint { a: 4, b: 5, rest_length: wing_seg * 0.5 },
            // Right wing chain
            Constraint { a: 2, b: 6, rest_length: wing_seg },
            Constraint { a: 6, b: 7, rest_length: wing_seg },
            Constraint { a: 7, b: 8, rest_length: wing_seg * 0.5 },
        ];

        let leg_upper = Self::vary(rng, 3.0, 0.75);
        let leg_lower = Self::vary(rng, 3.0, 0.75);
        let foot_y = body_y + leg_upper + leg_lower;

        let limbs = vec![
            Limb { anchor: 2, upper_len: leg_upper.max(2.0), lower_len: leg_lower.max(2.0), end_effector: Vec2::new(cx - 2.0, foot_y.min(23.0)), side: Side::Left },
            Limb { anchor: 2, upper_len: leg_upper.max(2.0), lower_len: leg_lower.max(2.0), end_effector: Vec2::new(cx + 2.0, foot_y.min(23.0)), side: Side::Right },
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
            let w = if i == 0 {
                head_w.max(2.0)
            } else if i < n - 2 {
                body_w.max(1.5)
            } else {
                tail_w.max(0.5)
            };
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test creature_test`
Expected: all tests pass (the original 8 Vec2/Xorshift tests + 10 new skeleton tests).

- [ ] **Step 5: Commit**

```bash
git add src/creature/skeleton.rs tests/creature_test.rs
git commit -m "feat(creature): add skeleton data model with 5 archetype builders"
```

---

### Task 3: Physics Engine — Verlet, Constraints, IK

**Files:**
- Create: `src/creature/physics.rs`
- Modify: `src/creature/mod.rs`
- Test: `tests/creature_test.rs`

- [ ] **Step 1: Write failing tests for physics functions**

Append to `tests/creature_test.rs`:

```rust
use summoner::creature::physics::{verlet_integrate, apply_constraints, solve_two_bone_ik};

#[test]
fn verlet_integration_moves_points() {
    let mut skel = Skeleton::instantiate(0, 42);
    let head_before = skel.points[0].pos;
    // Nudge head position to create velocity
    skel.points[0].prev_pos = skel.points[0].pos - Vec2::new(1.0, 0.0);
    verlet_integrate(&mut skel, 0.98, Vec2::new(0.0, 0.5));
    let head_after = skel.points[0].pos;
    // Head should have moved right (velocity) and down (gravity)
    assert!(head_after.x > head_before.x, "Head should move right from velocity");
    assert!(head_after.y > head_before.y, "Head should move down from gravity");
}

#[test]
fn verlet_pinned_points_dont_move() {
    let mut skel = Skeleton::instantiate(0, 42);
    skel.points[0].pinned = true;
    let head_before = skel.points[0].pos;
    skel.points[0].prev_pos = skel.points[0].pos - Vec2::new(1.0, 0.0);
    verlet_integrate(&mut skel, 0.98, Vec2::new(0.0, 0.5));
    assert!((skel.points[0].pos.x - head_before.x).abs() < 1e-6);
    assert!((skel.points[0].pos.y - head_before.y).abs() < 1e-6);
}

#[test]
fn constraints_maintain_distances() {
    let mut skel = Skeleton::instantiate(0, 42);
    // Displace head far away
    skel.points[0].pos = Vec2::new(0.0, 0.0);
    apply_constraints(&mut skel, 3);
    // After constraint solving, head-neck distance should be close to rest length
    let dist = (skel.points[0].pos - skel.points[1].pos).length();
    let rest = skel.constraints[0].rest_length;
    assert!((dist - rest).abs() < 0.5, "Constraint not satisfied: dist={dist}, rest={rest}");
}

#[test]
fn two_bone_ik_reaches_target() {
    let anchor = Vec2::new(9.0, 10.0);
    let target = Vec2::new(9.0, 16.0);
    let upper_len = 3.0;
    let lower_len = 3.0;
    let result = solve_two_bone_ik(anchor, target, upper_len, lower_len);
    // End should be at or near target
    let end_dist = (result.end - target).length();
    assert!(end_dist < 0.5, "IK end {:.1},{:.1} not near target {:.1},{:.1}", result.end.x, result.end.y, target.x, target.y);
    // Mid should be between anchor and end
    let upper_dist = (result.mid - anchor).length();
    let lower_dist = (result.end - result.mid).length();
    assert!((upper_dist - upper_len).abs() < 0.5, "Upper bone length off: {upper_dist} vs {upper_len}");
    assert!((lower_dist - lower_len).abs() < 0.5, "Lower bone length off: {lower_dist} vs {lower_len}");
}

#[test]
fn two_bone_ik_clamps_when_target_unreachable() {
    let anchor = Vec2::new(9.0, 10.0);
    let target = Vec2::new(9.0, 25.0); // Too far
    let result = solve_two_bone_ik(anchor, target, 3.0, 3.0);
    // Should extend fully toward target (straight line)
    let total = (result.end - anchor).length();
    assert!((total - 6.0).abs() < 0.5, "Should fully extend: total={total}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test creature_test`
Expected: compilation error — `physics` module doesn't exist.

- [ ] **Step 3: Create `src/creature/physics.rs`**

```rust
use super::skeleton::{Skeleton, Vec2};

pub struct IkResult {
    pub mid: Vec2,
    pub end: Vec2,
}

/// Verlet integration: update all non-pinned chain points.
/// `damping` controls velocity retention (0.0 = frozen, 1.0 = no damping).
/// `gravity` is applied as acceleration (e.g., Vec2::new(0.0, 0.5) for downward pull).
pub fn verlet_integrate(skeleton: &mut Skeleton, damping: f32, gravity: Vec2) {
    for pt in &mut skeleton.points {
        if pt.pinned {
            continue;
        }
        let velocity = (pt.pos - pt.prev_pos) * damping;
        pt.prev_pos = pt.pos;
        pt.pos = pt.pos + velocity + gravity;
    }
}

/// Enforce distance constraints between connected points.
/// `iterations` controls accuracy (2-3 is enough for chains of 4-8 points).
pub fn apply_constraints(skeleton: &mut Skeleton, iterations: usize) {
    for _ in 0..iterations {
        for ci in 0..skeleton.constraints.len() {
            let c = skeleton.constraints[ci].clone();
            let a_pos = skeleton.points[c.a].pos;
            let b_pos = skeleton.points[c.b].pos;
            let delta = b_pos - a_pos;
            let current_len = delta.length();
            if current_len < 1e-10 {
                continue;
            }
            let correction = delta * ((current_len - c.rest_length) / current_len * 0.5);
            let a_pinned = skeleton.points[c.a].pinned;
            let b_pinned = skeleton.points[c.b].pinned;
            match (a_pinned, b_pinned) {
                (false, false) => {
                    skeleton.points[c.a].pos += correction;
                    skeleton.points[c.b].pos -= correction;
                }
                (true, false) => {
                    skeleton.points[c.b].pos -= correction * 2.0;
                }
                (false, true) => {
                    skeleton.points[c.a].pos += correction * 2.0;
                }
                (true, true) => {}
            }
        }
    }
}

/// Solve 2-bone IK using the law of cosines.
/// Returns the mid-joint (elbow/knee) and end-effector positions.
pub fn solve_two_bone_ik(anchor: Vec2, target: Vec2, upper_len: f32, lower_len: f32) -> IkResult {
    let to_target = target - anchor;
    let dist = to_target.length();
    let max_reach = upper_len + lower_len;

    // If target is unreachable, fully extend toward it
    if dist >= max_reach || dist < 1e-10 {
        let dir = if dist < 1e-10 {
            Vec2::new(0.0, 1.0)
        } else {
            to_target.normalized()
        };
        return IkResult {
            mid: anchor + dir * upper_len,
            end: anchor + dir * max_reach.min(dist),
        };
    }

    // Law of cosines: angle at anchor
    let cos_angle = (upper_len * upper_len + dist * dist - lower_len * lower_len)
        / (2.0 * upper_len * dist);
    let angle = cos_angle.clamp(-1.0, 1.0).acos();

    // Direction from anchor to target
    let base_angle = to_target.y.atan2(to_target.x);
    let mid_angle = base_angle - angle; // bend to the left of the direction

    let mid = Vec2::new(
        anchor.x + mid_angle.cos() * upper_len,
        anchor.y + mid_angle.sin() * upper_len,
    );
    let end = {
        let to_end = (target - mid).normalized();
        mid + to_end * lower_len
    };

    IkResult { mid, end }
}

/// Snap point positions to the integer grid with hysteresis.
/// Only moves to a new grid cell when the float position is more than `threshold`
/// away from the current snapped position.
pub fn snap_to_grid(skeleton: &mut Skeleton, threshold: f32) {
    for pt in &mut skeleton.points {
        let snapped_x = pt.pos.x.round();
        let snapped_y = pt.pos.y.round();
        // Only update if significantly different from current rounded position
        if (pt.pos.x - snapped_x).abs() > threshold {
            pt.pos.x = snapped_x;
        }
        if (pt.pos.y - snapped_y).abs() > threshold {
            pt.pos.y = snapped_y;
        }
    }
}
```

- [ ] **Step 4: Add `physics` module to `mod.rs`**

In `src/creature/mod.rs`, add `pub mod physics;`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test creature_test`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/creature/physics.rs src/creature/mod.rs tests/creature_test.rs
git commit -m "feat(creature): add Verlet physics, distance constraints, and 2-bone IK"
```

---

### Task 4: Outline Rasterizer — Chain to Sprite

**Files:**
- Create: `src/creature/outline.rs`
- Modify: `src/creature/mod.rs`
- Test: `tests/creature_test.rs`

- [ ] **Step 1: Write failing tests for rasterization**

Append to `tests/creature_test.rs`:

```rust
use summoner::creature::outline::rasterize_skeleton;
use summoner::creature::generate::CellKind;

#[test]
fn rasterize_bipedal_produces_non_empty_sprite() {
    let skel = Skeleton::instantiate(0, 42);
    let sprite = rasterize_skeleton(&skel);
    assert_eq!(sprite.width, 18);
    assert_eq!(sprite.height, 24);
    let filled = sprite.cells.iter().filter(|c| **c != CellKind::Empty).count();
    assert!(filled > 10, "Rasterized bipedal should have significant fill, got {filled}");
}

#[test]
fn rasterize_all_archetypes_produce_visible_sprites() {
    for archetype in 0..ARCHETYPE_COUNT {
        let skel = Skeleton::instantiate(archetype, 42);
        let sprite = rasterize_skeleton(&skel);
        let filled = sprite.cells.iter().filter(|c| **c != CellKind::Empty).count();
        assert!(filled > 5, "Archetype {} produced only {filled} filled cells", archetype_name(archetype));
    }
}

#[test]
fn rasterized_sprite_has_borders_around_body() {
    let skel = Skeleton::instantiate(0, 42);
    let sprite = rasterize_skeleton(&skel);
    let has_body = sprite.cells.iter().any(|c| *c == CellKind::Body);
    let has_border = sprite.cells.iter().any(|c| *c == CellKind::Border);
    assert!(has_body, "Should have Body cells");
    assert!(has_border, "Should have Border cells");
}

#[test]
fn rasterize_same_skeleton_is_deterministic() {
    let skel = Skeleton::instantiate(0, 42);
    let s1 = rasterize_skeleton(&skel);
    let s2 = rasterize_skeleton(&skel);
    assert_eq!(s1.cells, s2.cells);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test creature_test`
Expected: compilation error — `outline` module doesn't exist.

- [ ] **Step 3: Create `src/creature/outline.rs`**

```rust
use super::generate::{CellKind, Sprite};
use super::physics::solve_two_bone_ik;
use super::skeleton::{Skeleton, Vec2};

const SPRITE_W: usize = 18;
const SPRITE_H: usize = 24;

/// Rasterize a skeleton into a Sprite (18x24 sub-pixel grid).
pub fn rasterize_skeleton(skeleton: &Skeleton) -> Sprite {
    let mut cells = vec![CellKind::Empty; SPRITE_W * SPRITE_H];

    // Draw body outline from chain points
    draw_body_outline(skeleton, &mut cells);

    // Draw limbs via IK
    for limb in &skeleton.limbs {
        let anchor = skeleton.points[limb.anchor].pos;
        let ik = solve_two_bone_ik(anchor, limb.end_effector, limb.upper_len, limb.lower_len);
        draw_line(anchor, ik.mid, &mut cells);
        draw_line(ik.mid, ik.end, &mut cells);
    }

    // Draw head circle
    if let Some(head) = skeleton.points.first() {
        let radius = (head.width / 2.0).max(1.0);
        draw_filled_circle(head.pos, radius, &mut cells);
    }

    // Scanline fill interior
    scanline_fill(&mut cells);

    // Edge detection: Body cells adjacent to Empty become Border
    edge_detect(&mut cells);

    Sprite { width: SPRITE_W, height: SPRITE_H, cells }
}

fn draw_body_outline(skeleton: &Skeleton, cells: &mut [CellKind]) {
    let spine_points = &skeleton.points;
    if spine_points.len() < 2 {
        return;
    }

    // Detect if this is a ring (blob) by checking if first and last points are
    // connected by a constraint — i.e., constraint list connects N-1 to 0
    let is_ring = skeleton.limbs.is_empty() && skeleton.constraints.iter().any(|c| {
        (c.a == spine_points.len() - 1 && c.b == 0) || (c.a == 0 && c.b == spine_points.len() - 1)
    });

    if is_ring {
        // Blob: connect ring points as polygon outline
        for i in 0..spine_points.len() {
            let next = (i + 1) % spine_points.len();
            draw_line(spine_points[i].pos, spine_points[next].pos, cells);
        }
        return;
    }

    // Spine-based: compute left/right boundary from perpendicular offsets
    let mut left_boundary = Vec::new();
    let mut right_boundary = Vec::new();

    for i in 0..spine_points.len() {
        let pt = &spine_points[i];
        // Direction: from previous to next (or endpoint tangent)
        let dir = if i == 0 {
            spine_points[1].pos - pt.pos
        } else if i == spine_points.len() - 1 {
            pt.pos - spine_points[i - 1].pos
        } else {
            spine_points[i + 1].pos - spine_points[i - 1].pos
        };
        let perp = dir.perpendicular().normalized();
        let half_w = pt.width / 2.0;
        left_boundary.push(pt.pos + perp * half_w);
        right_boundary.push(pt.pos - perp * half_w);
    }

    // Draw left boundary lines
    for i in 0..left_boundary.len() - 1 {
        draw_line(left_boundary[i], left_boundary[i + 1], cells);
    }
    // Draw right boundary lines
    for i in 0..right_boundary.len() - 1 {
        draw_line(right_boundary[i], right_boundary[i + 1], cells);
    }
    // Connect top and bottom
    draw_line(left_boundary[0], right_boundary[0], cells);
    if let (Some(l), Some(r)) = (left_boundary.last(), right_boundary.last()) {
        draw_line(*l, *r, cells);
    }
}

/// Bresenham line drawing — sets cells to Border along the line.
fn draw_line(from: Vec2, to: Vec2, cells: &mut [CellKind]) {
    let x0 = from.x.round() as i32;
    let y0 = from.y.round() as i32;
    let x1 = to.x.round() as i32;
    let y1 = to.y.round() as i32;

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let sy: i32 = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;

    loop {
        set_cell(x, y, CellKind::Border, cells);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn draw_filled_circle(center: Vec2, radius: f32, cells: &mut [CellKind]) {
    let cx = center.x.round() as i32;
    let cy = center.y.round() as i32;
    let r = radius.round() as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                set_cell(cx + dx, cy + dy, CellKind::Border, cells);
            }
        }
    }
}

/// Scanline fill: for each row, find leftmost and rightmost Border cells, fill between as Body.
fn scanline_fill(cells: &mut [CellKind]) {
    for y in 0..SPRITE_H {
        let mut left = None;
        let mut right = None;
        for x in 0..SPRITE_W {
            if cells[y * SPRITE_W + x] == CellKind::Border {
                if left.is_none() {
                    left = Some(x);
                }
                right = Some(x);
            }
        }
        if let (Some(l), Some(r)) = (left, right) {
            for x in (l + 1)..r {
                if cells[y * SPRITE_W + x] == CellKind::Empty {
                    cells[y * SPRITE_W + x] = CellKind::Body;
                }
            }
        }
    }
}

/// Edge detection: Body cells adjacent to Empty become Border.
fn edge_detect(cells: &mut [CellKind]) {
    let snapshot = cells.to_vec();
    for y in 0..SPRITE_H {
        for x in 0..SPRITE_W {
            let idx = y * SPRITE_W + x;
            if snapshot[idx] == CellKind::Body {
                let has_empty = [(0i32, -1), (0, 1), (-1, 0), (1, 0)].iter().any(|(dx, dy)| {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < SPRITE_W as i32 && ny >= 0 && ny < SPRITE_H as i32 {
                        snapshot[ny as usize * SPRITE_W + nx as usize] == CellKind::Empty
                    } else {
                        true // edge of sprite counts as empty
                    }
                });
                if has_empty {
                    cells[idx] = CellKind::Border;
                }
            }
        }
    }
}

fn set_cell(x: i32, y: i32, kind: CellKind, cells: &mut [CellKind]) {
    if x >= 0 && x < SPRITE_W as i32 && y >= 0 && y < SPRITE_H as i32 {
        cells[y as usize * SPRITE_W + x as usize] = kind;
    }
}
```

- [ ] **Step 4: Add `outline` module to `mod.rs`**

In `src/creature/mod.rs`, add `pub mod outline;`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test creature_test`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/creature/outline.rs src/creature/mod.rs tests/creature_test.rs
git commit -m "feat(creature): add outline rasterizer — chain to sprite pipeline"
```

---

### Task 5: Locomotion State Machine

**Files:**
- Create: `src/creature/locomotion.rs`
- Modify: `src/creature/mod.rs`
- Test: `tests/creature_test.rs`

- [ ] **Step 1: Write failing tests for locomotion**

Append to `tests/creature_test.rs`:

```rust
use summoner::creature::locomotion::LocomotionState;
use summoner::session::SessionState;
use std::time::Duration;

#[test]
fn locomotion_new_creates_valid_state() {
    let skel = Skeleton::instantiate(0, 42);
    let loco = LocomotionState::new(skel, SessionState::Working);
    assert_eq!(loco.state(), SessionState::Working);
}

#[test]
fn locomotion_tick_modifies_skeleton() {
    let skel = Skeleton::instantiate(0, 42);
    let mut loco = LocomotionState::new(skel, SessionState::Working);
    let before = loco.skeleton().points[0].pos;
    // Tick several times to allow locomotion to move the creature
    for _ in 0..10 {
        loco.tick(Duration::from_millis(33));
    }
    let after = loco.skeleton().points[0].pos;
    // Working state should produce movement
    let moved = (after.x - before.x).abs() > 0.1 || (after.y - before.y).abs() > 0.1;
    assert!(moved, "Working locomotion should move the head: before={:?} after={:?}", before, after);
}

#[test]
fn locomotion_sleeping_is_static() {
    let skel = Skeleton::instantiate(0, 42);
    let mut loco = LocomotionState::new(skel, SessionState::Sleeping);
    // Let it settle
    for _ in 0..20 {
        loco.tick(Duration::from_millis(33));
    }
    let before = loco.skeleton().points[0].pos;
    for _ in 0..10 {
        loco.tick(Duration::from_millis(33));
    }
    let after = loco.skeleton().points[0].pos;
    // Sleeping should be very slow — movement per tick should be tiny
    let dx = (after.x - before.x).abs();
    let dy = (after.y - before.y).abs();
    assert!(dx < 2.0 && dy < 2.0, "Sleeping should have minimal movement: dx={dx}, dy={dy}");
}

#[test]
fn locomotion_state_change_resets() {
    let skel = Skeleton::instantiate(0, 42);
    let mut loco = LocomotionState::new(skel, SessionState::Working);
    for _ in 0..5 {
        loco.tick(Duration::from_millis(33));
    }
    loco.set_state(SessionState::Idle);
    assert_eq!(loco.state(), SessionState::Idle);
}

#[test]
fn locomotion_disconnected_is_frozen() {
    let skel = Skeleton::instantiate(0, 42);
    let mut loco = LocomotionState::new(skel, SessionState::Disconnected);
    // Let it settle
    for _ in 0..15 {
        loco.tick(Duration::from_millis(33));
    }
    let before: Vec<Vec2> = loco.skeleton().points.iter().map(|p| p.pos).collect();
    for _ in 0..10 {
        loco.tick(Duration::from_millis(33));
    }
    let after: Vec<Vec2> = loco.skeleton().points.iter().map(|p| p.pos).collect();
    // After settling, disconnected should be completely frozen
    for (b, a) in before.iter().zip(after.iter()) {
        assert!((b.x - a.x).abs() < 0.01 && (b.y - a.y).abs() < 0.01,
            "Disconnected should freeze after settling");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test creature_test`
Expected: compilation error — `locomotion` module doesn't exist.

- [ ] **Step 3: Create `src/creature/locomotion.rs`**

```rust
use std::time::Duration;

use crate::session::SessionState;
use super::physics::{apply_constraints, snap_to_grid, verlet_integrate};
use super::skeleton::{Skeleton, Vec2};

/// Sigmoid easing for foot arcs: snappy lift-off and landing.
fn sigmoid(t: f32) -> f32 {
    1.0 / (1.0 + (-10.0 * (t - 0.5)).exp())
}

/// Tracks a single foot's stepping animation.
#[derive(Clone)]
struct FootStep {
    start_x: f32,
    target_x: f32,
    start_y: f32,
    arc_height: f32, // how high the foot lifts (in sub-pixels)
    progress: f32,   // 0.0 to 1.0
    stepping: bool,
}

impl FootStep {
    fn new() -> Self {
        Self { start_x: 0.0, target_x: 0.0, start_y: 0.0, arc_height: 2.0, progress: 0.0, stepping: false }
    }

    /// Start a step from current position to target.
    fn begin(&mut self, from_x: f32, to_x: f32, ground_y: f32) {
        self.start_x = from_x;
        self.target_x = to_x;
        self.start_y = ground_y;
        self.arc_height = 2.0;
        self.progress = 0.0;
        self.stepping = true;
    }

    /// Advance the step, returns (x, y) of the foot.
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
        // Parabolic arc: peaks at t=0.5
        let arc = self.arc_height * 4.0 * t * (1.0 - t);
        let y = self.start_y - arc;
        (x, y)
    }
}

pub struct LocomotionState {
    skeleton: Skeleton,
    base_widths: Vec<f32>, // original widths from skeleton instantiation
    state: SessionState,
    elapsed: f32,
    archetype: usize,
    foot_steps: Vec<FootStep>, // one per limb
    settled: bool,
}

impl LocomotionState {
    pub fn new(skeleton: Skeleton, state: SessionState) -> Self {
        let base_widths: Vec<f32> = skeleton.points.iter().map(|p| p.width).collect();
        let archetype = detect_archetype(&skeleton);
        let foot_steps = skeleton.limbs.iter().map(|_| FootStep::new()).collect();
        let mut ls = Self {
            skeleton,
            base_widths,
            state,
            elapsed: 0.0,
            archetype,
            foot_steps,
            settled: false,
        };
        if state == SessionState::Disconnected {
            ls.settle_disconnected();
        }
        ls
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn skeleton(&self) -> &Skeleton {
        &self.skeleton
    }

    pub fn set_state(&mut self, state: SessionState) {
        if state == self.state {
            return;
        }
        self.state = state;
        self.elapsed = 0.0;
        self.settled = false;
        // Restore base widths when changing state (prevent drift)
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
            SessionState::Disconnected => {} // frozen after settle
            SessionState::ShellOnly => {}    // no creature
        }
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

    fn drive_working_bipedal(&mut self, dt: f32) {
        let amplitude = 3.0;
        let frequency = 2.0;
        let offset = (self.elapsed * frequency * std::f32::consts::TAU).sin() * amplitude;

        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x = 9.0 + offset;
        }

        verlet_integrate(&mut self.skeleton, 0.98, Vec2::new(0.0, 0.1));
        apply_constraints(&mut self.skeleton, 3);
        self.update_foot_stepping_with_arc(dt, 5.0);
        self.update_arm_swing();
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_working_quadruped(&mut self, dt: f32) {
        // Quadruped: horizontal spine, head moves forward/backward
        let amplitude = 2.0;
        let frequency = 1.5;
        let offset = (self.elapsed * frequency * std::f32::consts::TAU).sin() * amplitude;

        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x += offset * dt * 3.0;
        }

        verlet_integrate(&mut self.skeleton, 0.98, Vec2::new(0.0, 0.1));
        apply_constraints(&mut self.skeleton, 3);
        // Diagonal gait: front-left(0) + rear-right(3) step together,
        // then front-right(1) + rear-left(2)
        self.update_diagonal_gait(dt);
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_working_blob(&mut self, _dt: f32) {
        // Asymmetric ring contraction to "roll"
        let phase = self.elapsed * 1.5 * std::f32::consts::TAU;
        let n = self.skeleton.points.len();
        for i in 0..n {
            let point_phase = phase + (i as f32 / n as f32) * std::f32::consts::TAU;
            let squeeze = point_phase.sin() * 1.5;
            let bw = self.base_widths[i];
            self.skeleton.points[i].width = (bw + squeeze * 0.3).max(0.5);
            // Also shift position slightly for rolling motion
            self.skeleton.points[i].pos.x += point_phase.cos() * 0.05;
        }
        verlet_integrate(&mut self.skeleton, 0.98, Vec2::new(0.0, 0.05));
        apply_constraints(&mut self.skeleton, 3);
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_working_winged(&mut self, dt: f32) {
        // Bipedal walk + wing flap overlay
        let amplitude = 2.5;
        let frequency = 1.8;
        let offset = (self.elapsed * frequency * std::f32::consts::TAU).sin() * amplitude;

        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x = 9.0 + offset;
        }

        // Wing flap: oscillate wing chain points vertically
        let flap_phase = self.elapsed * 4.0 * std::f32::consts::TAU;
        for pt in &mut self.skeleton.points {
            if pt.name.starts_with("lwing") || pt.name.starts_with("rwing") {
                let flap_offset = flap_phase.sin() * 2.0;
                pt.pos.y += flap_offset * 0.1;
            }
        }

        verlet_integrate(&mut self.skeleton, 0.98, Vec2::new(0.0, 0.1));
        apply_constraints(&mut self.skeleton, 3);
        self.update_foot_stepping_with_arc(dt, 5.0);
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    fn drive_working_serpentine(&mut self, _dt: f32) {
        // Sine wave propagation along the chain
        let n = self.skeleton.points.len();
        for i in 0..n {
            let phase = self.elapsed * 2.0 * std::f32::consts::TAU
                - (i as f32 / n as f32) * std::f32::consts::TAU * 1.5;
            let wave = phase.sin() * 2.0;
            self.skeleton.points[i].pos.y = 12.0 + wave; // oscillate around center
        }
        // Also move the whole body forward/backward slowly
        let drift = (self.elapsed * 0.5 * std::f32::consts::TAU).sin() * 2.0;
        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x += drift * 0.02;
        }

        verlet_integrate(&mut self.skeleton, 0.98, Vec2::new(0.0, 0.0)); // no gravity for serpentine
        apply_constraints(&mut self.skeleton, 3);
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    // --- Waiting state ---

    fn drive_waiting(&mut self, _dt: f32) {
        let offset = (self.elapsed * 1.0 * std::f32::consts::TAU).sin() * 1.5;
        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x = 9.0 + offset;
        }

        verlet_integrate(&mut self.skeleton, 0.95, Vec2::new(0.0, 0.1));
        apply_constraints(&mut self.skeleton, 3);
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    // --- Idle state ---

    fn drive_idle(&mut self, _dt: f32) {
        // Breathing: oscillate widths around base values (no cumulative drift)
        let breath = (self.elapsed * 0.8 * std::f32::consts::TAU).sin() * 0.3;
        for (pt, &bw) in self.skeleton.points.iter_mut().zip(self.base_widths.iter()) {
            pt.width = bw + breath * 0.3;
        }

        let head_nudge = (self.elapsed * 0.3 * std::f32::consts::TAU).sin() * 0.5;
        if let Some(head) = self.skeleton.points.first_mut() {
            head.pos.x = 9.0 + head_nudge;
        }

        verlet_integrate(&mut self.skeleton, 0.90, Vec2::new(0.0, 0.05));
        apply_constraints(&mut self.skeleton, 2);
        snap_to_grid(&mut self.skeleton, 0.7);
        self.clamp_to_bounds();
    }

    // --- Sleeping state ---

    fn drive_sleeping(&mut self, _dt: f32) {
        // Very slow breathing around base widths
        let breath = (self.elapsed * 0.4 * std::f32::consts::TAU).sin() * 0.2;
        for (pt, &bw) in self.skeleton.points.iter_mut().zip(self.base_widths.iter()) {
            pt.width = bw + breath * 0.15;
        }

        verlet_integrate(&mut self.skeleton, 0.85, Vec2::new(0.0, 0.02));
        apply_constraints(&mut self.skeleton, 2);
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
        for pt in &mut self.skeleton.points {
            pt.prev_pos = pt.pos;
        }
        self.settled = true;
    }

    // --- Foot stepping with sigmoid-eased parabolic arc ---

    fn update_foot_stepping_with_arc(&mut self, dt: f32, step_speed: f32) {
        if self.skeleton.limbs.is_empty() {
            return;
        }

        let com_x: f32 = self.skeleton.points.iter()
            .map(|p| p.pos.x).sum::<f32>() / self.skeleton.points.len() as f32;

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

    /// Quadruped diagonal gait: front-left + rear-right step together,
    /// then front-right + rear-left.
    fn update_diagonal_gait(&mut self, dt: f32) {
        if self.skeleton.limbs.len() < 4 {
            return;
        }

        let com_x: f32 = self.skeleton.points.iter()
            .map(|p| p.pos.x).sum::<f32>() / self.skeleton.points.len() as f32;

        // Pair indices: (0,3) and (1,2) for diagonal gait
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

    /// Swing arms in opposition to legs (for bipedal/winged).
    fn update_arm_swing(&mut self) {
        if self.skeleton.limbs.len() < 4 {
            return;
        }
        // Limbs 0,1 = arms, 2,3 = legs (by convention from bipedal builder)
        // Swing arm end effectors opposite to the leg on the other side
        let leg_l_x = self.skeleton.limbs[2].end_effector.x;
        let leg_r_x = self.skeleton.limbs[3].end_effector.x;
        let anchor_l = self.skeleton.points[self.skeleton.limbs[0].anchor].pos;
        let anchor_r = self.skeleton.points[self.skeleton.limbs[1].anchor].pos;

        // Left arm swings opposite to right leg
        let arm_swing = 1.5;
        self.skeleton.limbs[0].end_effector.x = anchor_l.x - (leg_r_x - anchor_r.x).signum() * arm_swing;
        self.skeleton.limbs[0].end_effector.y = anchor_l.y + self.skeleton.limbs[0].upper_len + self.skeleton.limbs[0].lower_len * 0.5;
        // Right arm swings opposite to left leg
        self.skeleton.limbs[1].end_effector.x = anchor_r.x - (leg_l_x - anchor_l.x).signum() * arm_swing;
        self.skeleton.limbs[1].end_effector.y = anchor_r.y + self.skeleton.limbs[1].upper_len + self.skeleton.limbs[1].lower_len * 0.5;
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

/// Detect archetype from skeleton topology.
/// 0=bipedal, 1=quadruped, 2=blob, 3=winged, 4=serpentine
fn detect_archetype(skeleton: &Skeleton) -> usize {
    // Blob: ring topology (has constraint connecting last to first)
    let is_ring = skeleton.limbs.is_empty() && skeleton.constraints.iter().any(|c| {
        let n = skeleton.points.len();
        (c.a == n - 1 && c.b == 0) || (c.a == 0 && c.b == n - 1)
    });
    if is_ring { return 2; }

    // Serpentine: no limbs, long chain (not a ring)
    if skeleton.limbs.is_empty() && skeleton.points.len() >= 6 { return 4; }

    // Winged: has points named "lwing_*"
    if skeleton.points.iter().any(|p| p.name.starts_with("lwing")) { return 3; }

    // Quadruped: 4 limbs and has a "tail_1" point
    if skeleton.limbs.len() == 4 && skeleton.points.iter().any(|p| p.name == "tail_1") { return 1; }

    // Default: bipedal
    0
}
```

- [ ] **Step 4: Add `locomotion` module to `mod.rs`**

In `src/creature/mod.rs`, add `pub mod locomotion;`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test creature_test`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/creature/locomotion.rs src/creature/mod.rs tests/creature_test.rs
git commit -m "feat(creature): add locomotion state machine with per-state physics"
```

---

### Task 6: Creature Test Mode

**Files:**
- Create: `src/test_creatures.rs`
- Modify: `src/main.rs`
- Modify: `src/lib.rs`

This is built first (before integrating into the dashboard) so it can be used to visually iterate on all earlier work.

- [ ] **Step 1: Write `src/test_creatures.rs`**

```rust
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::DefaultTerminal;

use crate::creature::locomotion::LocomotionState;
use crate::creature::outline::rasterize_skeleton;
use crate::creature::render::{render_sprite_to_buffer, state_palette};
use crate::creature::skeleton::{Skeleton, ARCHETYPE_COUNT, archetype_name};
use crate::session::SessionState;

const CELL_W: u16 = 20; // terminal cols per creature cell (18 sprite + 2 padding)
const CELL_H: u16 = 14; // terminal rows per creature cell (12 sprite + 2 padding)

const STATES: &[SessionState] = &[
    SessionState::Working,
    SessionState::Waiting,
    SessionState::Idle,
    SessionState::Sleeping,
    SessionState::Disconnected,
];

struct TestGrid {
    seed: u64,
    locomotions: Vec<Vec<LocomotionState>>, // [archetype][state]
}

impl TestGrid {
    fn new(seed: u64) -> Self {
        let mut locomotions = Vec::new();
        for archetype in 0..ARCHETYPE_COUNT {
            let mut row = Vec::new();
            for &state in STATES {
                let skel = Skeleton::instantiate(archetype, seed);
                row.push(LocomotionState::new(skel, state));
            }
            locomotions.push(row);
        }
        Self { seed, locomotions }
    }

    fn randomize(&mut self) {
        self.seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        *self = Self::new(self.seed);
    }

    fn tick(&mut self, dt: Duration) {
        for row in &mut self.locomotions {
            for loco in row {
                loco.tick(dt);
            }
        }
    }
}

pub fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut grid = TestGrid::new(42);
    let mut last_tick = Instant::now();

    loop {
        let dt = last_tick.elapsed();
        last_tick = Instant::now();
        grid.tick(dt);

        terminal.draw(|frame| {
            let area = frame.area();
            let buf = frame.buffer_mut();

            // Clear
            let bg = Style::default().bg(Color::Rgb(20, 20, 30));
            for y in area.y..area.y + area.height {
                for x in area.x..area.x + area.width {
                    if let Some(cell) = buf.cell_mut(Position { x, y }) {
                        cell.set_symbol(" ");
                        cell.set_style(bg);
                    }
                }
            }

            // Title
            let title = format!(" Creature Test Mode — seed: {} — [r] randomize  [q] quit ", grid.seed);
            let title_style = Style::default().fg(Color::Rgb(200, 200, 220)).bg(Color::Rgb(30, 30, 50));
            for (i, ch) in title.chars().enumerate() {
                let x = area.x + i as u16;
                if x >= area.x + area.width { break; }
                if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
                    cell.set_symbol(&ch.to_string());
                    cell.set_style(title_style);
                }
            }

            let grid_y = area.y + 2;
            let label_w: u16 = 12; // width for archetype labels

            // Column headers (state names)
            for (col, &state) in STATES.iter().enumerate() {
                let x = area.x + label_w + col as u16 * CELL_W;
                let style = Style::default().fg(state.color());
                let label = state.label();
                for (i, ch) in label.chars().enumerate() {
                    let px = x + i as u16;
                    if px >= area.x + area.width { break; }
                    if let Some(cell) = buf.cell_mut(Position { x: px, y: grid_y }) {
                        cell.set_symbol(&ch.to_string());
                        cell.set_style(style);
                    }
                }
            }

            let content_y = grid_y + 1;

            // Render grid
            for (row, archetype) in (0..ARCHETYPE_COUNT).enumerate() {
                let row_y = content_y + row as u16 * CELL_H;

                // Row label
                let name = archetype_name(archetype);
                let label_style = Style::default().fg(Color::Rgb(140, 140, 160));
                for (i, ch) in name.chars().enumerate() {
                    let x = area.x + i as u16;
                    if x >= area.x + area.width { break; }
                    let y = row_y + CELL_H / 2;
                    if y >= area.y + area.height { break; }
                    if let Some(cell) = buf.cell_mut(Position { x, y }) {
                        cell.set_symbol(&ch.to_string());
                        cell.set_style(label_style);
                    }
                }

                for (col, &state) in STATES.iter().enumerate() {
                    let cell_x = area.x + label_w + col as u16 * CELL_W;
                    let cell_y = row_y;

                    if cell_y + CELL_H > area.y + area.height { break; }
                    if cell_x + CELL_W > area.x + area.width { break; }

                    let creature_area = Rect {
                        x: cell_x + 1,
                        y: cell_y,
                        width: 18,
                        height: 12,
                    };

                    let loco = &grid.locomotions[archetype][col];
                    let sprite = rasterize_skeleton(loco.skeleton());
                    let palette = state_palette(state);
                    render_sprite_to_buffer(&sprite, &palette, creature_area, buf);
                }
            }
        })?;

        if event::poll(Duration::from_millis(33))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => grid.randomize(),
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Add module to `lib.rs`**

In `src/lib.rs`, add `pub mod test_creatures;`.

- [ ] **Step 3: Update `main.rs` to parse `--test-creatures` flag**

Replace `src/main.rs` with:

```rust
use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--test-creatures") {
        let mut terminal = ratatui::init();
        let result = summoner::test_creatures::run(&mut terminal);
        ratatui::restore();
        return result;
    }

    let mut terminal = ratatui::init();
    let result = summoner::app::run(&mut terminal);
    ratatui::restore();
    result
}
```

- [ ] **Step 4: Build and verify it compiles**

Run: `cargo build`
Expected: compiles successfully.

- [ ] **Step 5: Commit**

```bash
git add src/test_creatures.rs src/main.rs src/lib.rs
git commit -m "feat: add --test-creatures mode for visual iteration"
```

---

### Task 7: Integrate New System into Dashboard

**Files:**
- Modify: `src/app.rs`
- Modify: `src/ui/dashboard.rs`
- Modify: `src/creature/mod.rs`
- Test: `tests/creature_test.rs`

This is the integration task — replace the old creature API calls in `app.rs` and `dashboard.rs` with the new skeleton/locomotion/outline pipeline.

- [ ] **Step 1: Update `src/ui/dashboard.rs` constants**

Change line 14-15:

```rust
const CREATURE_WIDTH: u16 = 18;
const CREATURE_HEIGHT: u16 = 24;
```

The rest of `dashboard.rs` already uses these constants for layout math — the creature rendering call passes `creature_area` to `render_sprite_to_buffer`, which is unchanged.

- [ ] **Step 2: Update `src/app.rs` imports**

Replace the three creature-related import lines:

```rust
use crate::creature::animate::AnimationState;
use crate::creature::generate::{generate_sprite, Sprite};
use crate::creature::templates::{get_template, template_index, template_name, TEMPLATE_COUNT};
```

With:

```rust
use crate::creature::generate::Sprite;
use crate::creature::locomotion::LocomotionState;
use crate::creature::outline::rasterize_skeleton;
use crate::creature::skeleton::{Skeleton, ARCHETYPE_COUNT, archetype_index, archetype_name};
```

- [ ] **Step 3: Update `App` struct and `App::new()`**

In the `App` struct, replace the `animations` field:

```rust
    animations: Vec<AnimationState>,
```

With:

```rust
    locomotions: Vec<LocomotionState>,
```

In `App::new()`, in the session restoration loop (`for entry in &store.sessions`), change:

```rust
            let template_idx = template_index(&entry.creature_template);
            let mask = get_template(template_idx);
            let sprite = generate_sprite(&mask, entry.creature_seed);
```

To:

```rust
            let archetype_idx = archetype_index(&entry.creature_template);
            let skeleton = Skeleton::instantiate(archetype_idx, entry.creature_seed);
            let sprite = rasterize_skeleton(&skeleton);
```

And change:

```rust
            animations.push(AnimationState::new(SessionState::Disconnected));
```

To:

```rust
            locomotions.push(LocomotionState::new(skeleton, SessionState::Disconnected));
```

In the `App` struct initializer, replace `animations` with `locomotions`.

- [ ] **Step 4: Update `spawn_session()`**

In `spawn_session()`, replace the creature creation block:

```rust
        let template_idx = self.sessions.len() % TEMPLATE_COUNT;
        let creature_template = template_name(template_idx).to_string();
        let mask = get_template(template_idx);
        let sprite = generate_sprite(&mask, seed);
```

With:

```rust
        let archetype_idx = self.sessions.len() % ARCHETYPE_COUNT;
        let creature_template = archetype_name(archetype_idx).to_string();
        let skeleton = Skeleton::instantiate(archetype_idx, seed);
        let sprite = rasterize_skeleton(&skeleton);
```

And replace:

```rust
        self.animations.push(AnimationState::new(SessionState::ShellOnly));
```

With:

```rust
        self.locomotions.push(LocomotionState::new(skeleton, SessionState::ShellOnly));
```

- [ ] **Step 5: Update all `animations` references to `locomotions`**

Search `app.rs` for every remaining reference to `animations` and replace with `locomotions`. Specifically:
- `self.animations[i].set_state(...)` → `self.locomotions[i].set_state(...)` (appears in state update logic and `resume_session`)
- `self.animations.remove(idx)` → `self.locomotions.remove(idx)` (in `close_session`)
- The `tick_animations` method: replace its body with locomotion ticking + re-rasterization:

```rust
    fn tick_animations(&mut self, dt: Duration) {
        for (i, loco) in self.locomotions.iter_mut().enumerate() {
            loco.tick(dt);
            // Re-rasterize sprite from updated skeleton
            self.sprites[i] = rasterize_skeleton(loco.skeleton());
        }
    }
```

- [ ] **Step 6: Update dashboard rendering — remove `animations` field**

In `dashboard.rs`, the `Dashboard` struct has both `sprites` and `animations` fields. Since sprites are now pre-rasterized from the locomotion skeleton each tick, we no longer need `animations` or `animate_sprite`. Make these changes:

1. Remove the `animations` field from the `Dashboard` struct (keep `sprites`)
2. Remove the `animations` parameter from `Dashboard::new()`
3. Remove the import `use crate::creature::animate::{animate_sprite, AnimationState};`

Then replace the creature rendering block in the `Widget` impl:

```rust
                    if let (Some(sprite), Some(anim)) =
                        (self.sprites.get(sess_idx), self.animations.get(sess_idx))
                    {
                        let animated = animate_sprite(sprite, anim);
                        let palette = state_palette(session.state);
                        render_sprite_to_buffer(&animated, &palette, creature_area, buf);
                    }
```

With:

```rust
                    if let Some(sprite) = self.sprites.get(sess_idx) {
                        let palette = state_palette(session.state);
                        render_sprite_to_buffer(sprite, &palette, creature_area, buf);
                    }
```

Update the import in `dashboard.rs` — remove `use crate::creature::animate::{animate_sprite, AnimationState};`. The `Sprite` import from `crate::creature::generate::Sprite` stays.

- [ ] **Step 7: Update Dashboard construction call in `app.rs`**

Find where `Dashboard::new()` is called in `app.rs` and remove the `&self.animations` argument. It should become:

```rust
Dashboard::new(&self.sessions, &self.sprites, &self.nav)
```

- [ ] **Step 8: Build and verify it compiles**

Run: `cargo build`
Expected: compiles. There may be unused import warnings for the old `animate` module — that's fine, we'll clean up next.

- [ ] **Step 9: Run tests**

Run: `cargo test`
Expected: all tests pass. Some old creature tests that reference `generate_sprite` with masks will have been replaced in Task 1.

- [ ] **Step 10: Commit**

```bash
git add src/app.rs src/ui/dashboard.rs
git commit -m "feat: integrate skeleton-based creatures into dashboard"
```

---

### Task 8: Remove Old Modules and Clean Up

**Files:**
- Modify: `src/creature/mod.rs`
- Modify: `src/creature/generate.rs`
- Remove content from: `src/creature/templates.rs`
- Remove content from: `src/creature/animate.rs`

- [ ] **Step 1: Remove old module declarations**

In `src/creature/mod.rs`, remove `pub mod templates;` and `pub mod animate;`. The file should now be:

```rust
pub mod generate;
pub mod render;
pub mod skeleton;
pub mod physics;
pub mod outline;
pub mod locomotion;
```

- [ ] **Step 2: Remove `generate_sprite` and `neighbors` from `generate.rs`**

In `src/creature/generate.rs`, remove the `generate_sprite` function (lines 62-119) and the `neighbors` function (lines 121-137). Keep `Xorshift`, `CellKind`, and `Sprite` (these are still used throughout the codebase).

The file should contain only:
- `Xorshift` struct and impl
- `CellKind` enum
- `Sprite` struct and impl (with `set` now `pub`)

- [ ] **Step 3: Delete old files**

Delete `src/creature/templates.rs` and `src/creature/animate.rs` (they are no longer referenced by `mod.rs`).

- [ ] **Step 4: Fix any remaining imports**

Search for any remaining references to the old modules:
- `use crate::creature::templates::` — should be none after Task 7
- `use crate::creature::animate::` — should be none after Task 7
- `use summoner::creature::animate::` in tests — was replaced in Task 1

- [ ] **Step 5: Build and run all tests**

Run: `cargo build && cargo test`
Expected: clean compile, all tests pass, no warnings about unused modules.

- [ ] **Step 6: Commit**

```bash
git add -A src/creature/
git commit -m "refactor: remove old mask/template/animation modules"
```

---

### Task 9: Final Integration Test and Polish

**Files:**
- Test: `tests/creature_test.rs`

- [ ] **Step 1: Add end-to-end rendering test**

Append to `tests/creature_test.rs`:

```rust
use summoner::creature::render::{render_sprite_to_buffer, state_palette, sprite_cell_size};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[test]
fn full_pipeline_skeleton_to_rendered_buffer() {
    // Test the complete pipeline: instantiate → rasterize → render
    for archetype in 0..ARCHETYPE_COUNT {
        let skel = Skeleton::instantiate(archetype, 42);
        let sprite = rasterize_skeleton(&skel);
        let (w, h) = sprite_cell_size(&sprite);
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        let palette = state_palette(SessionState::Working);
        render_sprite_to_buffer(&sprite, &palette, area, &mut buf);
        let non_empty = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|(x, y)| {
                let cell = &buf[ratatui::layout::Position { x: *x, y: *y }];
                cell.symbol() != " "
            })
            .count();
        assert!(non_empty > 0, "Archetype {} rendered nothing", archetype_name(archetype));
    }
}

#[test]
fn locomotion_produces_changing_sprites_over_time() {
    let skel = Skeleton::instantiate(0, 42);
    let mut loco = LocomotionState::new(skel, SessionState::Working);
    let sprite1 = rasterize_skeleton(loco.skeleton());
    for _ in 0..20 {
        loco.tick(Duration::from_millis(33));
    }
    let sprite2 = rasterize_skeleton(loco.skeleton());
    // Working animation should produce different sprites over time
    assert_ne!(sprite1.cells, sprite2.cells, "Working animation should change sprite over time");
}
```

- [ ] **Step 2: Run full test suite**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 3: Run `--test-creatures` visually**

Run: `cargo run --release -- --test-creatures`
Expected: a 5×5 grid of animated creatures. Press `r` to randomize, `q` to quit. Verify:
- All 5 archetypes render visible creatures
- Working creatures show movement
- Sleeping creatures are mostly static with subtle breathing
- Disconnected creatures are frozen
- `r` produces visually different creatures

- [ ] **Step 4: Commit**

```bash
git add tests/creature_test.rs
git commit -m "test: add end-to-end pipeline and animation tests"
```

---

## Task Dependency Summary

```
Task 1 (Vec2 + foundation)
  └→ Task 2 (Skeleton data model)
       └→ Task 3 (Physics engine)
       └→ Task 4 (Outline rasterizer)  ← depends on Task 3 for IK
            └→ Task 5 (Locomotion)     ← depends on Task 3 + 4
                 └→ Task 6 (Test mode) ← depends on Task 5
                 └→ Task 7 (Dashboard integration) ← depends on Task 5
                      └→ Task 8 (Cleanup old modules)
                           └→ Task 9 (Final tests + polish)
```
