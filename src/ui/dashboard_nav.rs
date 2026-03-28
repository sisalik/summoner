// Navigation state — flat navigation across all sessions

#[derive(Debug)]
pub struct DashboardNav {
    selected: usize,
    total_sessions: usize,
}

impl DashboardNav {
    pub fn new() -> Self {
        Self {
            selected: 0,
            total_sessions: 0,
        }
    }

    pub fn update_total(&mut self, total: usize) {
        self.total_sessions = total;
        if total > 0 {
            self.selected = self.selected.min(total - 1);
        } else {
            self.selected = 0;
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn move_left(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.selected + 1 < self.total_sessions {
            self.selected += 1;
        }
    }

    pub fn move_up(&mut self) {
        self.move_left();
    }

    pub fn move_down(&mut self) {
        self.move_right();
    }
}
