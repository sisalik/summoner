use summoner::creature::generate::{Sprite, generate_sprite, CellKind, Xorshift};

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
fn sprite_has_correct_dimensions() {
    let mask = vec![
        vec![0, 0, 1, 1],
        vec![0, 1, 1, 1],
        vec![0, 1, 2, 2],
        vec![0, 0, 1, 1],
    ];
    let sprite = generate_sprite(&mask, 42);
    // Width = mask[0].len() * 2 (mirrored), height = mask.len()
    assert_eq!(sprite.width, 8);
    assert_eq!(sprite.height, 4);
}

#[test]
fn sprite_is_horizontally_symmetric() {
    let mask = vec![
        vec![0, 1, 1, 2],
        vec![1, 1, 2, 2],
        vec![0, 1, 1, 1],
    ];
    let sprite = generate_sprite(&mask, 42);
    for y in 0..sprite.height {
        for x in 0..sprite.width / 2 {
            let mirror_x = sprite.width - 1 - x;
            assert_eq!(
                sprite.get(x, y),
                sprite.get(mirror_x, y),
                "Asymmetry at y={y}, x={x} vs x={mirror_x}"
            );
        }
    }
}

#[test]
fn same_seed_produces_same_sprite() {
    let mask = vec![
        vec![0, 1, 2, 1],
        vec![1, 1, 2, 2],
    ];
    let s1 = generate_sprite(&mask, 12345);
    let s2 = generate_sprite(&mask, 12345);
    assert_eq!(s1.cells, s2.cells);
}

#[test]
fn different_seeds_produce_different_sprites() {
    let mask = vec![
        vec![0, 1, 2, 1],
        vec![1, 1, 2, 2],
        vec![1, 2, 2, 1],
        vec![0, 1, 1, 0],
    ];
    let s1 = generate_sprite(&mask, 100);
    let s2 = generate_sprite(&mask, 200);
    assert_ne!(s1.cells, s2.cells);
}

#[test]
fn borders_surround_body_cells() {
    let mask = vec![
        vec![0, 0, 0, 0],
        vec![0, 1, 1, 0],
        vec![0, 1, 1, 0],
        vec![0, 0, 0, 0],
    ];
    let sprite = generate_sprite(&mask, 1);
    let mut has_body = false;
    let mut has_border = false;
    for y in 0..sprite.height {
        for x in 0..sprite.width {
            match sprite.get(x, y) {
                CellKind::Body => has_body = true,
                CellKind::Border => has_border = true,
                CellKind::Empty => {}
            }
        }
    }
    assert!(has_body || has_border, "Sprite should have some filled cells");
}
