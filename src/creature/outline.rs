use super::generate::{CellKind, Sprite};
use super::physics::solve_two_bone_ik_dir;
use super::skeleton::{Skeleton, Vec2};

const BASE_W: usize = 18;
const BASE_H: usize = 24;
const BORDER_WIDTH: f32 = 1.0;

/// Result of rasterizing a skeleton: the sprite plus a per-pixel capsule ID map.
///
/// Capsule ID scheme:
/// - 0 = empty / no capsule
/// - 1 = head
/// - 2..=N+1 = spine segment N (from skeleton.constraints)
/// - 100+i*2 = limb i upper bone
/// - 100+i*2+1 = limb i lower bone
/// - 200+i = wing membrane triangle i
#[derive(Debug, Clone)]
pub struct RasterResult {
    pub sprite: Sprite,
    pub capsule_ids: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Capsule representation
// ---------------------------------------------------------------------------

struct Capsule {
    a: Vec2,
    b: Vec2,
    r_a: f32,
    r_b: f32,
    id: u8,
}

struct MembraneTriangle {
    v0: Vec2,
    v1: Vec2,
    v2: Vec2,
    id: u8,
}

// ---------------------------------------------------------------------------
// SDF helpers
// ---------------------------------------------------------------------------

/// Signed distance from point `p` to a tapered capsule defined by endpoints
/// `a`, `b` with radii `r_a` and `r_b`.
fn sdf_tapered_capsule(p: Vec2, a: Vec2, b: Vec2, r_a: f32, r_b: f32) -> f32 {
    let ab = b - a;
    let len_sq = ab.dot(ab);

    // Degenerate case: zero-length segment → circle at `a`
    if len_sq < 1e-10 {
        let dist = (p - a).length();
        return dist - r_a;
    }

    let ap = p - a;
    let t = f32::clamp(ap.dot(ab) / len_sq, 0.0, 1.0);
    let closest = a + ab * t;
    let radius = r_a + (r_b - r_a) * t;
    (p - closest).length() - radius
}

/// Barycentric test: is point `p` inside triangle `(v0, v1, v2)`?
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
    let inv = 1.0 / denom;
    let u = (d11 * d20 - d01 * d21) * inv;
    let v = (d00 * d21 - d01 * d20) * inv;

    u >= 0.0 && v >= 0.0 && (u + v) <= 1.0
}

// ---------------------------------------------------------------------------
// Capsule collection
// ---------------------------------------------------------------------------

fn collect_capsules(skeleton: &Skeleton, scale_f: f32) -> Vec<Capsule> {
    let mut capsules = Vec::new();

    // Head: zero-length capsule at head position
    if let Some(head) = skeleton.points.first() {
        let pos = head.pos * scale_f;
        let radius = head.width * scale_f / 2.0;
        capsules.push(Capsule {
            a: pos,
            b: pos,
            r_a: radius.max(1.0),
            r_b: radius.max(1.0),
            id: 1,
        });
    }

    // Spine: each constraint becomes a capsule
    for (ci, c) in skeleton.constraints.iter().enumerate() {
        let pa = skeleton.points[c.a].pos * scale_f;
        let pb = skeleton.points[c.b].pos * scale_f;
        let ra = skeleton.points[c.a].width * scale_f / 2.0;
        let rb = skeleton.points[c.b].width * scale_f / 2.0;
        capsules.push(Capsule {
            a: pa,
            b: pb,
            r_a: ra,
            r_b: rb,
            id: 2 + ci as u8,
        });
    }

    // Limbs: solve IK, upper + lower bones
    for (li, limb) in skeleton.limbs.iter().enumerate() {
        let anchor = skeleton.points[limb.anchor].pos * scale_f;
        let target = limb.end_effector * scale_f;
        let ik = solve_two_bone_ik_dir(
            anchor,
            target,
            limb.upper_len * scale_f,
            limb.lower_len * scale_f,
            limb.bend_dir,
        );

        let upper_r = limb.upper_width * scale_f / 2.0;
        let lower_r = limb.lower_width * scale_f / 2.0;

        // Upper bone: anchor → mid
        capsules.push(Capsule {
            a: anchor,
            b: ik.mid,
            r_a: upper_r,
            r_b: upper_r,
            id: 100 + li as u8 * 2,
        });

        // Lower bone: mid → end
        capsules.push(Capsule {
            a: ik.mid,
            b: ik.end,
            r_a: lower_r,
            r_b: lower_r,
            id: 100 + li as u8 * 2 + 1,
        });
    }

    capsules
}

