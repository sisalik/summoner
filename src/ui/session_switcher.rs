use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Padding, Widget};
use unicode_width::UnicodeWidthStr;

use crate::app::display_path;
use crate::session::{session_order, Session, SessionState};

const MAX_VISIBLE_ROWS: usize = 10;
const POPUP_WIDTH: u16 = 64;

pub enum SwitcherAction {
    None,
    Cancel,
    /// Switch to this session (index into the session vec).
    Select(usize),
}

struct Entry {
    sess_idx: usize,
    name: String,
    path: String,
    state: SessionState,
    fkey_pos: Option<usize>,
    is_current: bool,
    focus_seq: u64,
    haystack: String,
}

impl Entry {
    fn waiting(&self) -> bool {
        self.state == SessionState::Waiting
    }
}

struct Row {
    entry: usize,
    /// Char positions of fuzzy-match hits within the entry's haystack.
    indices: Vec<u32>,
}

pub struct SessionSwitcher {
    entries: Vec<Entry>,
    filtered: Vec<Row>,
    query: String,
    selected: usize,
    matcher: Matcher,
}

impl SessionSwitcher {
    pub fn new(sessions: &[Session], focus_seqs: &[u64], current: Option<usize>) -> Self {
        let order = session_order(sessions);
        let entries = sessions
            .iter()
            .enumerate()
            .map(|(idx, session)| {
                let path = display_path(&session.directory);
                let haystack = format!("{} {}", session.name, path);
                Entry {
                    sess_idx: idx,
                    name: session.name.clone(),
                    path,
                    state: session.state,
                    fkey_pos: order.iter().position(|&i| i == idx).filter(|&p| p < 11),
                    is_current: current == Some(idx),
                    focus_seq: focus_seqs.get(idx).copied().unwrap_or(0),
                    haystack,
                }
            })
            .collect();

        let mut switcher = Self {
            entries,
            filtered: Vec::new(),
            query: String::new(),
            selected: 0,
            matcher: Matcher::new(Config::DEFAULT),
        };
        switcher.update_filter();
        switcher.select_default();
        switcher
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SwitcherAction {
        match key.code {
            KeyCode::Esc => SwitcherAction::Cancel,
            KeyCode::Enter => match self.filtered.get(self.selected) {
                Some(row) => SwitcherAction::Select(self.entries[row.entry].sess_idx),
                None => SwitcherAction::None,
            },
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                SwitcherAction::None
            }
            KeyCode::Down => {
                if self.selected + 1 < self.filtered.len() {
                    self.selected += 1;
                }
                SwitcherAction::None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.clear();
                self.refilter();
                SwitcherAction::None
            }
            // Ctrl+Backspace deletes the last word; legacy terminals send it
            // as 0x08, which crossterm reports as Ctrl+H
            KeyCode::Backspace | KeyCode::Char('h')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                crate::ui::delete_last_word(&mut self.query);
                self.refilter();
                SwitcherAction::None
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.refilter();
                SwitcherAction::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
                SwitcherAction::None
            }
            _ => SwitcherAction::None,
        }
    }

    /// Session index the current selection points at, if any.
    pub fn selected_session(&self) -> Option<usize> {
        self.filtered
            .get(self.selected)
            .map(|row| self.entries[row.entry].sess_idx)
    }

    /// Visible session indices in display order.
    pub fn visible_sessions(&self) -> Vec<usize> {
        self.filtered
            .iter()
            .map(|row| self.entries[row.entry].sess_idx)
            .collect()
    }

    /// F-key position (0-based; F1 = 0) shown for a session, if it has one.
    pub fn fkey_pos(&self, sess_idx: usize) -> Option<usize> {
        self.entries
            .iter()
            .find(|e| e.sess_idx == sess_idx)
            .and_then(|e| e.fkey_pos)
    }

    pub fn preferred_size(&self) -> (u16, u16) {
        // borders (2) + padding (2) + input (1) + separator (1) + rows
        let rows = self.filtered.len().clamp(1, MAX_VISIBLE_ROWS) as u16;
        (POPUP_WIDTH, rows + 6)
    }

    fn refilter(&mut self) {
        self.update_filter();
        if self.query.is_empty() {
            self.select_default();
        } else {
            self.selected = 0;
        }
    }

    fn update_filter(&mut self) {
        if self.query.is_empty() {
            let mut rows: Vec<Row> = (0..self.entries.len())
                .map(|entry| Row { entry, indices: Vec::new() })
                .collect();
            rows.sort_by_key(|row| {
                let e = &self.entries[row.entry];
                (!e.waiting(), std::cmp::Reverse(e.focus_seq))
            });
            self.filtered = rows;
            return;
        }

        let pattern = Pattern::new(
            &self.query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut buf = Vec::new();
        let mut scored: Vec<(u32, Row)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(entry, e)| {
                let mut indices = Vec::new();
                let haystack = Utf32Str::new(&e.haystack, &mut buf);
                let score = pattern.indices(haystack, &mut self.matcher, &mut indices)?;
                indices.sort_unstable();
                indices.dedup();
                Some((score, Row { entry, indices }))
            })
            .collect();
        scored.sort_by_key(|(score, row)| {
            let e = &self.entries[row.entry];
            (std::cmp::Reverse(*score), !e.waiting(), std::cmp::Reverse(e.focus_seq))
        });
        self.filtered = scored.into_iter().map(|(_, row)| row).collect();
    }

    /// Select the most recently used non-current session — the "previous"
    /// session — so opening the switcher and pressing Enter toggles back.
    fn select_default(&mut self) {
        self.selected = self
            .filtered
            .iter()
            .enumerate()
            .filter(|(_, row)| !self.entries[row.entry].is_current)
            .max_by_key(|(_, row)| self.entries[row.entry].focus_seq)
            .map(|(i, _)| i)
            .unwrap_or(0);
    }
}

