// Navigation state — flat navigation across all sessions, with row-aware up/down

#[derive(Debug)]
pub struct DashboardNav {
    selected: usize,
    total: usize,
    /// group_ranges[i] = (start_session_idx, count)
    group_ranges: Vec<(usize, usize)>,
    /// row_groups[row] = [group_idx, ...] — which groups are on each visual row
    row_groups: Vec<Vec<usize>>,
    /// Index of the topmost row rendered in the full band.
    scroll_row: usize,
    /// Cached count of fully-rendered rows from the most recent render.
    /// Input handlers use this to drive `ensure_selection_visible`.
    visible_full_rows: usize,
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
            scroll_row: 0,
            visible_full_rows: 0,
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
        // Clamp scroll_row against new layout.
        let max = self.max_scroll_row();
        if self.scroll_row > max {
            self.scroll_row = max;
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

    pub fn scroll_row(&self) -> usize {
        self.scroll_row
    }

    pub fn visible_full_rows(&self) -> usize {
        self.visible_full_rows
    }

    pub fn set_visible_full_rows(&mut self, n: usize) {
        self.visible_full_rows = n;
        let max = self.max_scroll_row();
        if self.scroll_row > max {
            self.scroll_row = max;
        }
    }

    pub fn max_scroll_row(&self) -> usize {
        self.row_groups.len().saturating_sub(self.visible_full_rows)
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_row > 0 {
            self.scroll_row -= 1;
        }
    }

    pub fn scroll_down(&mut self) {
        let max = self.max_scroll_row();
        if self.scroll_row < max {
            self.scroll_row += 1;
        }
    }

    /// Move viewport so the row containing the selection sits inside
    /// `[scroll_row, scroll_row + visible_full_rows - 1]`.
    pub fn ensure_selection_visible(&mut self) {
        if self.visible_full_rows == 0 { return; }
        let Some(g) = self.current_group() else { return };
        let Some(row) = self.group_row(g) else { return };

        if row < self.scroll_row {
            self.scroll_row = row;
        } else if row >= self.scroll_row + self.visible_full_rows {
            self.scroll_row = row + 1 - self.visible_full_rows;
        }
        let max = self.max_scroll_row();
        if self.scroll_row > max {
            self.scroll_row = max;
        }
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
        self.ensure_selection_visible();
    }

    pub fn move_right(&mut self) {
        if self.selected + 1 < self.total {
            self.selected += 1;
        }
        self.ensure_selection_visible();
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
        self.ensure_selection_visible();
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
        self.ensure_selection_visible();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav_with(rows: Vec<Vec<usize>>, sizes: Vec<usize>) -> DashboardNav {
        let mut nav = DashboardNav::new();
        nav.update_layout_with_rows(&sizes, rows);
        nav
    }

    #[test]
    fn scroll_up_down_clamps_to_max() {
        // 4 rows of 1 group each, 2 visible -> max_scroll_row = 2
        let mut nav = nav_with(
            vec![vec![0], vec![1], vec![2], vec![3]],
            vec![1, 1, 1, 1],
        );
        nav.set_visible_full_rows(2);

        assert_eq!(nav.scroll_row(), 0);
        nav.scroll_up();
        assert_eq!(nav.scroll_row(), 0, "cannot scroll above 0");

        nav.scroll_down();
        assert_eq!(nav.scroll_row(), 1);
        nav.scroll_down();
        assert_eq!(nav.scroll_row(), 2);
        nav.scroll_down();
        assert_eq!(nav.scroll_row(), 2, "cannot scroll past max");
    }

    #[test]
    fn ensure_selection_visible_pulls_viewport_down() {
        let mut nav = nav_with(
            vec![vec![0], vec![1], vec![2], vec![3]],
            vec![1, 1, 1, 1],
        );
        nav.set_visible_full_rows(2);

        // Select row 3, expect scroll_row -> 2 so selection is last visible
        nav.set_selected(3);
        nav.ensure_selection_visible();
        assert_eq!(nav.scroll_row(), 2);
    }

    #[test]
    fn ensure_selection_visible_pulls_viewport_up() {
        let mut nav = nav_with(
            vec![vec![0], vec![1], vec![2], vec![3]],
            vec![1, 1, 1, 1],
        );
        nav.set_visible_full_rows(2);
        nav.set_selected(3);
        nav.ensure_selection_visible();
        assert_eq!(nav.scroll_row(), 2);

        nav.set_selected(0);
        nav.ensure_selection_visible();
        assert_eq!(nav.scroll_row(), 0, "selection above band -> scroll_row becomes selection row");
    }

    #[test]
    fn ensure_selection_visible_noop_when_in_band() {
        let mut nav = nav_with(
            vec![vec![0], vec![1], vec![2], vec![3]],
            vec![1, 1, 1, 1],
        );
        nav.set_visible_full_rows(2);
        nav.scroll_down();
        assert_eq!(nav.scroll_row(), 1);

        nav.set_selected(2); // row 2 is inside [1, 2]
        nav.ensure_selection_visible();
        assert_eq!(nav.scroll_row(), 1, "no change when selection already visible");
    }

    #[test]
    fn layout_update_clamps_scroll_row() {
        let mut nav = nav_with(
            vec![vec![0], vec![1], vec![2], vec![3]],
            vec![1, 1, 1, 1],
        );
        nav.set_visible_full_rows(2);
        nav.scroll_down();
        nav.scroll_down();
        assert_eq!(nav.scroll_row(), 2);

        // Layout shrinks to 2 rows total
        nav.update_layout_with_rows(&[1, 1], vec![vec![0], vec![1]]);
        assert!(nav.scroll_row() <= nav.max_scroll_row(),
                "scroll_row must be clamped on layout change");
    }
}
