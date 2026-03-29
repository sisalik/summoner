use super::generate::{CellKind, Sprite};
use super::physics::solve_two_bone_ik;
use super::skeleton::{Skeleton, Vec2};

const SPRITE_W: usize = 18;
const SPRITE_H: usize = 24;

pub fn rasterize_skeleton(skeleton: &Skeleton) -> Sprite {
    let mut cells = vec![CellKind::Empty; SPRITE_W * SPRITE_H];
    draw_body_outline(skeleton, &mut cells);
    for limb in &skeleton.limbs {
        let anchor = skeleton.points[limb.anchor].pos;
        let ik = solve_two_bone_ik(anchor, limb.end_effector, limb.upper_len, limb.lower_len);
        draw_line(anchor, ik.mid, &mut cells);
        draw_line(ik.mid, ik.end, &mut cells);
    }
    if let Some(head) = skeleton.points.first() {
        let radius = (head.width / 2.0).max(1.0);
        draw_filled_circle(head.pos, radius, &mut cells);
    }
    scanline_fill(&mut cells);
    edge_detect(&mut cells);
    Sprite { width: SPRITE_W, height: SPRITE_H, cells }
}

fn draw_body_outline(skeleton: &Skeleton, cells: &mut [CellKind]) {
    let spine_points = &skeleton.points;
    if spine_points.len() < 2 { return; }

    let is_ring = skeleton.limbs.is_empty() && skeleton.constraints.iter().any(|c| {
        (c.a == spine_points.len() - 1 && c.b == 0) || (c.a == 0 && c.b == spine_points.len() - 1)
    });

    if is_ring {
        for i in 0..spine_points.len() {
            let next = (i + 1) % spine_points.len();
            draw_line(spine_points[i].pos, spine_points[next].pos, cells);
        }
        return;
    }

    let mut left_boundary = Vec::new();
    let mut right_boundary = Vec::new();
    for i in 0..spine_points.len() {
        let pt = &spine_points[i];
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
    for i in 0..left_boundary.len() - 1 {
        draw_line(left_boundary[i], left_boundary[i + 1], cells);
    }
    for i in 0..right_boundary.len() - 1 {
        draw_line(right_boundary[i], right_boundary[i + 1], cells);
    }
    draw_line(left_boundary[0], right_boundary[0], cells);
    if let (Some(l), Some(r)) = (left_boundary.last(), right_boundary.last()) {
        draw_line(*l, *r, cells);
    }
}

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
        if x == x1 && y == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x += sx; }
        if e2 <= dx { err += dx; y += sy; }
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

fn scanline_fill(cells: &mut [CellKind]) {
    for y in 0..SPRITE_H {
        let mut left = None;
        let mut right = None;
        for x in 0..SPRITE_W {
            if cells[y * SPRITE_W + x] == CellKind::Border {
                if left.is_none() { left = Some(x); }
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
                        true
                    }
                });
                if has_empty { cells[idx] = CellKind::Border; }
            }
        }
    }
}

fn set_cell(x: i32, y: i32, kind: CellKind, cells: &mut [CellKind]) {
    if x >= 0 && x < SPRITE_W as i32 && y >= 0 && y < SPRITE_H as i32 {
        cells[y as usize * SPRITE_W + x as usize] = kind;
    }
}
