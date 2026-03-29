use summoner::creature::generate::{CellKind, Xorshift};
use summoner::creature::skeleton::Vec2;
use summoner::creature::skeleton::{Skeleton, ARCHETYPE_COUNT, archetype_name, archetype_index};

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

#[test]
fn skeleton_instantiate_bipedal_has_correct_topology() {
    let skel = Skeleton::instantiate(0, 42);
    assert!(skel.points.len() >= 6, "Bipedal should have at least 6 chain points, got {}", skel.points.len());
    assert_eq!(skel.limbs.len(), 4, "Bipedal should have 4 limbs");
    assert!(skel.constraints.len() >= 5, "Bipedal should have at least 5 constraints");
}

#[test]
fn skeleton_instantiate_quadruped_has_correct_topology() {
    let skel = Skeleton::instantiate(1, 42);
    assert!(skel.points.len() >= 7);
    assert_eq!(skel.limbs.len(), 4);
}

#[test]
fn skeleton_instantiate_blob_has_no_limbs() {
    let skel = Skeleton::instantiate(2, 42);
    assert!(skel.points.len() >= 6);
    assert_eq!(skel.limbs.len(), 0);
}

#[test]
fn skeleton_instantiate_winged_has_wings_and_legs() {
    let skel = Skeleton::instantiate(3, 42);
    assert!(skel.points.len() >= 9);
    assert_eq!(skel.limbs.len(), 2);
}

#[test]
fn skeleton_instantiate_serpentine_has_long_chain() {
    let skel = Skeleton::instantiate(4, 42);
    assert!(skel.points.len() >= 6);
    assert_eq!(skel.limbs.len(), 0);
}

#[test]
fn different_seeds_produce_different_skeletons() {
    let s1 = Skeleton::instantiate(0, 100);
    let s2 = Skeleton::instantiate(0, 200);
    assert_eq!(s1.points.len(), s2.points.len());
    assert_eq!(s1.limbs.len(), s2.limbs.len());
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

use summoner::creature::physics::{verlet_integrate, apply_constraints, solve_two_bone_ik};

#[test]
fn verlet_integration_moves_points() {
    let mut skel = Skeleton::instantiate(0, 42);
    let head_before = skel.points[0].pos;
    skel.points[0].prev_pos = skel.points[0].pos - Vec2::new(1.0, 0.0);
    verlet_integrate(&mut skel, 0.98, Vec2::new(0.0, 0.5));
    let head_after = skel.points[0].pos;
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
    skel.points[0].pos = Vec2::new(0.0, 0.0);
    apply_constraints(&mut skel, 3);
    let dist = (skel.points[0].pos - skel.points[1].pos).length();
    let rest = skel.constraints[0].rest_length;
    assert!((dist - rest).abs() < 0.5, "Constraint not satisfied: dist={dist}, rest={rest}");
}

#[test]
fn two_bone_ik_reaches_target() {
    let anchor = Vec2::new(9.0, 10.0);
    let target = Vec2::new(9.0, 16.0);
    let result = solve_two_bone_ik(anchor, target, 3.0, 3.0);
    let end_dist = (result.end - target).length();
    assert!(end_dist < 0.5, "IK end not near target");
    let upper_dist = (result.mid - anchor).length();
    let lower_dist = (result.end - result.mid).length();
    assert!((upper_dist - 3.0).abs() < 0.5);
    assert!((lower_dist - 3.0).abs() < 0.5);
}

#[test]
fn two_bone_ik_clamps_when_target_unreachable() {
    let anchor = Vec2::new(9.0, 10.0);
    let target = Vec2::new(9.0, 25.0);
    let result = solve_two_bone_ik(anchor, target, 3.0, 3.0);
    let total = (result.end - anchor).length();
    assert!((total - 6.0).abs() < 0.5, "Should fully extend: total={total}");
}

use summoner::creature::outline::rasterize_skeleton;

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

use summoner::creature::locomotion::LocomotionState;
use summoner::session::SessionState;
use std::time::Duration;
use summoner::creature::render::{render_sprite_to_buffer, state_palette, sprite_cell_size};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

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
    for _ in 0..10 {
        loco.tick(Duration::from_millis(33));
    }
    let after = loco.skeleton().points[0].pos;
    let moved = (after.x - before.x).abs() > 0.1 || (after.y - before.y).abs() > 0.1;
    assert!(moved, "Working locomotion should move the head: before={:?} after={:?}", before, after);
}

#[test]
fn locomotion_sleeping_is_static() {
    let skel = Skeleton::instantiate(0, 42);
    let mut loco = LocomotionState::new(skel, SessionState::Sleeping);
    for _ in 0..20 {
        loco.tick(Duration::from_millis(33));
    }
    let before = loco.skeleton().points[0].pos;
    for _ in 0..10 {
        loco.tick(Duration::from_millis(33));
    }
    let after = loco.skeleton().points[0].pos;
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
    for _ in 0..15 {
        loco.tick(Duration::from_millis(33));
    }
    let before: Vec<Vec2> = loco.skeleton().points.iter().map(|p| p.pos).collect();
    for _ in 0..10 {
        loco.tick(Duration::from_millis(33));
    }
    let after: Vec<Vec2> = loco.skeleton().points.iter().map(|p| p.pos).collect();
    for (b, a) in before.iter().zip(after.iter()) {
        assert!((b.x - a.x).abs() < 0.01 && (b.y - a.y).abs() < 0.01,
            "Disconnected should freeze after settling");
    }
}

#[test]
fn full_pipeline_skeleton_to_rendered_buffer() {
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
    assert_ne!(sprite1.cells, sprite2.cells, "Working animation should change sprite over time");
}
