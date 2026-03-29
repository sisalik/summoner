# Creature Outlines & Debug Tooling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the broken Bresenham+scanline outline algorithm with SDF capsule evaluation, add per-capsule shading, and build feature-gated CLI debug tooling (PNG render command + TUI enhancements).

**Architecture:** SDF capsule evaluation replaces the current outline.rs rasterizer. Each bone becomes a tapered capsule; every pixel is evaluated against all capsules to determine Body/Border/Empty + which capsule is closest. The capsule ID feeds a shade lookup in the renderer. Debug tooling (PNG output, test TUI) is behind a `dev-creature` cargo feature.

**Tech Stack:** Rust, ratatui, `png` crate (optional, feature-gated)

**Working directory:** `/home/siim/dev/summoner/.worktrees/expressive-creatures/` (NOT the main repo root — the expressive-creatures worktree has the skeleton/physics/locomotion system that master lacks)

---

### Task 1: Add limb width fields to Skeleton

**Files:**
- Modify: `src/creature/skeleton.rs:123-130` (Limb struct)
- Modify: `src/creature/skeleton.rs:186-196` (build_bipedal limb construction)
- Modify: `src/creature/skeleton.rs:239-244` (build_quadruped limb construction)
- Modify: `src/creature/skeleton.rs:310-313` (build_winged limb construction)

- [ ] **Step 1: Add `upper_width` and `lower_width` fields to the `Limb` struct**

In `src/creature/skeleton.rs`, change the `Limb` struct:

```rust
#[derive(Debug, Clone)]
pub struct Limb {
    pub anchor: usize,
    pub upper_len: f32,
    pub lower_len: f32,
    pub upper_width: f32,
    pub lower_width: f32,
    pub end_effector: Vec2,
    pub side: Side,
}
```

- [ ] **Step 2: Update `build_bipedal` to populate limb widths**

In `build_bipedal`, add PRNG-varied widths. After the existing `arm_upper`/`arm_lower`/`leg_upper`/`leg_lower` declarations (around line 186), add:

```rust
let arm_width_upper = Self::vary(rng, 1.2, 0.3).max(0.8);
let arm_width_lower = Self::vary(rng, 1.0, 0.2).max(0.6);
let leg_width_upper = Self::vary(rng, 1.5, 0.4).max(0.8);
let leg_width_lower = Self::vary(rng, 1.2, 0.3).max(0.6);
```

Then update each Limb in the `limbs` vec to include the new fields:

```rust
let limbs = vec![
    Limb { anchor: 2, upper_len: arm_upper.max(1.5), lower_len: arm_lower.max(1.5), upper_width: arm_width_upper, lower_width: arm_width_lower, end_effector: Vec2::new(cx - 3.0, upper_y + arm_upper + arm_lower), side: Side::Left },
    Limb { anchor: 2, upper_len: arm_upper.max(1.5), lower_len: arm_lower.max(1.5), upper_width: arm_width_upper, lower_width: arm_width_lower, end_effector: Vec2::new(cx + 3.0, upper_y + arm_upper + arm_lower), side: Side::Right },
    Limb { anchor: 4, upper_len: leg_upper.max(2.0), lower_len: leg_lower.max(2.0), upper_width: leg_width_upper, lower_width: leg_width_lower, end_effector: Vec2::new(cx - 2.0, foot_y.min(23.0)), side: Side::Left },
    Limb { anchor: 5, upper_len: leg_upper.max(2.0), lower_len: leg_lower.max(2.0), upper_width: leg_width_upper, lower_width: leg_width_lower, end_effector: Vec2::new(cx + 2.0, foot_y.min(23.0)), side: Side::Right },
];
```

- [ ] **Step 3: Update `build_quadruped` to populate limb widths**

Add after existing leg length declarations:

```rust
let leg_width_upper = Self::vary(rng, 1.3, 0.3).max(0.8);
let leg_width_lower = Self::vary(rng, 1.0, 0.2).max(0.6);
```

Update each Limb to include `upper_width: leg_width_upper, lower_width: leg_width_lower`.

- [ ] **Step 4: Update `build_winged` to populate limb widths**

Add after existing leg length declarations:

```rust
let leg_width_upper = Self::vary(rng, 1.3, 0.3).max(0.8);
let leg_width_lower = Self::vary(rng, 1.0, 0.2).max(0.6);
```

Update each Limb to include `upper_width: leg_width_upper, lower_width: leg_width_lower`.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check` from the worktree directory.
Expected: Compile errors in `outline.rs` and `test_creatures.rs` where Limb is constructed without the new fields — that's expected and will be fixed in subsequent tasks.

- [ ] **Step 6: Commit**

```bash
git add src/creature/skeleton.rs
git commit -m "feat: add upper_width/lower_width to Limb struct"
```

---

### Task 2: Implement SDF capsule rasterizer

**Files:**
- Rewrite: `src/creature/outline.rs` (replace entire contents)

This is the core of the change. Replace all existing outline code with SDF-based evaluation.

- [ ] **Step 1: Write the SDF helper functions and `RasterResult` type**

Replace the entire contents of `src/creature/outline.rs` with:

```rust
use super::generate::{CellKind, Sprite};
use super::physics::solve_two_bone_ik;
use super::skeleton::{Skeleton, Vec2};

const BASE_W: usize = 18;
const BASE_H: usize = 24;
const BORDER_WIDTH: f32 = 1.0;

/// Result of rasterizing a skeleton: sprite grid + per-pixel capsule IDs.
pub struct RasterResult {
    pub sprite: Sprite,
    pub capsule_ids: Vec<u8>,
}

/// Capsule definition for SDF evaluation.
struct Capsule {
    a: Vec2,
    b: Vec2,
    r_a: f32,
    r_b: f32,
    id: u8,
}

