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

    pub fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    pub fn chance(&mut self, probability: f64) -> bool {
        (self.next_u64() % 1000) < (probability * 1000.0) as u64
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