/// Collect wing membrane triangles for the winged archetype.
/// Finds "body" point, collects left/right wing points, builds triangles.
fn collect_wing_membranes(skeleton: &Skeleton, scale_f: f32) -> Vec<MembraneTriangle> {
    let mut membranes = Vec::new();

    // Find body anchor point
    let body_idx = skeleton
        .points
        .iter()
        .position(|p| p.name == "body");
    let body_pos = match body_idx {
        Some(idx) => skeleton.points[idx].pos * scale_f,
        None => return membranes,
    };

    // Collect left and right wing points (sorted by name for consistent ordering)
    let mut left_wings: Vec<(usize, Vec2)> = skeleton
        .points
        .iter()
        .enumerate()
        .filter(|(_, p)| p.name.starts_with("lwing"))
        .map(|(i, p)| (i, p.pos * scale_f))
        .collect();
    let mut right_wings: Vec<(usize, Vec2)> = skeleton
        .points
        .iter()
        .enumerate()
        .filter(|(_, p)| p.name.starts_with("rwing"))
        .map(|(i, p)| (i, p.pos * scale_f))
        .collect();

    // Sort by index to preserve chain order
    left_wings.sort_by_key(|(i, _)| *i);
    right_wings.sort_by_key(|(i, _)| *i);

    let mut tri_index: u8 = 0;

    // Left wing triangles
    for pair in left_wings.windows(2) {
        membranes.push(MembraneTriangle {
            v0: body_pos,
            v1: pair[0].1,
            v2: pair[1].1,
            id: 200 + tri_index,
        });
        tri_index += 1;
    }

    // Right wing triangles
    for pair in right_wings.windows(2) {
        membranes.push(MembraneTriangle {
            v0: body_pos,
            v1: pair[0].1,
            v2: pair[1].1,
            id: 200 + tri_index,
        });
        tri_index += 1;
    }

    membranes
}

// ---------------------------------------------------------------------------
// SDF rasterization
// ---------------------------------------------------------------------------

/// Rasterize at 1x (18x24).
pub fn rasterize_skeleton(skeleton: &Skeleton) -> RasterResult {
    rasterize_skeleton_scaled(skeleton, 1)
}

/// Rasterize at Nx scale using SDF capsule evaluation.
pub fn rasterize_skeleton_scaled(skeleton: &Skeleton, scale: usize) -> RasterResult {
    let scale_f = scale as f32;
    let w = BASE_W * scale;
    let h = BASE_H * scale;

    let mut cells = vec![CellKind::Empty; w * h];
    let mut ids = vec![0u8; w * h];

    let capsules = collect_capsules(skeleton, scale_f);
    let membranes = collect_wing_membranes(skeleton, scale_f);

    // Phase 1: SDF capsule evaluation
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

    // Phase 2: Wing membrane fill — only fill Empty pixels
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if cells[idx] != CellKind::Empty {
                continue;
            }
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            for tri in &membranes {
                if point_in_triangle(p, tri.v0, tri.v1, tri.v2) {
                    cells[idx] = CellKind::Body;
                    ids[idx] = tri.id;
                    break;
                }
            }
        }
    }

    // Phase 3: Ring interior fill (blob archetype)
    if is_ring_topology(skeleton) {
        ring_interior_fill(&mut cells, &mut ids, w, h);
        // Convert interior Border pixels to Body (they're no longer on the edge)
        demote_interior_borders(&mut cells, w, h);
    }

    // Phase 4: Edge detection — Body pixels adjacent to Empty become Border
    edge_detect(&mut cells, w, h);

    RasterResult {
        sprite: Sprite {
            width: w,
            height: h,
            cells,
        },
        capsule_ids: ids,
    }
}

// ---------------------------------------------------------------------------
// Wireframe rasterizer (Bresenham lines, no fill)
// ---------------------------------------------------------------------------

/// Debug wireframe: draws skeleton lines, joint dots, end-effector dots.
/// No SDF fill, no edge detection.
pub fn rasterize_skeleton_wireframe(skeleton: &Skeleton, scale: usize) -> RasterResult {
    let scale_f = scale as f32;
    let w = BASE_W * scale;
    let h = BASE_H * scale;
    let mut cells = vec![CellKind::Empty; w * h];
    let mut ids = vec![0u8; w * h];

    let scaled_points: Vec<(Vec2, f32)> = skeleton
        .points
        .iter()
        .map(|pt| (pt.pos * scale_f, pt.width * scale_f))
        .collect();

    // Draw spine constraint lines
    for (ci, c) in skeleton.constraints.iter().enumerate() {
        let id = 2 + ci as u8;
        draw_line_tagged(
            scaled_points[c.a].0,
            scaled_points[c.b].0,
            &mut cells,
            &mut ids,
            id,
            w,
            h,
        );
    }

    // Draw limbs via IK
    for (li, limb) in skeleton.limbs.iter().enumerate() {
        let anchor = skeleton.points[limb.anchor].pos * scale_f;
        let target = limb.end_effector * scale_f;
        let ik = solve_two_bone_ik_dir(
            anchor,
            target,
            limb.upper_len * scale_f,
            limb.lower_len * scale_f,
            limb.bend_dir,
        );
        let upper_id = 100 + li as u8 * 2;
        let lower_id = 100 + li as u8 * 2 + 1;
        draw_line_tagged(anchor, ik.mid, &mut cells, &mut ids, upper_id, w, h);
        draw_line_tagged(ik.mid, ik.end, &mut cells, &mut ids, lower_id, w, h);
    }

    // Draw joint dots at each chain point
    for (i, (pos, _)) in scaled_points.iter().enumerate() {
        let id = if i == 0 { 1 } else { 2 + i as u8 };
        let r = if i == 0 {
            2.0 * scale_f / 2.0
        } else {
            1.0
        };
        draw_filled_circle_tagged(*pos, r.max(1.0), &mut cells, &mut ids, id, w, h);
    }

    // Draw end-effector dots for limbs
    for (li, limb) in skeleton.limbs.iter().enumerate() {
        let target = limb.end_effector * scale_f;
        let id = 100 + li as u8 * 2 + 1;
        draw_filled_circle_tagged(target, 1.0, &mut cells, &mut ids, id, w, h);
    }

    RasterResult {
        sprite: Sprite {
            width: w,
            height: h,
            cells,
        },
        capsule_ids: ids,
    }
}

