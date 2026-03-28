use crossterm::event::{KeyCode, KeyEvent};
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Padding, Widget};

pub enum DirPickerAction {
    None,
    Cancel,
    Select(String),
}

pub struct DirPicker {
    recent_dirs: Vec<String>,
    query: String,
    filtered: Vec<(String, u32)>,
    selected: usize,
    matcher: Matcher,
}

impl DirPicker {
    pub fn new(recent_dirs: Vec<String>) -> Self {
        let filtered = recent_dirs.iter().map(|d| (d.clone(), 0)).collect();
        Self {
            recent_dirs,
            query: String::new(),
            filtered,
            selected: 0,
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> DirPickerAction {
        match key.code {
            KeyCode::Esc => DirPickerAction::Cancel,
            KeyCode::Enter => {
                if let Some((dir, _)) = self.filtered.get(self.selected) {
                    DirPickerAction::Select(dir.clone())
                } else if !self.query.is_empty() {
                    DirPickerAction::Select(self.query.clone())
                } else {
                    DirPickerAction::None
                }
            }
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                DirPickerAction::None
            }
            KeyCode::Down => {
                if self.selected + 1 < self.filtered.len() {
                    self.selected += 1;
                }
                DirPickerAction::None
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.update_filter();
                self.selected = 0;
                DirPickerAction::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.update_filter();
                self.selected = 0;
                DirPickerAction::None
            }
            _ => DirPickerAction::None,
        }
    }

    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = self.recent_dirs.iter().map(|d| (d.clone(), 0)).collect();
        } else {
            let pattern = Pattern::new(
                &self.query,
                CaseMatching::Ignore,
                Normalization::Smart,
                AtomKind::Fuzzy,
            );
            // match_list returns Vec<(&String, u32)>; clone the string refs into owned Strings
            let matches = pattern.match_list(&self.recent_dirs, &mut self.matcher);
            self.filtered = matches
                .into_iter()
                .map(|(s, score)| (s.clone(), score))
                .collect();
        }
    }
}

impl Widget for &DirPicker {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let block = Block::default()
            .title(" Open Directory ")
            .title_style(
                Style::default()
                    .fg(Color::Rgb(220, 220, 240))
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(100, 100, 140)))
            .padding(Padding::uniform(1));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 || inner.width < 10 {
            return;
        }

        // Search input line
        let prompt = "> ";
        let input_style = Style::default().fg(Color::Rgb(220, 220, 240));
        buf.set_string(inner.x, inner.y, prompt, input_style);
        buf.set_string(
            inner.x + prompt.len() as u16,
            inner.y,
            &self.query,
            input_style.add_modifier(Modifier::BOLD),
        );

        // Cursor
        let cursor_x = inner.x + prompt.len() as u16 + self.query.len() as u16;
        if let Some(cell) = buf.cell_mut(Position {
            x: cursor_x,
            y: inner.y,
        }) {
            cell.set_symbol("▌");
            cell.set_style(Style::default().fg(Color::Rgb(150, 150, 200)));
        }

        // Separator
        let sep_y = inner.y + 1;
        for x in inner.x..inner.x + inner.width {
            if let Some(cell) = buf.cell_mut(Position { x, y: sep_y }) {
                cell.set_symbol("─");
                cell.set_style(Style::default().fg(Color::Rgb(60, 60, 80)));
            }
        }

        // Results list
        let results_start = sep_y + 1;
        let max_results = (inner.height - 2) as usize;

        if self.filtered.is_empty() {
            let msg = if self.query.is_empty() {
                "No recent directories. Type a path."
            } else {
                "No matches. Press Enter to use as path."
            };
            buf.set_string(
                inner.x,
                results_start,
                msg,
                Style::default().fg(Color::Rgb(100, 100, 120)),
            );
        } else {
            for (i, (dir, _score)) in self.filtered.iter().take(max_results).enumerate() {
                let y = results_start + i as u16;
                let is_selected = i == self.selected;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Rgb(255, 255, 255))
                        .bg(Color::Rgb(50, 50, 80))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(180, 180, 200))
                };
                let prefix = if is_selected { "▸ " } else { "  " };
                let max_len = inner.width as usize - 2;
                let display_dir = if dir.len() > max_len {
                    &dir[dir.len() - max_len..]
                } else {
                    dir.as_str()
                };
                let display = format!("{}{}", prefix, display_dir);
                buf.set_string(inner.x, y, &display, style);
            }
        }
    }
}
