use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use dirs;
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Padding, Widget};

fn expand_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), rest);
        }
    }
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.display().to_string();
        }
    }
    path.to_string()
}

fn display_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        if let Some(rest) = path.strip_prefix(&home_str) {
            return format!("~{}", rest);
        }
    }
    path.to_string()
}

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
    user_typed: bool,
}

impl DirPicker {
    pub fn new(recent_dirs: Vec<String>, initial_query: Option<String>) -> Self {
        let mut picker = Self {
            recent_dirs: recent_dirs.clone(),
            query: initial_query.unwrap_or_default(),
            filtered: Vec::new(),
            selected: 0,
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
            user_typed: false,
        };
        // Show recent dirs by default without filtering
        picker.filtered = recent_dirs.iter().map(|d| (d.clone(), 0)).collect();
        picker
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> DirPickerAction {
        match key.code {
            KeyCode::Esc => DirPickerAction::Cancel,
            KeyCode::Enter => {
                if let Some((dir, _)) = self.filtered.get(self.selected) {
                    DirPickerAction::Select(expand_path(dir))
                } else if !self.query.is_empty() {
                    DirPickerAction::Select(expand_path(&self.query))
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
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.user_typed = true;
                self.query.clear();
                self.update_filter();
                self.selected = 0;
                DirPickerAction::None
            }
            KeyCode::Char(c) => {
                self.user_typed = true;
                self.query.push(c);
                self.update_filter();
                self.selected = 0;
                DirPickerAction::None
            }
            KeyCode::Backspace => {
                self.user_typed = true;
                self.query.pop();
                self.update_filter();
                self.selected = 0;
                DirPickerAction::None
            }
            _ => DirPickerAction::None,
        }
    }

    fn update_filter(&mut self) {
        if !self.user_typed {
            self.filtered = self.recent_dirs.iter().map(|d| (d.clone(), 0)).collect();
            return;
        }
        if self.query.is_empty() {
            self.filtered = self.recent_dirs.iter().map(|d| (d.clone(), 0)).collect();
            return;
        }

        // Start with fuzzy matches from recent dirs
        let pattern = Pattern::new(
            &self.query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        self.filtered = pattern
            .match_list(&self.recent_dirs, &mut self.matcher)
            .into_iter()
            .map(|(s, score)| (s.clone(), score))
            .collect();

        // If query looks like a path, also scan filesystem
        if self.query.starts_with('/') || self.query.starts_with('~') || self.query.starts_with('.') {
            let expanded = expand_path(&self.query);
            let scan_dir = if std::path::Path::new(&expanded).is_dir() {
                expanded.clone()
            } else {
                // Parent directory
                std::path::Path::new(&expanded)
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            };

            if let Ok(entries) = std::fs::read_dir(&scan_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        let path = entry.path().display().to_string();
                        if !self.filtered.iter().any(|(d, _)| d == &path) {
                            // Score based on whether the entry name matches the query tail
                            let name = entry.file_name().to_string_lossy().to_string();
                            let query_tail = self.query.rsplit('/').next().unwrap_or(&self.query);
                            if query_tail.is_empty() || name.to_lowercase().contains(&query_tail.to_lowercase()) {
                                self.filtered.push((path, 0));
                            }
                        }
                    }
                }
            }

            // If the expanded path itself is a dir and not already listed, add it at top
            if std::path::Path::new(&expanded).is_dir() && !self.filtered.iter().any(|(d, _)| d == &expanded) {
                self.filtered.insert(0, (expanded, u32::MAX));
            }
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
                let tilde_dir = display_path(dir);
                let truncated = if tilde_dir.len() > max_len {
                    tilde_dir[tilde_dir.len() - max_len..].to_string()
                } else {
                    tilde_dir
                };
                let display = format!("{}{}", prefix, truncated);
                buf.set_string(inner.x, y, &display, style);
            }
        }
    }
}