impl Widget for &SessionSwitcher {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let block = Block::default()
            .title(" Switch Session ")
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

        if inner.height < 3 || inner.width < 20 {
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
        let cursor_x = inner.x + (prompt.len() + self.query.width()) as u16;
        if let Some(cell) = buf.cell_mut(Position { x: cursor_x, y: inner.y }) {
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

        let results_start = sep_y + 1;
        let max_results = (inner.height - 2) as usize;

        if self.filtered.is_empty() {
            buf.set_string(
                inner.x,
                results_start,
                "No matching sessions.",
                Style::default().fg(Color::Rgb(100, 100, 120)),
            );
            return;
        }

        let name_col = self
            .filtered
            .iter()
            .map(|row| self.entries[row.entry].name.width())
            .max()
            .unwrap_or(0)
            .min(24);

        for (i, row) in self.filtered.iter().take(max_results).enumerate() {
            let entry = &self.entries[row.entry];
            let y = results_start + i as u16;
            let is_selected = i == self.selected;
            render_row(buf, inner, y, entry, &row.indices, is_selected, name_col);
        }
    }
}

/// One result row: `▸ ● name  ~/path  F1`, fuzzy hits highlighted, the
/// current session dimmed wholesale.
fn render_row(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    entry: &Entry,
    indices: &[u32],
    is_selected: bool,
    name_col: usize,
) {
    let bg = if is_selected { Some(Color::Rgb(50, 50, 80)) } else { None };
    let apply_bg = |mut style: Style| {
        if let Some(bg) = bg {
            style = style.bg(bg);
        }
        style
    };

    let (base, path_style, hit_style) = if entry.is_current {
        let dim = apply_bg(Style::default().fg(Color::Rgb(100, 100, 120)));
        (dim, dim, dim.add_modifier(Modifier::BOLD))
    } else {
        let mut base = apply_bg(Style::default().fg(Color::Rgb(180, 180, 200)));
        if is_selected {
            base = base.fg(Color::Rgb(255, 255, 255)).add_modifier(Modifier::BOLD);
        }
        (
            base,
            apply_bg(Style::default().fg(if is_selected {
                Color::Rgb(192, 192, 220)
            } else {
                Color::Rgb(130, 130, 155)
            })),
            apply_bg(Style::default().fg(Color::Rgb(210, 180, 255)).add_modifier(Modifier::BOLD)),
        )
    };

    // Selected-row background across the full width
    if bg.is_some() {
        for x in inner.x..inner.x + inner.width {
            if let Some(cell) = buf.cell_mut(Position { x, y }) {
                cell.set_symbol(" ");
                cell.set_style(base);
            }
        }
    }

    let mut x = inner.x;
    let max_x = inner.x + inner.width;
    let mut put = |x: &mut u16, text: &str, style: Style| {
        for ch in text.chars() {
            if *x >= max_x {
                return;
            }
            if let Some(cell) = buf.cell_mut(Position { x: *x, y }) {
                cell.set_symbol(&ch.to_string());
                cell.set_style(style);
            }
            *x += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1) as u16;
        }
    };

    put(&mut x, if is_selected { "▸ " } else { "  " }, base);

    let icon_style = if entry.is_current {
        base
    } else {
        apply_bg(Style::default().fg(entry.state.color()))
    };
    put(&mut x, entry.state.bar_icon(), icon_style);
    put(&mut x, " ", base);

    // Name and path share one haystack ("name path"); hit indices are char
    // positions into it, so track a running char index across both parts.
    let name_chars = entry.name.chars().count() as u32;
    for (ci, ch) in entry.name.chars().enumerate() {
        let style = if indices.contains(&(ci as u32)) { hit_style } else { base };
        put(&mut x, &ch.to_string(), style);
    }
    let pad = name_col.saturating_sub(entry.name.width()) + 2;
    put(&mut x, &" ".repeat(pad), base);

    // Reserve room on the right for the F-key label
    let fkey = entry.fkey_pos.map(|p| format!("F{}", p + 1));
    let fkey_w = fkey.as_ref().map(|f| f.len() + 2).unwrap_or(0) as u16;
    let path_max = max_x.saturating_sub(x + fkey_w);
    let path_width = entry.path.width() as u16;
    if path_width <= path_max {
        for (ci, ch) in entry.path.chars().enumerate() {
            let hay_idx = name_chars + 1 + ci as u32;
            let style = if indices.contains(&hay_idx) { hit_style } else { path_style };
            put(&mut x, &ch.to_string(), style);
        }
    } else {
        // Keep the tail — that's where the project directory name lives
        let skip = entry
            .path
            .chars()
            .count()
            .saturating_sub(path_max.saturating_sub(1) as usize);
        put(&mut x, "…", path_style);
        for (ci, ch) in entry.path.chars().enumerate().skip(skip) {
            let hay_idx = name_chars + 1 + ci as u32;
            let style = if indices.contains(&hay_idx) { hit_style } else { path_style };
            put(&mut x, &ch.to_string(), style);
        }
    }

    if let Some(fkey) = fkey {
        let fx = max_x.saturating_sub(fkey.len() as u16);
        let style = if entry.is_current {
            base
        } else {
            apply_bg(Style::default().fg(Color::Rgb(106, 106, 132)))
        };
        let mut fx = fx;
        put(&mut fx, &fkey, style);
    }
}
