use summoner::creature::generate::CellKind;
use summoner::creature::outline::{rasterize_skeleton, rasterize_skeleton_scaled};
use summoner::creature::skeleton::{Skeleton, ARCHETYPE_COUNT};

#[test]
fn sdf_rasterize_produces_nonempty_sprite() {
    for archetype in 0..ARCHETYPE_COUNT {
        let skel = Skeleton::instantiate(archetype, 42);
        let raster = rasterize_skeleton(&skel);
        let filled = raster.sprite.cells.iter().filter(|c| **c != CellKind::Empty).count();
        assert!(filled > 0, "Archetype {} produced empty sprite", archetype);
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
    let skel = Skeleton::instantiate(0, 42);
    let raster = rasterize_skeleton(&skel);
    for (i, cell) in raster.sprite.cells.iter().enumerate() {
        match cell {
            CellKind::Body | CellKind::Border => {
                assert!(raster.capsule_ids[i] > 0, "Filled cell at index {} has capsule_id 0", i);
            }
            CellKind::Empty => {}
        }
    }
}

#[test]
fn bipedal_legs_are_separated() {
    let skel = Skeleton::instantiate(0, 42);
    let raster = rasterize_skeleton(&skel);
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
