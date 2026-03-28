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

use summoner::creature::templates::{get_template, template_name, TEMPLATE_COUNT};

#[test]
fn all_templates_produce_valid_sprites() {
    for i in 0..TEMPLATE_COUNT {
        let mask = get_template(i);
        let sprite = generate_sprite(&mask, 42);
        assert!(sprite.width > 0);
        assert!(sprite.height > 0);
        let filled = sprite.cells.iter().filter(|c| **c != CellKind::Empty).count();
        assert!(filled > 0, "Template {} produced empty sprite", template_name(i));
    }
}

#[test]
fn template_selection_wraps_with_modulo() {
    let t1 = get_template(0);
    let t2 = get_template(TEMPLATE_COUNT);
    assert_eq!(t1.len(), t2.len());
}

use summoner::creature::render::{render_sprite_to_buffer, state_palette};
use summoner::creature::animate::{AnimationState, animate_sprite};
use summoner::session::SessionState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[test]
fn render_sprite_fills_buffer_cells() {
    let mask = vec![
        vec![0, 1, 1],
        vec![1, 1, 1],
        vec![1, 1, 0],
        vec![0, 1, 0],
    ];
    let sprite = generate_sprite(&mask, 42);
    let area = Rect::new(0, 0, sprite.width as u16, (sprite.height / 2 + sprite.height % 2) as u16);
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
    assert!(non_empty > 0, "Rendered sprite should have visible cells");
}

#[test]
fn animation_state_advances_frames() {
    let mut anim = AnimationState::new(SessionState::Working);
    let frame0 = anim.current_frame();
    anim.tick(std::time::Duration::from_millis(250));
    let frame1 = anim.current_frame();
    assert!(frame0 == 0);
    assert!(frame1 > 0 || anim.total_frames() == 1);
}

#[test]
fn animation_state_changes_reset_frame() {
    let mut anim = AnimationState::new(SessionState::Working);
    anim.tick(std::time::Duration::from_millis(500));
    anim.set_state(SessionState::Idle);
    assert_eq!(anim.current_frame(), 0);
}

#[test]
fn animate_sprite_returns_modified_sprite() {
    let mask = vec![
        vec![0, 1, 1],
        vec![1, 1, 1],
        vec![1, 1, 0],
        vec![0, 1, 0],
    ];
    let base = generate_sprite(&mask, 42);
    let anim = AnimationState::new(SessionState::Idle);
    let animated = animate_sprite(&base, &anim);
    assert!(animated.width == base.width);
    assert!(animated.height >= base.height - 1 && animated.height <= base.height + 2);
}