// ---------------------------------------------------------------------------
// Ring interior fill (for blob archetype)
// ---------------------------------------------------------------------------

/// Detect if the skeleton is a ring topology (no limbs, last constraint wraps).
fn is_ring_topology(skeleton: &Skeleton) -> bool {
    skeleton.limbs.is_empty()
        && skeleton.constraints.iter().any(|c| {
            let n = skeleton.points.len();
            (c.a == n - 1 && c.b == 0) || (c.a == 0 && c.b == n - 1)
        })
}

/// Scanline fill the interior of a ring of capsules.
/// For each row, find leftmost and rightmost filled pixels (Body or Border),
/// fill Empty pixels between them as Body. Inherit capsule ID from nearest border.
fn ring_interior_fill(cells: &mut [CellKind], ids: &mut [u8], w: usize, h: usize) {
    for y in 0..h {
        let mut left = None;
        let mut right = None;
        let mut left_id = 0u8;
        let mut right_id = 0u8;
        for x in 0..w {
            let idx = y * w + x;
            if cells[idx] != CellKind::Empty {
                if left.is_none() {
                    left = Some(x);
                    left_id = ids[idx];
                }
                right = Some(x);
                right_id = ids[idx];
            }
        }
        if let (Some(l), Some(r)) = (left, right)
            && r > l + 1 {
                let mid = (l + r) / 2;
                for x in (l + 1)..r {
                    let idx = y * w + x;
                    if cells[idx] == CellKind::Empty {
                        cells[idx] = CellKind::Body;
                        ids[idx] = if x <= mid { left_id } else { right_id };
                    }
                }
            }
    }
}

/// Convert Border pixels that are completely surrounded (no adjacent Empty) to Body.
/// Used after ring fill to remove internal outlines.
fn demote_interior_borders(cells: &mut [CellKind], w: usize, h: usize) {
    let snapshot = cells.to_vec();
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if snapshot[idx] == CellKind::Border {
                let has_empty =
                    [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)]
                        .iter()
                        .any(|(dx, dy)| {
                            let nx = x as i32 + dx;
                            let ny = y as i32 + dy;
                            if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                                snapshot[ny as usize * w + nx as usize] == CellKind::Empty
                            } else {
                                false
                            }
                        });
                if !has_empty {
                    cells[idx] = CellKind::Body;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Edge detection
// ---------------------------------------------------------------------------

fn edge_detect(cells: &mut [CellKind], w: usize, h: usize) {
    let snapshot = cells.to_vec();
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if snapshot[idx] == CellKind::Body {
                let has_empty =
                    [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)]
                        .iter()
                        .any(|(dx, dy)| {
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
}

// ---------------------------------------------------------------------------
// Bresenham helpers (for wireframe mode)
// ---------------------------------------------------------------------------

fn draw_line_tagged(
    from: Vec2,
    to: Vec2,
    cells: &mut [CellKind],
    comp: &mut [u8],
    id: u8,
    w: usize,
    h: usize,
) {
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
        set_cell_tagged(x, y, id, cells, comp, w, h);
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

fn draw_filled_circle_tagged(
    center: Vec2,
    radius: f32,
    cells: &mut [CellKind],
    comp: &mut [u8],
    id: u8,
    w: usize,
    h: usize,
) {
    let cx = center.x.round() as i32;
    let cy = center.y.round() as i32;
    let r = radius.round() as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                set_cell_tagged(cx + dx, cy + dy, id, cells, comp, w, h);
            }
        }
    }
}

/// Stamp a border cell (all tagged drawing is border-only) with its capsule id.
fn set_cell_tagged(
    x: i32,
    y: i32,
    id: u8,
    cells: &mut [CellKind],
    comp: &mut [u8],
    w: usize,
    h: usize,
) {
    if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
        let idx = y as usize * w + x as usize;
        cells[idx] = CellKind::Border;
        comp[idx] = id;
    }
}
