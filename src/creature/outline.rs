use super::generate::{CellKind, Sprite};
use super::physics::solve_two_bone_ik;
use super::skeleton::{Skeleton, Vec2};

const BASE_W: usize = 18;
const BASE_H: usize = 24;

/// Rasterize a skeleton into a Sprite at 1x resolution (18x24 sub-pixel grid).
pub fn rasterize_skeleton(skeleton: &Skeleton) -> Sprite {
    rasterize_skeleton_scaled(skeleton, 1)
}

/// Rasterize a skeleton at `scale`x resolution.
/// The output sprite is (18*scale) x (24*scale) sub-pixels.
/// All skeleton positions and widths are scaled up before rasterization,
/// producing genuinely higher-detail output.
pub fn rasterize_skeleton_scaled(skeleton: &Skeleton, scale: usize) -> Sprite {
    let scale_f = scale as f32;
    let w = BASE_W * scale;
    let h = BASE_H * scale;
    let mut cells = vec![CellKind::Empty; w * h];

    // Scale all positions and widths for rasterization
    let scaled_points: Vec<(Vec2, f32)> = skeleton.points.iter()
        .map(|pt| (pt.pos * scale_f, pt.width * scale_f))
        .collect();

    // Draw body outline
    draw_body_outline(skeleton, &scaled_points, &mut cells, w, h);

    // Draw limbs via IK (scaled)
    for limb in &skeleton.limbs {
        let anchor = skeleton.points[limb.anchor].pos * scale_f;
        let target = limb.end_effector * scale_f;
        let ik = solve_two_bone_ik(anchor, target, limb.upper_len * scale_f, limb.lower_len * scale_f);
        draw_line(anchor, ik.mid, &mut cells, w, h);
        draw_line(ik.mid, ik.end, &mut cells, w, h);
    }

    // Draw head circle (scaled)
    if let Some((pos, width)) = scaled_points.first() {
        let radius = (width / 2.0).max(1.0);
        draw_filled_circle(*pos, radius, &mut cells, w, h);
    }

    scanline_fill(&mut cells, w, h);
    edge_detect(&mut cells, w, h);

    Sprite { width: w, height: h, cells }
}

fn draw_body_outline(
    skeleton: &Skeleton,
    scaled_points: &[(Vec2, f32)],
    cells: &mut [CellKind],
    w: usize,
    h: usize,
) {
    if scaled_points.len() < 2 { return; }

    let is_ring = skeleton.limbs.is_empty() && skeleton.constraints.iter().any(|c| {
        let n = skeleton.points.len();
        (c.a == n - 1 && c.b == 0) || (c.a == 0 && c.b == n - 1)
    });

    if is_ring {
        for i in 0..scaled_points.len() {
            let next = (i + 1) % scaled_points.len();
            draw_line(scaled_points[i].0, scaled_points[next].0, cells, w, h);
        }
        return;
    }

    let mut left_boundary = Vec::new();
    let mut right_boundary = Vec::new();
    for i in 0..scaled_points.len() {
        let (pos, width) = scaled_points[i];
        let dir = if i == 0 {
            scaled_points[1].0 - pos
        } else if i == scaled_points.len() - 1 {
            pos - scaled_points[i - 1].0
        } else {
            scaled_points[i + 1].0 - scaled_points[i - 1].0
        };
        let perp = dir.perpendicular().normalized();
        let half_w = width / 2.0;
        left_boundary.push(pos + perp * half_w);
        right_boundary.push(pos - perp * half_w);
    }
    for i in 0..left_boundary.len() - 1 {
        draw_line(left_boundary[i], left_boundary[i + 1], cells, w, h);
    }
    for i in 0..right_boundary.len() - 1 {
        draw_line(right_boundary[i], right_boundary[i + 1], cells, w, h);
    }
    draw_line(left_boundary[0], right_boundary[0], cells, w, h);
    if let (Some(l), Some(r)) = (left_boundary.last(), right_boundary.last()) {
        draw_line(*l, *r, cells, w, h);
    }
}

fn draw_line(from: Vec2, to: Vec2, cells: &mut [CellKind], w: usize, h: usize) {
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
        set_cell(x, y, CellKind::Border, cells, w, h);
        if x == x1 && y == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x += sx; }
        if e2 <= dx { err += dx; y += sy; }
    }
}

fn draw_filled_circle(center: Vec2, radius: f32, cells: &mut [CellKind], w: usize, h: usize) {
    let cx = center.x.round() as i32;
    let cy = center.y.round() as i32;
    let r = radius.round() as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                set_cell(cx + dx, cy + dy, CellKind::Border, cells, w, h);
            }
        }
    }
}

fn scanline_fill(cells: &mut [CellKind], w: usize, h: usize) {
    for y in 0..h {
        let mut left = None;
        let mut right = None;
        for x in 0..w {
            if cells[y * w + x] == CellKind::Border {
                if left.is_none() { left = Some(x); }
                right = Some(x);
            }
        }
        if let (Some(l), Some(r)) = (left, right) {
            for x in (l + 1)..r {
                if cells[y * w + x] == CellKind::Empty {
                    cells[y * w + x] = CellKind::Body;
                }
            }
        }
    }
}

fn edge_detect(cells: &mut [CellKind], w: usize, h: usize) {
    let snapshot = cells.to_vec();
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
                if has_empty { cells[idx] = CellKind::Border; }
            }
        }
    }
}

fn set_cell(x: i32, y: i32, kind: CellKind, cells: &mut [CellKind], w: usize, h: usize) {
    if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
        cells[y as usize * w + x as usize] = kind;
    }
}