/// Signed distance from point `p` to a tapered capsule (line segment with
/// linearly interpolated radius from `r_a` at `a` to `r_b` at `b`).
fn sdf_tapered_capsule(p: Vec2, a: Vec2, b: Vec2, r_a: f32, r_b: f32) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let ab_dot = ab.dot(ab);
    if ab_dot < 1e-10 {
        // Degenerate capsule (zero-length segment) — treat as circle
        return (p - a).length() - r_a;
    }
    let t = ap.dot(ab) / ab_dot;
    let t = t.clamp(0.0, 1.0);
    let closest = a + ab * t;
    let radius = r_a + (r_b - r_a) * t;
    (p - closest).length() - radius
}

/// Test if point `p` is inside triangle (v0, v1, v2) using barycentric coordinates.
fn point_in_triangle(p: Vec2, v0: Vec2, v1: Vec2, v2: Vec2) -> bool {
    let d00 = (v1 - v0).dot(v1 - v0);
    let d01 = (v1 - v0).dot(v2 - v0);
    let d11 = (v2 - v0).dot(v2 - v0);
    let d20 = (p - v0).dot(v1 - v0);
    let d21 = (p - v0).dot(v2 - v0);
    let denom = d00 * d11 - d01 * d01;
    if denom.abs() < 1e-10 {
        return false;
    }
    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    v >= 0.0 && w >= 0.0 && (v + w) <= 1.0
}
```

- [ ] **Step 2: Write the capsule collection function**

Append to `outline.rs`:

```rust
/// Capsule ID scheme:
///   1         = head
///   2..=N+1   = spine segment N (constraint between chain points)
///   100+i*2   = limb i upper bone
///   100+i*2+1 = limb i lower bone
///   200+i     = wing membrane triangle i
fn collect_capsules(skeleton: &Skeleton, scale: f32) -> Vec<Capsule> {
    let mut capsules = Vec::new();

    // Head capsule (zero-length, just a circle)
    if let Some(head) = skeleton.points.first() {
        let r = (head.width * scale / 2.0).max(1.0);
        let pos = head.pos * scale;
        capsules.push(Capsule {
            a: pos,
            b: pos,
            r_a: r,
            r_b: r,
            id: 1,
        });
    }

    // Spine segment capsules from constraints between chain points
    // (skip constraints that connect to limb anchors like hip points for bipedal)
    for (ci, c) in skeleton.constraints.iter().enumerate() {
        let pa = &skeleton.points[c.a];
        let pb = &skeleton.points[c.b];
        capsules.push(Capsule {
            a: pa.pos * scale,
            b: pb.pos * scale,
            r_a: (pa.width * scale / 2.0).max(0.5),
            r_b: (pb.width * scale / 2.0).max(0.5),
            id: 2 + ci as u8,
        });
    }

    // Limb capsules via IK solve
    for (li, limb) in skeleton.limbs.iter().enumerate() {
        let anchor = skeleton.points[limb.anchor].pos * scale;
        let target = limb.end_effector * scale;
        let ik = solve_two_bone_ik(
            anchor, target,
            limb.upper_len * scale,
            limb.lower_len * scale,
        );
        // Upper bone: anchor → mid
        capsules.push(Capsule {
            a: anchor,
            b: ik.mid,
            r_a: (limb.upper_width * scale / 2.0).max(0.5),
            r_b: (limb.upper_width * scale / 2.0).max(0.5),
            id: 100 + li as u8 * 2,
        });
        // Lower bone: mid → end
        capsules.push(Capsule {
            a: ik.mid,
            b: ik.end,
            r_a: (limb.lower_width * scale / 2.0).max(0.5),
            r_b: (limb.lower_width * scale / 2.0).max(0.5),
            id: 100 + li as u8 * 2 + 1,
        });
    }

    capsules
}
```

- [ ] **Step 3: Write the wing membrane triangle collection**

Append to `outline.rs`:

```rust
struct MembraneTriangle {
    v0: Vec2,
    v1: Vec2,
    v2: Vec2,
    id: u8,
}

/// Collect wing membrane triangles for winged archetype.
/// Each wing has triangles: (body_anchor, wing_N, wing_N+1).
fn collect_wing_membranes(skeleton: &Skeleton, scale: f32) -> Vec<MembraneTriangle> {
    let mut triangles = Vec::new();

    // Find body anchor point (the point named "body")
    let body_idx = skeleton.points.iter().position(|p| p.name == "body");
    let body_pos = match body_idx {
        Some(i) => skeleton.points[i].pos * scale,
        None => return triangles,
    };

    // Collect left wing points and right wing points
    let lwing_indices: Vec<usize> = skeleton.points.iter().enumerate()
        .filter(|(_, p)| p.name.starts_with("lwing"))
        .map(|(i, _)| i)
        .collect();
    let rwing_indices: Vec<usize> = skeleton.points.iter().enumerate()
        .filter(|(_, p)| p.name.starts_with("rwing"))
        .map(|(i, _)| i)
        .collect();

    let mut tri_id = 0u8;
    for wing_points in [&lwing_indices, &rwing_indices] {
        if wing_points.len() < 2 { continue; }
        for i in 0..wing_points.len() - 1 {
            let p1 = skeleton.points[wing_points[i]].pos * scale;
            let p2 = skeleton.points[wing_points[i + 1]].pos * scale;
            triangles.push(MembraneTriangle {
                v0: body_pos,
                v1: p1,
                v2: p2,
                id: 200 + tri_id,
            });
            tri_id += 1;
        }
    }

    triangles
}
```

- [ ] **Step 4: Write the main SDF rasterization function**

Append to `outline.rs`:

```rust
/// Rasterize a skeleton into a sprite using SDF capsule evaluation.
/// Returns sprite + capsule ID map for shading.
pub fn rasterize_skeleton(skeleton: &Skeleton) -> RasterResult {
    rasterize_skeleton_scaled(skeleton, 1)
}

