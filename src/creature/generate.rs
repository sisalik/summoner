/// Xorshift64 PRNG — deterministic, fast, seedable.
pub struct Xorshift {
    state: u64,
}

impl Xorshift {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    pub fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    pub fn chance(&mut self, probability: f64) -> bool {
        (self.next() % 1000) < (probability * 1000.0) as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Empty,
    Body,
    Border,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<CellKind>,
}

impl Sprite {
    pub fn get(&self, x: usize, y: usize) -> CellKind {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x]
        } else {
            CellKind::Empty
        }
    }

    pub fn set(&mut self, x: usize, y: usize, kind: CellKind) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = kind;
        }
    }
}

/// Mask cell values:
///  -1 = always border
///   0 = always empty
///   1 = 50% body, 50% empty
///   2 = 50% body, 50% border
///
/// The mask represents the LEFT HALF. It is mirrored to produce the full sprite.
pub fn generate_sprite(mask: &[Vec<i8>], seed: u64) -> Sprite {
    let half_width = mask[0].len();
    let full_width = half_width * 2;
    let height = mask.len();

    let mut sprite = Sprite {
        width: full_width,
        height,
        cells: vec![CellKind::Empty; full_width * height],
    };

    let mut rng = Xorshift::new(seed);

    // Phase 1: Fill the left half from the mask
    for y in 0..height {
        for x in 0..half_width {
            let cell = mask[y][x];
            let kind = match cell {
                -1 => CellKind::Border,
                0 => CellKind::Empty,
                1 => {
                    if rng.chance(0.5) { CellKind::Body } else { CellKind::Empty }
                }
                2 => {
                    if rng.chance(0.5) { CellKind::Body } else { CellKind::Border }
                }
                _ => CellKind::Empty,
            };
            sprite.set(x, y, kind);
        }
    }

    // Phase 2: Mirror left half to right half
    for y in 0..height {
        for x in 0..half_width {
            let kind = sprite.get(x, y);
            let mirror_x = full_width - 1 - x;
            sprite.set(mirror_x, y, kind);
        }
    }

    // Phase 3: Edge detection — body cells adjacent to empty become border
    let snapshot = sprite.cells.clone();
    for y in 0..height {
        for x in 0..full_width {
            let idx = y * full_width + x;
            if snapshot[idx] == CellKind::Body {
                let has_empty_neighbor = neighbors(x, y, full_width, height)
                    .any(|(nx, ny)| snapshot[ny * full_width + nx] == CellKind::Empty);
                if has_empty_neighbor {
                    sprite.set(x, y, CellKind::Border);
                }
            }
        }
    }

    sprite
}

fn neighbors(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> impl Iterator<Item = (usize, usize)> {
    let deltas: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    deltas.into_iter().filter_map(move |(dx, dy)| {
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
            Some((nx as usize, ny as usize))
        } else {
            None
        }
    })
}
