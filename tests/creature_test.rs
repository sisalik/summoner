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