/// Rasterize at `scale`x resolution. Output is (18*scale) x (24*scale).
pub fn rasterize_skeleton_scaled(skeleton: &Skeleton, scale: usize) -> RasterResult {
    let scale_f = scale as f32;
    let w = BASE_W * scale;
    let h = BASE_H * scale;
    let mut cells = vec![CellKind::Empty; w * h];
    let mut ids = vec![0u8; w * h];

    let capsules = collect_capsules(skeleton, scale_f);
    let membranes = collect_wing_membranes(skeleton, scale_f);

    // Evaluate SDF at each pixel
    for y in 0..h {
        for x in 0..w {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let mut min_dist = f32::MAX;
            let mut closest_id = 0u8;

            for cap in &capsules {
                let d = sdf_tapered_capsule(p, cap.a, cap.b, cap.r_a, cap.r_b);
                if d < min_dist {
                    min_dist = d;
                    closest_id = cap.id;
                }
            }

            let idx = y * w + x;
            if min_dist <= 0.0 {
                cells[idx] = CellKind::Body;
                ids[idx] = closest_id;
            } else if min_dist <= BORDER_WIDTH {
                cells[idx] = CellKind::Border;
                ids[idx] = closest_id;
            }
        }
    }

    // Wing membrane fill: mark empty pixels inside membrane triangles as Body
    for tri in &membranes {
        // Compute bounding box for the triangle
        let min_x = tri.v0.x.min(tri.v1.x).min(tri.v2.x).floor().max(0.0) as usize;
        let max_x = tri.v0.x.max(tri.v1.x).max(tri.v2.x).ceil().min(w as f32 - 1.0) as usize;
        let min_y = tri.v0.y.min(tri.v1.y).min(tri.v2.y).floor().max(0.0) as usize;
        let max_y = tri.v0.y.max(tri.v1.y).max(tri.v2.y).ceil().min(h as f32 - 1.0) as usize;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let idx = y * w + x;
                if cells[idx] == CellKind::Empty {
                    let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    if point_in_triangle(p, tri.v0, tri.v1, tri.v2) {
                        cells[idx] = CellKind::Body;
                        ids[idx] = tri.id;
                    }
                }
            }
        }
    }

    // Edge detection: Body pixels adjacent to Empty become Border
    let snapshot = cells.clone();
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if snapshot[idx] == CellKind::Body {
                let has_empty = [(0i32, -1), (0, 1), (-1, 0), (1, 0)].iter().any(|(dx, dy)| {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        snapshot[ny as usize * w + nx as usize] == CellKind::Empty
                    } else {
                        true
                    }
                });
                if has_empty {
                    cells[idx] = CellKind::Border;
                }
            }
        }
    }

    RasterResult {
        sprite: Sprite { width: w, height: h, cells },
        capsule_ids: ids,
    }
}
```

- [ ] **Step 5: Write the wireframe rasterizer (for debug mode)**

Append to `outline.rs`. This keeps the Bresenham line approach for wireframe-only rendering:

```rust
/// Wireframe debug rasterizer: draws skeleton lines + joints, no fill.
/// Returns sprite + component IDs for coloring.
pub fn rasterize_skeleton_wireframe(skeleton: &Skeleton, scale: usize) -> RasterResult {
    let scale_f = scale as f32;
    let w = BASE_W * scale;
    let h = BASE_H * scale;
    let mut cells = vec![CellKind::Empty; w * h];
    let mut ids = vec![0u8; w * h];

    // Draw spine chain as lines between constrained points
    for (ci, c) in skeleton.constraints.iter().enumerate() {
        let a = skeleton.points[c.a].pos * scale_f;
        let b = skeleton.points[c.b].pos * scale_f;
        let id = 2 + ci as u8;
        draw_line(a, b, &mut cells, &mut ids, id, w, h);
    }

    // Draw limbs via IK
    for (li, limb) in skeleton.limbs.iter().enumerate() {
        let anchor = skeleton.points[limb.anchor].pos * scale_f;
        let target = limb.end_effector * scale_f;
        let ik = solve_two_bone_ik(anchor, target, limb.upper_len * scale_f, limb.lower_len * scale_f);
        let upper_id = 100 + li as u8 * 2;
        let lower_id = 100 + li as u8 * 2 + 1;
        draw_line(anchor, ik.mid, &mut cells, &mut ids, upper_id, w, h);
        draw_line(ik.mid, ik.end, &mut cells, &mut ids, lower_id, w, h);
    }

    // Draw joint dots at each chain point
    for (i, pt) in skeleton.points.iter().enumerate() {
        let pos = pt.pos * scale_f;
        let id = if i == 0 { 1 } else { 2 + i as u8 };
        let r = if i == 0 { (pt.width * scale_f / 2.0).max(1.0) } else { 1.0 };
        draw_filled_circle(pos, r, &mut cells, &mut ids, id, w, h);
    }

    // Draw end-effector dots for limbs
    for (li, limb) in skeleton.limbs.iter().enumerate() {
        let target = limb.end_effector * scale_f;
        let id = 100 + li as u8 * 2 + 1;
        draw_filled_circle(target, 1.0, &mut cells, &mut ids, id, w, h);
    }

    RasterResult {
        sprite: Sprite { width: w, height: h, cells },
        capsule_ids: ids,
    }
}

// --- Bresenham helpers (used only by wireframe mode) ---

fn draw_line(from: Vec2, to: Vec2, cells: &mut [CellKind], ids: &mut [u8], id: u8, w: usize, h: usize) {
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
        if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
            let idx = y as usize * w + x as usize;
            cells[idx] = CellKind::Border;
            ids[idx] = id;
        }
        if x == x1 && y == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x += sx; }
        if e2 <= dx { err += dx; y += sy; }
    }
}

