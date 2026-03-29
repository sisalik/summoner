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
