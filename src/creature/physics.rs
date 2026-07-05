use super::skeleton::{Skeleton, Vec2};

pub struct IkResult {
    pub mid: Vec2,
    pub end: Vec2,
}

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

pub fn solve_two_bone_ik(anchor: Vec2, target: Vec2, upper_len: f32, lower_len: f32) -> IkResult {
    solve_two_bone_ik_dir(anchor, target, upper_len, lower_len, 1.0)
}

/// Two-bone IK with an explicit bend side. With y-down and the target below
/// the anchor, `bend_dir = 1.0` puts the joint toward +x, `-1.0` toward -x.
pub fn solve_two_bone_ik_dir(
    anchor: Vec2,
    target: Vec2,
    upper_len: f32,
    lower_len: f32,
    bend_dir: f32,
) -> IkResult {
    let to_target = target - anchor;
    let dist = to_target.length();
    let max_reach = upper_len + lower_len;

    if dist >= max_reach || dist < 1e-10 {
        let dir = if dist < 1e-10 { Vec2::new(0.0, 1.0) } else { to_target.normalized() };
        return IkResult {
            mid: anchor + dir * upper_len,
            end: anchor + dir * max_reach.min(dist),
        };
    }

    let cos_angle = (upper_len * upper_len + dist * dist - lower_len * lower_len)
        / (2.0 * upper_len * dist);
    let angle = cos_angle.clamp(-1.0, 1.0).acos();
    let base_angle = to_target.y.atan2(to_target.x);
    let mid_angle = base_angle - angle * bend_dir;

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