fn draw_filled_circle(center: Vec2, radius: f32, cells: &mut [CellKind], ids: &mut [u8], id: u8, w: usize, h: usize) {
    let cx = center.x.round() as i32;
    let cy = center.y.round() as i32;
    let r = radius.round() as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                let x = cx + dx;
                let y = cy + dy;
                if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
                    let idx = y as usize * w + x as usize;
                    cells[idx] = CellKind::Border;
                    ids[idx] = id;
                }
            }
        }
    }
}
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo check` from the worktree directory.
Expected: May have errors in files that call the old `rasterize_skeleton_debug` or use old return types. Those are fixed in the next tasks.

- [ ] **Step 7: Commit**

```bash
git add src/creature/outline.rs
git commit -m "feat: replace Bresenham+scanline with SDF capsule rasterizer"
```

---

### Task 3: Update renderer for per-capsule shading

**Files:**
- Modify: `src/creature/render.rs` (add shaded rendering, shade offset logic)

- [ ] **Step 1: Add shade offset function and shaded render function**

In `src/creature/render.rs`, add these functions after the existing `kind_color` function:

```rust
/// Apply a lightness offset to an RGB color.
/// Positive = lighter, negative = darker. Clamped to 0-255.
pub fn shade_color(color: Color, offset: i16) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as i16 + offset).clamp(0, 255) as u8,
            (g as i16 + offset).clamp(0, 255) as u8,
            (b as i16 + offset).clamp(0, 255) as u8,
        ),
        other => other,
    }
}

/// Map a capsule ID to a shade offset for per-capsule shading.
///   1         = head → +40 (lightest)
///   2..99     = spine segments → 0 (base)
///   100+i*2   = limb upper bone → -20
///   100+i*2+1 = limb lower bone → -40
///   200+      = wing membrane → +20
pub fn capsule_shade_offset(id: u8) -> i16 {
    match id {
        0 => 0,
        1 => 40,          // head
        2..=99 => 0,       // spine
        200..=255 => 20,   // wing membrane
        id if id >= 100 => {
            if (id - 100) % 2 == 0 { -20 } else { -40 } // upper / lower limb
        }
        _ => 0,
    }
}

/// Render a sprite with per-capsule shading using the state palette + capsule IDs.
pub fn render_sprite_shaded(
    sprite: &Sprite,
    capsule_ids: &[u8],
    palette: &Palette,
    area: Rect,
    buf: &mut Buffer,
) {
    let rows = (sprite.height + 1) / 2;

    for row in 0..rows.min(area.height as usize) {
        for col in 0..sprite.width.min(area.width as usize) {
            let upper_y = row * 2;
            let lower_y = row * 2 + 1;

            let upper = sprite.get(col, upper_y);
            let lower = if lower_y < sprite.height {
                sprite.get(col, lower_y)
            } else {
                CellKind::Empty
            };

            let upper_id = capsule_ids[upper_y * sprite.width + col];
            let lower_id = if lower_y < sprite.height {
                capsule_ids[lower_y * sprite.width + col]
            } else {
                0
            };

            let pos = Position {
                x: area.x + col as u16,
                y: area.y + row as u16,
            };

            if let Some(cell) = buf.cell_mut(pos) {
                let upper_color = shaded_kind_color(&upper, upper_id, palette);
                let lower_color = shaded_kind_color(&lower, lower_id, palette);

                match (upper, lower) {
                    (CellKind::Empty, CellKind::Empty) => {}
                    (CellKind::Empty, _) => {
                        cell.set_symbol(LOWER_HALF);
                        cell.set_style(Style::default().fg(lower_color));
                    }
                    (_, CellKind::Empty) => {
                        cell.set_symbol(UPPER_HALF);
                        cell.set_style(Style::default().fg(upper_color));
                    }
                    (_, _) => {
                        if upper_color == lower_color {
                            cell.set_symbol(FULL_BLOCK);
                            cell.set_style(Style::default().fg(upper_color));
                        } else {
                            cell.set_symbol(LOWER_HALF);
                            cell.set_style(Style::default().fg(lower_color).bg(upper_color));
                        }
                    }
                }
            }
        }
    }
}

