// Navigation state — Task 11

#[derive(Debug)]
pub struct DashboardNav {
    card_index: usize,
    session_index: Option<usize>,
    card_count: usize,
    sessions_per_card: Vec<usize>,
}

impl DashboardNav {
    pub fn new() -> Self {
        Self {
            card_index: 0,
            session_index: None,
            card_count: 0,
            sessions_per_card: Vec::new(),
        }
    }

    pub fn update_counts(&mut self, sessions_per_card: Vec<usize>) {
        self.card_count = sessions_per_card.len();
        self.sessions_per_card = sessions_per_card;
        if self.card_count > 0 {
            self.card_index = self.card_index.min(self.card_count - 1);
        } else {
            self.card_index = 0;
            self.session_index = None;
        }
        if let Some(si) = self.session_index {
            let max = self.current_card_session_count();
            if max == 0 {
                self.session_index = None;
            } else {
                self.session_index = Some(si.min(max - 1));
            }
        }
    }

    pub fn selected_card(&self) -> usize { self.card_index }
    pub fn selected_session_in_card(&self) -> Option<usize> { self.session_index }
    pub fn is_in_card(&self) -> bool { self.session_index.is_some() }

    pub fn enter_card(&mut self) {
        if self.current_card_session_count() > 0 {
            self.session_index = Some(0);
        }
    }

    pub fn exit_card(&mut self) { self.session_index = None; }

    pub fn move_left(&mut self) {
        if let Some(si) = &mut self.session_index {
            if *si > 0 { *si -= 1; }
        } else if self.card_index > 0 {
            self.card_index -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if let Some(si) = self.session_index {
            let max = self.current_card_session_count();
            if si + 1 < max { self.session_index = Some(si + 1); }
        } else if self.card_index + 1 < self.card_count {
            self.card_index += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.session_index.is_none() && self.card_index > 0 {
            self.card_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.session_index.is_none() && self.card_index + 1 < self.card_count {
            self.card_index += 1;
        }
    }

    pub fn selected_global_session(&self, group_session_indices: &[Vec<usize>]) -> Option<usize> {
        let si = self.session_index?;
        group_session_indices
            .get(self.card_index)
            .and_then(|indices| indices.get(si).copied())
    }

    fn current_card_session_count(&self) -> usize {
        self.sessions_per_card.get(self.card_index).copied().unwrap_or(0)
    }
}
