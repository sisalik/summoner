// Navigation state — flat navigation across all sessions, with row-aware up/down

#[derive(Debug)]
pub struct DashboardNav {
    selected: usize,
    total: usize,
    /// group_ranges[i] = (start_session_idx, count)
    group_ranges: Vec<(usize, usize)>,
    /// row_groups[row] = [group_idx, ...] — which groups are on each visual row
    row_groups: Vec<Vec<usize>>,
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
            group_ranges: Vec::new(),
            row_groups: Vec::new(),
        }
    }

    pub fn update_layout_with_rows(&mut self, group_sizes: &[usize], row_groups: Vec<Vec<usize>>) {
        self.row_groups = row_groups;
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

    pub fn set_selected(&mut self, pos: usize) {
        if self.total > 0 {
            self.selected = pos.min(self.total - 1);
        }
    }

    /// Convert flat position to actual session index using the given order mapping.
    pub fn selected_session(&self, session_order: &[usize]) -> Option<usize> {
        session_order.get(self.selected).copied()
    }

    pub fn row_groups(&self) -> &[Vec<usize>] {
        &self.row_groups
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

    fn group_row(&self, group_idx: usize) -> Option<usize> {
        self.row_groups.iter().position(|row| row.contains(&group_idx))
    }

    fn group_col_in_row(&self, group_idx: usize, row: usize) -> Option<usize> {
        self.row_groups.get(row).and_then(|r| r.iter().position(|&g| g == group_idx))
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
        let Some(g) = self.current_group() else { return };
        let Some(row) = self.group_row(g) else { return };
        if row == 0 { return; }

        let col = self.group_col_in_row(g, row).unwrap_or(0);
        let target_row = &self.row_groups[row - 1];
        let target_group = target_row[col.min(target_row.len() - 1)];
        let pos = self.position_in_group();
        let (start, size) = self.group_ranges[target_group];
        self.selected = start + pos.min(size - 1);
    }

    pub fn move_down(&mut self) {
        let Some(g) = self.current_group() else { return };
        let Some(row) = self.group_row(g) else { return };
        if row + 1 >= self.row_groups.len() { return; }

        let col = self.group_col_in_row(g, row).unwrap_or(0);
        let target_row = &self.row_groups[row + 1];
        let target_group = target_row[col.min(target_row.len() - 1)];
        let pos = self.position_in_group();
        let (start, size) = self.group_ranges[target_group];
        self.selected = start + pos.min(size - 1);
    }
}