fn shaded_kind_color(kind: &CellKind, capsule_id: u8, palette: &Palette) -> Color {
    match kind {
        CellKind::Body => shade_color(palette.body, capsule_shade_offset(capsule_id)),
        CellKind::Border => palette.border,
        CellKind::Empty => Color::Reset,
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles (new functions are additive, no existing signatures changed).

- [ ] **Step 3: Commit**

```bash
git add src/creature/render.rs
git commit -m "feat: add per-capsule shaded rendering"
```

---

### Task 4: Update app.rs to use new rasterizer output

**Files:**
- Modify: `src/app.rs` (store `RasterResult` instead of bare `Sprite`, pass capsule IDs to renderer)
- Modify: `src/ui/dashboard.rs` (use shaded renderer)

- [ ] **Step 1: Update app.rs imports and storage**

In `src/app.rs`, change the import:

```rust
// Old:
use crate::creature::outline::rasterize_skeleton;
// New:
use crate::creature::outline::{rasterize_skeleton, RasterResult};
```

Change the `sprites` field type in the `App` struct from `Vec<Sprite>` to `Vec<RasterResult>`. Find where `sprites` is declared and update:

```rust
// In the App struct:
sprites: Vec<RasterResult>,
```

- [ ] **Step 2: Update all places that create or use sprites**

Where sprites are created (search for `rasterize_skeleton` calls in app.rs):

```rust
// Old:
let sprite = rasterize_skeleton(&skeleton);
// New:
let raster = rasterize_skeleton(&skeleton);
```

Store the `RasterResult` in the sprites vec instead of the `Sprite`.

Where sprites are passed to the dashboard, update to pass both sprite and capsule_ids. Update the Dashboard `new()` call to pass `&[RasterResult]` instead of `&[Sprite]`.

In `tick_animations`:
```rust
fn tick_animations(&mut self, dt: Duration) {
    for (i, loco) in self.locomotions.iter_mut().enumerate() {
        loco.tick(dt);
        self.sprites[i] = rasterize_skeleton(loco.skeleton());
    }
}
```

- [ ] **Step 3: Update dashboard.rs to accept RasterResult and use shaded rendering**

In `src/ui/dashboard.rs`, change imports:

```rust
use crate::creature::outline::RasterResult;
use crate::creature::render::{render_sprite_shaded, state_palette, terminal_icon_sprite, render_sprite_to_buffer};
```

Change `Dashboard` struct to accept `&'a [RasterResult]`:

```rust
pub struct Dashboard<'a> {
    sessions: &'a [Session],
    rasters: &'a [RasterResult],
    nav: &'a DashboardNav,
}

impl<'a> Dashboard<'a> {
    pub fn new(
        sessions: &'a [Session],
        rasters: &'a [RasterResult],
        nav: &'a DashboardNav,
    ) -> Self {
        Self { sessions, rasters, nav }
    }
}
```

In the rendering code where creature sprites are drawn, replace:

```rust
// Old:
if let Some(sprite) = self.sprites.get(sess_idx) {
    let palette = state_palette(session.state);
    render_sprite_to_buffer(sprite, &palette, creature_area, buf);
}
// New:
if let Some(raster) = self.rasters.get(sess_idx) {
    let palette = state_palette(session.state);
    render_sprite_shaded(&raster.sprite, &raster.capsule_ids, &palette, creature_area, buf);
}
```

The `terminal_icon_sprite()` path for ShellOnly sessions still uses `render_sprite_to_buffer` (flat palette, no capsule IDs) — that's correct since terminal icons don't have capsules.

- [ ] **Step 4: Verify it compiles and runs**

Run: `cargo check` then `cargo run --release` briefly to verify the dashboard still renders.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/ui/dashboard.rs
git commit -m "feat: integrate SDF rasterizer and shaded rendering into app"
```

---

### Task 5: Update test_creatures.rs for new outline API

**Files:**
- Modify: `src/test_creatures.rs` (use `RasterResult`, remove `State` color mode, add pause/frame-step)

- [ ] **Step 1: Update imports and color modes**

In `src/test_creatures.rs`, update imports:

```rust
// Remove:
use crate::creature::outline::{rasterize_skeleton, rasterize_skeleton_debug, rasterize_skeleton_scaled, rasterize_skeleton_wireframe};
// Add:
use crate::creature::outline::{rasterize_skeleton, rasterize_skeleton_scaled, rasterize_skeleton_wireframe, RasterResult};
```

Change `ColorMode` enum — remove `State`, rename `Limb` for clarity:

```rust
#[derive(Clone, Copy, PartialEq)]
enum ColorMode {
    Shaded,    // state palette + per-capsule shading
    Limb,      // rainbow colors by component ID
    Wireframe, // skeleton lines only
}

impl ColorMode {
    fn next(self) -> Self {
        match self {
            Self::Shaded => Self::Limb,
            Self::Limb => Self::Wireframe,
            Self::Wireframe => Self::Shaded,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Shaded => "shaded",
            Self::Limb => "limb colors",
            Self::Wireframe => "wireframe",
        }
    }
}
```

- [ ] **Step 2: Add pause and frame-stepping to ZoomView**

Add fields to `ZoomView`:

```rust
struct ZoomView {
    seed: u64,
    archetype: usize,
    state_idx: usize,
    loco: Option<LocomotionState>,
    color_mode: ColorMode,
    paused: bool,
    tick_count: u32,
}
```

Update `ZoomView::new` to initialize `paused: false, tick_count: 0`.

Update `ZoomView::tick`:

```rust
fn tick(&mut self, dt: Duration) {
    if self.paused { return; }
    if let Some(loco) = &mut self.loco {
        loco.tick(dt);
        self.tick_count += 1;
    }
}

fn step_forward(&mut self) {
    if let Some(loco) = &mut self.loco {
        loco.tick(Duration::from_millis(33));
        self.tick_count += 1;
    }
}

fn step_backward(&mut self) {
    if self.tick_count == 0 { return; }
    let target = self.tick_count - 1;
    // Re-simulate from scratch
    self.rebuild();
    for _ in 0..target {
        if let Some(loco) = &mut self.loco {
            loco.tick(Duration::from_millis(33));
        }
    }
    self.tick_count = target;
}
```

Update `rebuild` to reset `tick_count`:

```rust
fn rebuild(&mut self) {
    self.loco = if self.state_idx < ANIM_STATES.len() {
        let skel = Skeleton::instantiate(self.archetype, self.seed);
        Some(LocomotionState::new(skel, ANIM_STATES[self.state_idx]))
    } else {
        None
    };
    self.tick_count = 0;
}
```

- [ ] **Step 3: Update key handling for pause/step**

In the zoom mode key handler, add:

```rust
KeyCode::Char(' ') => {
    zoom.paused = !zoom.paused;
}
KeyCode::Char('.') => {
    if zoom.paused {
        zoom.step_forward();
    }
}
KeyCode::Char(',') => {
    if zoom.paused {
        zoom.step_backward();
    }
}
```

- [ ] **Step 4: Update render_grid to use SDF rasterizer**

In `render_grid`, replace the creature rendering:

```rust
let skel = grid.skeleton_for(archetype, col);
let raster = rasterize_skeleton(&skel);
let palette = if col < ANIM_STATES.len() {
    state_palette(ANIM_STATES[col])
} else {
    state_palette(SessionState::Idle)
};
render_sprite_shaded(&raster.sprite, &raster.capsule_ids, &palette, creature_area, buf);
```

Add the import for `render_sprite_shaded` at the top:

```rust
use crate::creature::render::{render_sprite_shaded, state_palette};
```

- [ ] **Step 5: Update render_zoom to use RasterResult**

Replace the zoom rendering logic:

```rust
let skel = zoom.current_skeleton();

let raster = match zoom.color_mode {
    ColorMode::Shaded => rasterize_skeleton_scaled(&skel, scale),
    ColorMode::Limb => rasterize_skeleton_scaled(&skel, scale),
    ColorMode::Wireframe => rasterize_skeleton_wireframe(&skel, scale),
};

// ... (area calculation stays the same) ...

match zoom.color_mode {
    ColorMode::Shaded => {
        let palette = if zoom.state_idx < ANIM_STATES.len() {
            state_palette(ANIM_STATES[zoom.state_idx])
        } else {
            state_palette(SessionState::Idle)
        };
        render_sprite_shaded(&raster.sprite, &raster.capsule_ids, &palette, creature_area, buf);
    }
    ColorMode::Limb | ColorMode::Wireframe => {
        render_sprite_with_components(&raster.sprite, &raster.capsule_ids, creature_area, buf);
    }
}
```

- [ ] **Step 6: Update the pause indicator in zoom title**

Add pause state to the title string:

```rust
let pause_str = if zoom.paused { " PAUSED" } else { "" };
let title = format!(
    " ZOOM: {} / {} [{}]{} — seed: {} — [space] pause  [,/.] step  [arrows] nav  [c] color  [r] seed  [z] back ",
    archetype_name(zoom.archetype), state_name, zoom.color_mode.label(), pause_str, zoom.seed,
);
```

- [ ] **Step 7: Verify it compiles**

Run: `cargo check`

- [ ] **Step 8: Commit**

```bash
git add src/test_creatures.rs
git commit -m "feat: update test creatures for SDF rasterizer, add pause/step"
```

---

### Task 6: Feature-gate dev tooling and add PNG output

**Files:**
- Modify: `Cargo.toml` (add feature + optional dep)
- Modify: `src/lib.rs` (feature-gate test_creatures module)
- Modify: `src/main.rs` (feature-gate CLI args, add --render-creature)

- [ ] **Step 1: Update Cargo.toml**

Add to `Cargo.toml`:

```toml
[features]
default = []
dev-creature = ["dep:png"]
```

And in `[dependencies]`, add:

```toml
png = { version = "0.17", optional = true }
```

- [ ] **Step 2: Feature-gate test_creatures in lib.rs**

In `src/lib.rs`, change:

```rust
// Old:
pub mod test_creatures;
// New:
#[cfg(feature = "dev-creature")]
pub mod test_creatures;
```

- [ ] **Step 3: Feature-gate --test-creatures in main.rs**

In `src/main.rs`, wrap the test-creatures check:

```rust
#[cfg(feature = "dev-creature")]
if args.iter().any(|a| a == "--test-creatures") {
    let mut terminal = ratatui::init();
    let result = summoner::test_creatures::run(&mut terminal);
    ratatui::restore();
    return result;
}
```

- [ ] **Step 4: Add --render-creature command to main.rs**

Add this function and the CLI handling. Add at the top of `main.rs`:

```rust
#[cfg(feature = "dev-creature")]
mod render_creature {
    use anyhow::Result;
    use std::fs::File;
    use std::io::BufWriter;

    use summoner::creature::generate::CellKind;
    use summoner::creature::locomotion::LocomotionState;
    use summoner::creature::outline::{rasterize_skeleton_scaled, rasterize_skeleton_wireframe, RasterResult};
    use summoner::creature::render::{state_palette, Palette};
    use summoner::creature::skeleton::{Skeleton, archetype_index};
    use summoner::session::SessionState;

    fn parse_state(s: &str) -> SessionState {
        match s {
            "working" => SessionState::Working,
            "waiting" => SessionState::Waiting,
            "idle" => SessionState::Idle,
            "sleeping" => SessionState::Sleeping,
            "disconnected" => SessionState::Disconnected,
            _ => SessionState::Idle,
        }
    }

    fn shade_color_rgb(r: u8, g: u8, b: u8, offset: i16) -> (u8, u8, u8) {
        (
            (r as i16 + offset).clamp(0, 255) as u8,
            (g as i16 + offset).clamp(0, 255) as u8,
            (b as i16 + offset).clamp(0, 255) as u8,
        )
    }

    // Use capsule_shade_offset from render module (imported above)
    use summoner::creature::render::capsule_shade_offset;

    /// Component-ID rainbow colors for limb debug mode.
    fn component_color_rgb(id: u8) -> (u8, u8, u8) {
        const COLORS: &[(u8, u8, u8)] = &[
            (255, 100, 100), // head
            (100, 255, 100), // spine 0
            (100, 100, 255), // spine 1
            (255, 255, 100), // spine 2
            (255, 100, 255), // spine 3
            (100, 255, 255), // spine 4
            (255, 180, 100), // spine 5
            (180, 100, 255), // spine 6
            (100, 200, 150), // spine 7
            (200, 200, 100), // spine 8
        ];
        const LIMB_COLORS: &[(u8, u8, u8)] = &[
            (255, 80, 80),
            (80, 200, 255),
            (255, 200, 80),
            (80, 255, 160),
            (200, 80, 255),
            (255, 160, 80),
            (80, 255, 80),
            (80, 160, 255),
        ];
        if id == 0 {
            (20, 20, 30) // background
        } else if id >= 200 {
            (180, 140, 255) // wing membrane — light purple
        } else if id >= 100 {
            let li = (id - 100) as usize;
            LIMB_COLORS[li % LIMB_COLORS.len()]
        } else {
            let i = (id as usize).saturating_sub(1);
            COLORS[i % COLORS.len()]
        }
    }

    fn palette_to_rgb(palette: &Palette) -> ((u8, u8, u8), (u8, u8, u8)) {
        let body = match palette.body {
            ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
            _ => (100, 100, 100),
        };
        let border = match palette.border {
            ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
            _ => (60, 60, 60),
        };
        (body, border)
    }

    pub fn run(args: &[String]) -> Result<()> {
        let get_arg = |flag: &str| -> Option<String> {
            args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
        };

        let archetype_name = get_arg("--archetype").unwrap_or_else(|| "bipedal".into());
        let archetype = archetype_index(&archetype_name);
        let state_name = get_arg("--state").unwrap_or_else(|| "rest".into());
        let seed: u64 = get_arg("--seed").and_then(|s| s.parse().ok()).unwrap_or(42);
        let phase: f32 = get_arg("--phase").and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let color_mode = get_arg("--color").unwrap_or_else(|| "shaded".into());
        let scale: usize = get_arg("--scale").and_then(|s| s.parse().ok()).unwrap_or(16);
        let output = get_arg("-o").unwrap_or_else(|| "creature.png".into());

        let mut skel = Skeleton::instantiate(archetype, seed);

        // Simulate animation if phase > 0 and not rest
        if phase > 0.0 && state_name != "rest" {
            let state = parse_state(&state_name);
            let mut loco = LocomotionState::new(skel, state);
            let ticks = (phase * 60.0).round() as u32; // ~60 ticks per cycle
            for _ in 0..ticks {
                loco.tick(std::time::Duration::from_millis(33));
            }
            skel = loco.skeleton().clone();
        }

        let raster = if color_mode == "wireframe" {
            rasterize_skeleton_wireframe(&skel, scale)
        } else {
            rasterize_skeleton_scaled(&skel, scale)
        };

        let img_w = raster.sprite.width;
        let img_h = raster.sprite.height;

        // Build RGB pixel buffer
        let mut pixels = vec![0u8; img_w * img_h * 3];
        let bg = (20u8, 20u8, 30u8);

        let palette = if state_name == "rest" {
            state_palette(SessionState::Idle)
        } else {
            state_palette(parse_state(&state_name))
        };
        let (body_rgb, border_rgb) = palette_to_rgb(&palette);

        for y in 0..img_h {
            for x in 0..img_w {
                let idx = y * img_w + x;
                let cell = raster.sprite.get(x, y);
                let cap_id = raster.capsule_ids[idx];

                let (r, g, b) = match &color_mode[..] {
                    "shaded" => match cell {
                        CellKind::Body => shade_color_rgb(body_rgb.0, body_rgb.1, body_rgb.2, capsule_shade_offset(cap_id)),
                        CellKind::Border => border_rgb,
                        CellKind::Empty => bg,
                    },
                    "limb" | "wireframe" => match cell {
                        CellKind::Body | CellKind::Border => component_color_rgb(cap_id),
                        CellKind::Empty => bg,
                    },
                    _ => bg,
                };

                let pi = idx * 3;
                pixels[pi] = r;
                pixels[pi + 1] = g;
                pixels[pi + 2] = b;
            }
        }

        // Write PNG
        let file = File::create(&output)?;
        let w = BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, img_w as u32, img_h as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&pixels)?;

        eprintln!("Wrote {}x{} PNG to {}", img_w, img_h, output);
        Ok(())
    }
}
```

Add the CLI handling in `main()`, before the TUI init:

```rust
#[cfg(feature = "dev-creature")]
if args.iter().any(|a| a == "--render-creature") {
    return render_creature::run(&args);
}
```

- [ ] **Step 5: Verify feature-gated builds**

Run both:
```bash
cargo check                          # should compile without dev-creature features
cargo check --features dev-creature  # should compile with PNG support
```

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/lib.rs src/main.rs
git commit -m "feat: feature-gate dev tooling, add --render-creature PNG output"
```

---

### Task 7: Write tests for the SDF rasterizer

**Files:**
- Create: `tests/outline_test.rs`

- [ ] **Step 1: Write SDF function tests**

Create `tests/outline_test.rs`:

```rust
use summoner::creature::generate::CellKind;
use summoner::creature::outline::{rasterize_skeleton, rasterize_skeleton_scaled, RasterResult};
use summoner::creature::skeleton::{Skeleton, ARCHETYPE_COUNT};

#[test]
fn sdf_rasterize_produces_nonempty_sprite() {
    for archetype in 0..ARCHETYPE_COUNT {
        let skel = Skeleton::instantiate(archetype, 42);
        let raster = rasterize_skeleton(&skel);
        let filled = raster.sprite.cells.iter().filter(|c| **c != CellKind::Empty).count();
        assert!(
            filled > 0,
            "Archetype {} produced empty sprite",
            archetype
        );
    }
}

#[test]
fn sdf_rasterize_has_body_and_border() {
    for archetype in 0..ARCHETYPE_COUNT {
        let skel = Skeleton::instantiate(archetype, 42);
        let raster = rasterize_skeleton(&skel);
        let has_body = raster.sprite.cells.iter().any(|c| *c == CellKind::Body);
        let has_border = raster.sprite.cells.iter().any(|c| *c == CellKind::Border);
        assert!(has_body, "Archetype {} has no Body cells", archetype);
        assert!(has_border, "Archetype {} has no Border cells", archetype);
    }
}

#[test]
fn capsule_ids_match_sprite_cells() {
    let skel = Skeleton::instantiate(0, 42); // bipedal
    let raster = rasterize_skeleton(&skel);
    for (i, cell) in raster.sprite.cells.iter().enumerate() {
        match cell {
            CellKind::Body | CellKind::Border => {
                assert!(raster.capsule_ids[i] > 0, "Filled cell at index {} has capsule_id 0", i);
            }
            CellKind::Empty => {
                // Empty cells may have id 0 or a nearby capsule id from border detection
            }
        }
    }
}

#[test]
fn bipedal_legs_are_separated() {
    let skel = Skeleton::instantiate(0, 42); // bipedal
    let raster = rasterize_skeleton(&skel);
    // Check the bottom third of the sprite for a gap between legs
    let w = raster.sprite.width;
    let h = raster.sprite.height;
    let bottom_start = h * 2 / 3;
    let mut has_empty_between_filled = false;
    for y in bottom_start..h {
        let mut first_filled = None;
        let mut last_filled = None;
        for x in 0..w {
            if raster.sprite.get(x, y) != CellKind::Empty {
                if first_filled.is_none() { first_filled = Some(x); }
                last_filled = Some(x);
            }
        }
        if let (Some(first), Some(last)) = (first_filled, last_filled) {
            for x in first..last {
                if raster.sprite.get(x, y) == CellKind::Empty {
                    has_empty_between_filled = true;
                    break;
                }
            }
        }
        if has_empty_between_filled { break; }
    }
    assert!(has_empty_between_filled, "Bipedal legs should have visible gap between them");
}

#[test]
fn scaled_rasterize_has_larger_dimensions() {
    let skel = Skeleton::instantiate(0, 42);
    let r1 = rasterize_skeleton(&skel);
    let r2 = rasterize_skeleton_scaled(&skel, 4);
    assert_eq!(r1.sprite.width, 18);
    assert_eq!(r1.sprite.height, 24);
    assert_eq!(r2.sprite.width, 72);
    assert_eq!(r2.sprite.height, 96);
}

#[test]
fn same_seed_produces_same_raster() {
    let s1 = Skeleton::instantiate(0, 12345);
    let s2 = Skeleton::instantiate(0, 12345);
    let r1 = rasterize_skeleton(&s1);
    let r2 = rasterize_skeleton(&s2);
    assert_eq!(r1.sprite.cells, r2.sprite.cells);
    assert_eq!(r1.capsule_ids, r2.capsule_ids);
}

#[test]
fn different_seeds_produce_different_rasters() {
    let s1 = Skeleton::instantiate(0, 100);
    let s2 = Skeleton::instantiate(0, 200);
    let r1 = rasterize_skeleton(&s1);
    let r2 = rasterize_skeleton(&s2);
    assert_ne!(r1.sprite.cells, r2.sprite.cells);
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --test outline_test`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/outline_test.rs
git commit -m "test: add SDF rasterizer tests"
```

---

### Task 8: Update existing tests for new API

**Files:**
- Modify: `tests/creature_test.rs` (update imports, remove references to old mask-based API if they fail)

- [ ] **Step 1: Check which existing tests pass**

Run: `cargo test`
Expected: Some tests in `creature_test.rs` may fail because they reference `generate_sprite`, `templates`, `animate`, which may have changed in the expressive-creatures branch. Fix any that fail by updating to use the new skeleton-based API.

- [ ] **Step 2: Update or remove broken tests**

Tests referencing `generate_sprite`, `get_template`, `AnimationState`, `animate_sprite` should be updated if those functions still exist, or removed if they've been replaced by the skeleton system.

The `render_sprite_fills_buffer_cells` test should still work since `render_sprite_to_buffer` still exists.

The Xorshift tests should be unchanged.

- [ ] **Step 3: Verify all tests pass**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/creature_test.rs
git commit -m "test: update creature tests for new skeleton-based API"
```

---

### Task 9: End-to-end verification with PNG output

**Files:** None (verification only)

- [ ] **Step 1: Build with dev-creature feature**

Run: `cargo build --features dev-creature`
Expected: Clean build.

- [ ] **Step 2: Render a bipedal creature in each color mode**

Run:
```bash
cargo run --features dev-creature -- --render-creature --archetype bipedal --seed 42 --color shaded -o /tmp/bipedal-shaded.png
cargo run --features dev-creature -- --render-creature --archetype bipedal --seed 42 --color limb -o /tmp/bipedal-limb.png
cargo run --features dev-creature -- --render-creature --archetype bipedal --seed 42 --color wireframe -o /tmp/bipedal-wireframe.png
```

Expected: Three PNG files created. The shaded version should show clear limb separation with color variation between body parts.

- [ ] **Step 3: Render all 5 archetypes**

Run:
```bash
for arch in bipedal quadruped blob winged serpentine; do
    cargo run --features dev-creature -- --render-creature --archetype $arch --seed 42 -o /tmp/$arch.png
done
```

Expected: Five PNG files, each showing a recognizable creature with distinct limbs and shading.

- [ ] **Step 4: Verify the interactive TUI**

Run: `cargo run --features dev-creature -- --test-creatures`

Verify:
- Grid mode shows all archetypes with shaded colors
- Zoom mode works with arrow keys, `c` cycles shaded→limb→wireframe
- Space pauses, `.` steps forward, `,` steps backward
- `r` randomizes, `z` returns to grid

- [ ] **Step 5: Verify release build excludes dev tooling**

Run:
```bash
cargo build --release
# Verify --test-creatures and --render-creature flags are not recognized
```

- [ ] **Step 6: Commit any fixes needed**

If any adjustments were needed during verification, commit them.

---

### Task 10: Visual review of creature outlines

This is an iterative task — use the `--render-creature` CLI to visually verify each archetype looks correct and adjust parameters if needed.

- [ ] **Step 1: Review bipedal creature**

Render and examine: are legs separated? Arms visible and distinct from torso? Head stands out? Shading gradient readable?

If issues: adjust capsule widths in `build_bipedal`, shade offsets in `capsule_shade_offset`, or border width constant.

- [ ] **Step 2: Review quadruped creature**

Check: 4 legs clearly separated? Tail tapers? Body reads as horizontal?

- [ ] **Step 3: Review blob creature**

Check: rounded shape? No spurious gaps? Single flat shade?

- [ ] **Step 4: Review winged creature**

Check: wing membrane visible? Body/legs distinct? Wings don't overpower the body?

- [ ] **Step 5: Review serpentine creature**

Check: body tapers head→tail? Shade gradient readable? Snake-like silhouette?

- [ ] **Step 6: Review animated states**

```bash
for state in working waiting idle sleeping disconnected; do
    cargo run --features dev-creature -- --render-creature --archetype bipedal --state $state --phase 0.5 -o /tmp/bipedal-$state.png
done
```

Check: do animated poses still produce clean outlines? No artifacts from extreme skeleton positions?

- [ ] **Step 7: Commit any parameter adjustments**

```bash
git add -A
git commit -m "polish: tune creature outline parameters after visual review"
```
