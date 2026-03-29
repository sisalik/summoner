// Navigation state — flat navigation across all sessions, with grid-aware up/down

pub fn grid_cols_for_count(count: usize) -> usize {
    match count {
        0 | 1 => 1,
        2 | 3 => count,
        _ => 3,
    }
}

#[derive(Debug)]
pub struct DashboardNav {
    selected: usize,
    total: usize,
    grid_cols: usize,
    /// group_ranges[i] = (start_session_idx, count)
    group_ranges: Vec<(usize, usize)>,
}

impl Default for DashboardNav {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardNav {
    pub fn new() -> Self {
        Self {
            selected: 0,
            total: 0,
            grid_cols: 1,
            group_ranges: Vec::new(),
        }
    }

    pub fn update_layout(&mut self, group_sizes: &[usize]) {
        self.grid_cols = grid_cols_for_count(group_sizes.len());
        self.total = group_sizes.iter().sum();
        let mut start = 0;
        self.group_ranges = group_sizes
            .iter()
            .map(|&size| {
                let range = (start, size);
                start += size;
                range
            })
            .collect();
        if self.total > 0 {
            self.selected = self.selected.min(self.total - 1);
        } else {
            self.selected = 0;
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Convert flat position to actual session index using the given order mapping.
    pub fn selected_session(&self, session_order: &[usize]) -> Option<usize> {
        session_order.get(self.selected).copied()
    }

    pub fn grid_cols(&self) -> usize {
        self.grid_cols
    }

    fn current_group(&self) -> Option<usize> {
        self.group_ranges
            .iter()
            .position(|&(start, size)| self.selected >= start && self.selected < start + size)
    }

    fn position_in_group(&self) -> usize {
        if let Some(g) = self.current_group() {
            self.selected - self.group_ranges[g].0
        } else {
            0
        }
    }

    pub fn move_left(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.selected + 1 < self.total {
            self.selected += 1;
        }
    }

    pub fn move_up(&mut self) {
        if let Some(g) = self.current_group()
            && g >= self.grid_cols {
                let target_group = g - self.grid_cols;
                let pos = self.position_in_group();
                let (start, size) = self.group_ranges[target_group];
                self.selected = start + pos.min(size - 1);
            }
    }

    pub fn move_down(&mut self) {
        if let Some(g) = self.current_group() {
            let target_group = g + self.grid_cols;
            if target_group < self.group_ranges.len() {
                let pos = self.position_in_group();
                let (start, size) = self.group_ranges[target_group];
                self.selected = start + pos.min(size - 1);
            }
        }
    }
}
