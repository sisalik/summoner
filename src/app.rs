use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::DefaultTerminal;

use crate::claude::{detect_claude_state, find_conversation_id};
use crate::config::{AppConfig, RecentDirs, SessionStore, SessionEntry};
use crate::creature::animate::AnimationState;
use crate::creature::generate::{generate_sprite, Sprite};
use crate::creature::templates::{get_template, template_index, template_name, TEMPLATE_COUNT};
use crate::session::{Session, SessionState, session_order};
use crate::terminal::PtySession;
use crate::ui::dashboard::Dashboard;
use crate::ui::dashboard_nav::DashboardNav;
use crate::ui::dir_picker::{DirPicker, DirPickerAction};
use crate::ui::session_view::TerminalView;
use crate::ui::status_bar::StatusBar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Dashboard,
    Session(usize),
    DirPicker,
}

struct App {
    mode: Mode,
    sessions: Vec<Session>,
    pty_sessions: Vec<Option<PtySession>>,
    vt_parsers: Vec<vt100::Parser>,
    animations: Vec<AnimationState>,
    sprites: Vec<Sprite>,
    nav: DashboardNav,
    dir_picker: Option<DirPicker>,
    config: AppConfig,
    recent_dirs: RecentDirs,
    config_dir: PathBuf,
    last_tick: Instant,
    confirm_close_project: Option<String>,
    confirm_selection: bool,
    last_session: Option<usize>,
    last_save: Instant,
}

pub fn display_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        if let Some(rest) = path.strip_prefix(&home_str) {
            return format!("~{}", rest);
        }
    }
    path.to_string()
}

impl App {
    fn new() -> Result<Self> {
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".summoner");

        let config = AppConfig::load(&config_dir).unwrap_or_default();
        let recent_dirs = RecentDirs::load(&config_dir).unwrap_or_else(|_| RecentDirs {
            directories: Vec::new(),
        });
        let store = SessionStore::load(&config_dir).unwrap_or_else(|_| SessionStore {
            sessions: Vec::new(),
        });

        let mut sessions = Vec::new();
        let mut pty_sessions = Vec::new();
        let mut vt_parsers = Vec::new();
        let mut animations = Vec::new();
        let mut sprites = Vec::new();

        // Restore disconnected sessions from store
        for entry in &store.sessions {
            let template_idx = template_index(&entry.creature_template);
            let mask = get_template(template_idx);
            let sprite = generate_sprite(&mask, entry.creature_seed);

            let mut session = Session::new(
                entry.directory.clone(),
                entry.creature_seed,
                entry.creature_template.clone(),
            );
            session.id = entry.id;
            session.state = SessionState::Disconnected;
            session.claude_conversation_id = entry.claude_conversation_id.clone();

            sessions.push(session);
            pty_sessions.push(None);
            vt_parsers.push(vt100::Parser::new(24, 80, 0));
            animations.push(AnimationState::new(SessionState::Disconnected));
            sprites.push(sprite);
        }

        let mut nav = DashboardNav::new();
        {
            let groups = crate::session::group_by_project(&sessions);
            let group_sizes: Vec<usize> = groups.iter().map(|g| g.sessions.len()).collect();
            nav.update_layout(&group_sizes);
        }

        let mode = if sessions.is_empty() {
            Mode::DirPicker
        } else {
            Mode::Dashboard
        };

        let dir_picker = if mode == Mode::DirPicker {
            let cwd = std::env::current_dir()
                .ok()
                .map(|p| display_path(&p.display().to_string()));
            Some(DirPicker::new(recent_dirs.directories.clone(), cwd))
        } else {
            None
        };

        Ok(Self {
            mode,
            sessions,
            pty_sessions,
            vt_parsers,
            animations,
            sprites,
            nav,
            dir_picker,
            config,
            recent_dirs,
            config_dir,
            last_tick: Instant::now(),
            confirm_close_project: None,
            confirm_selection: false,
            last_session: None,
            last_save: Instant::now(),
        })
    }

    fn spawn_session(&mut self, directory: String, rows: u16, cols: u16) -> Result<()> {
        let seed = rand_seed();
        let template_idx = self.sessions.len() % TEMPLATE_COUNT;
        let creature_template = template_name(template_idx).to_string();
        let mask = get_template(template_idx);
        let sprite = generate_sprite(&mask, seed);

        let session = Session::new(directory.clone(), seed, creature_template);

        let pty = PtySession::spawn(&self.config.general.default_shell, &directory, rows, cols)?;

        let idx = self.sessions.len();
        self.sessions.push(session);
        self.pty_sessions.push(Some(pty));
        self.vt_parsers.push(vt100::Parser::new(rows, cols, 0));
        self.animations.push(AnimationState::new(SessionState::ShellOnly));
        self.sprites.push(sprite);

        // Update recent dirs
        let max = self.config.new_session.recent_dirs_count;
        self.recent_dirs.add(directory, max);
        let _ = self.recent_dirs.save(&self.config_dir);

        {
            let groups = crate::session::group_by_project(&self.sessions);
            let group_sizes: Vec<usize> = groups.iter().map(|g| g.sessions.len()).collect();
            self.nav.update_layout(&group_sizes);
        }

        self.last_session = Some(idx);
        self.mode = Mode::Session(idx);
        Ok(())
    }

    fn close_session(&mut self, idx: usize) {
        if idx >= self.sessions.len() {
            return;
        }

        // Kill PTY if alive
        if let Some(Some(pty)) = self.pty_sessions.get(idx) {
            pty.kill();
        }

        self.sessions.remove(idx);
        self.pty_sessions.remove(idx);
        self.vt_parsers.remove(idx);
        self.animations.remove(idx);
        self.sprites.remove(idx);

        {
            let groups = crate::session::group_by_project(&self.sessions);
            let group_sizes: Vec<usize> = groups.iter().map(|g| g.sessions.len()).collect();
            self.nav.update_layout(&group_sizes);
        }

        // Adjust mode
        match self.mode {
            Mode::Session(i) if i == idx => {
                if self.sessions.is_empty() {
                    self.mode = Mode::Dashboard;
                } else {
                    self.mode = Mode::Session(i.min(self.sessions.len() - 1));
                }
            }
            Mode::Session(i) if i > idx => {
                self.mode = Mode::Session(i - 1);
            }
            _ => {}
        }
    }

    fn process_pty_output(&mut self) {
        let mut dir_changed = false;
        let mut remove_active: Option<usize> = None;
        for i in 0..self.sessions.len() {
            let pty = match &self.pty_sessions[i] {
                Some(p) => p,
                None => continue,
            };

            // Read output and feed to vt100 parser
            let chunks = pty.read_available();
            for chunk in &chunks {
                self.vt_parsers[i].process(chunk);
            }

            // Update working directory from /proc/PID/cwd
            if let Some(cwd) = pty.cwd() {
                if cwd != self.sessions[i].directory {
                    self.sessions[i].directory = cwd.clone();
                    self.sessions[i].name = std::path::Path::new(&cwd)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    dir_changed = true;
                }
            }

            // Detect Claude state from screen with hysteresis
            let screen = self.vt_parsers[i].screen();
            let in_alternate_screen = screen.alternate_screen();
            let detected = detect_claude_state(screen);

            // If not in alternate screen, Claude has exited — use ShellOnly as default
            let new_state = if let Some(state) = detected {
                state
            } else if in_alternate_screen {
                // In alternate screen but no patterns matched — probably still Claude, default Idle
                if self.sessions[i].claude_conversation_id.is_some()
                    || matches!(self.sessions[i].state, SessionState::Working | SessionState::Waiting | SessionState::Idle)
                {
                    SessionState::Idle
                } else {
                    SessionState::ShellOnly
                }
            } else {
                // Normal screen — no Claude running
                SessionState::ShellOnly
            };

            if new_state != self.sessions[i].state {
                // State change detected - require consistency
                if self.sessions[i].pending_state == Some(new_state) {
                    self.sessions[i].pending_state_count += 1;
                } else {
                    self.sessions[i].pending_state = Some(new_state);
                    self.sessions[i].pending_state_count = 1;
                }

                // Only commit state change after consistent detections
                let threshold = if new_state == SessionState::Working || new_state == SessionState::Waiting {
                    1 // React to Working/Waiting immediately
                } else if self.sessions[i].state == SessionState::Working {
                    8 // Very resistant to leaving Working state (prevents flashing)
                } else {
                    3 // Normal threshold for other transitions
                };

                if self.sessions[i].pending_state_count >= threshold {
                    self.sessions[i].state = new_state;
                    self.animations[i].set_state(new_state);
                    self.sessions[i].pending_state = None;
                    self.sessions[i].pending_state_count = 0;
                }
            } else {
                // State unchanged, reset pending
                self.sessions[i].pending_state = None;
                self.sessions[i].pending_state_count = 0;
            }

            // Try to find conversation ID if we don't have one
            if self.sessions[i].claude_conversation_id.is_none() {
                if let Some(pid) = pty.pid() {
                    if let Some(conv_id) = find_conversation_id(pid) {
                        self.sessions[i].claude_conversation_id = Some(conv_id);
                    }
                }
            }

            // Check for child exit
            if pty.try_wait().is_some() {
                self.pty_sessions[i] = None;
                if matches!(self.mode, Mode::Session(idx) if idx == i) {
                    // Currently viewing this session — mark for removal
                    remove_active = Some(i);
                } else {
                    // Background session died — mark disconnected
                    self.sessions[i].state = SessionState::Disconnected;
                    self.animations[i].set_state(SessionState::Disconnected);
                    // Adjust mode index if needed
                    if let Mode::Session(idx) = self.mode {
                        if idx > i {
                            self.mode = Mode::Session(idx - 1);
                        }
                    }
                }
            }
        }

        if let Some(idx) = remove_active {
            self.close_session(idx);
            self.mode = Mode::Dashboard;
        }

        if dir_changed {
            {
            let groups = crate::session::group_by_project(&self.sessions);
            let group_sizes: Vec<usize> = groups.iter().map(|g| g.sessions.len()).collect();
            self.nav.update_layout(&group_sizes);
        }
        }
    }

    fn save_state(&self) {
        let entries: Vec<SessionEntry> = self
            .sessions
            .iter()
            .map(|s| SessionEntry {
                id: s.id,
                directory: s.directory.clone(),
                creature_seed: s.creature_seed,
                creature_template: s.creature_template.clone(),
                claude_conversation_id: s.claude_conversation_id.clone(),
                last_active: chrono::Utc::now(),
                active: true,
            })
            .collect();

        let store = SessionStore { sessions: entries };
        let _ = store.save(&self.config_dir);
    }

    fn render(&self, terminal: &mut DefaultTerminal) -> Result<()> {
        terminal.draw(|frame| {
            let area = frame.area();

            let layout = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);

            let main_area = layout[0];
            let status_area = layout[1];

            // Render main content
            match self.mode {
                Mode::Dashboard | Mode::DirPicker => {
                    let dashboard = Dashboard::new(
                        &self.sessions,
                        &self.animations,
                        &self.sprites,
                        &self.nav,
                    );
                    frame.render_widget(dashboard, main_area);

                    // Render confirmation overlay for project close
                    if let Some(ref project_dir) = self.confirm_close_project {
                        let popup_area = centered_rect(50, 20, main_area);
                        let display_dir = display_path(project_dir);
                        render_confirm_overlay(frame, popup_area, &display_dir, self.confirm_selection);
                    }

                    // Render dir picker overlay
                    if self.mode == Mode::DirPicker {
                        if let Some(ref picker) = self.dir_picker {
                            let popup_area = centered_rect(60, 60, main_area);
                            frame.render_widget(picker, popup_area);
                        }
                    }
                }
                Mode::Session(idx) => {
                    if idx < self.vt_parsers.len() {
                        let screen = self.vt_parsers[idx].screen();
                        let view = TerminalView::new(screen);
                        frame.render_widget(view, main_area);
                    }
                }
            }

            // Render status bar
            let active_index = match self.mode {
                Mode::Session(idx) => Some(idx),
                _ => None,
            };
            let status_bar = StatusBar::new(&self.sessions, active_index);
            frame.render_widget(status_bar, status_area);
        })?;

        Ok(())
    }

    fn handle_input(&mut self, key: KeyEvent, rows: u16, cols: u16) -> Result<bool> {
        // Handle confirmation overlay first
        if self.confirm_close_project.is_some() {
            match key.code {
                KeyCode::Left | KeyCode::Right => {
                    self.confirm_selection = !self.confirm_selection;
                }
                KeyCode::Enter => {
                    if self.confirm_selection {
                        // Yes selected — close all sessions
                        let dir = self.confirm_close_project.take().unwrap();
                        let indices: Vec<usize> = self.sessions.iter().enumerate()
                            .filter(|(_, s)| s.directory == dir)
                            .map(|(i, _)| i)
                            .collect();
                        for &idx in indices.iter().rev() {
                            self.close_session(idx);
                        }
                        self.mode = Mode::Dashboard;
                    } else {
                        // No selected — cancel
                        self.confirm_close_project = None;
                    }
                    self.confirm_selection = false;
                }
                KeyCode::Char('y') => {
                    let dir = self.confirm_close_project.take().unwrap();
                    let indices: Vec<usize> = self.sessions.iter().enumerate()
                        .filter(|(_, s)| s.directory == dir)
                        .map(|(i, _)| i)
                        .collect();
                    for &idx in indices.iter().rev() {
                        self.close_session(idx);
                    }
                    self.mode = Mode::Dashboard;
                    self.confirm_selection = false;
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.confirm_close_project = None;
                    self.confirm_selection = false;
                }
                _ => {}
            }
            return Ok(false);
        }

        // Ctrl+C or Ctrl+Q always quits
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('q'))
        {
            return Ok(true);
        }

        // F12 toggles between dashboard and last session
        if key.code == KeyCode::F(12) {
            match self.mode {
                Mode::Dashboard | Mode::DirPicker => {
                    // Go back to last session if one exists
                    if let Some(idx) = self.last_session {
                        if idx < self.sessions.len() && self.pty_sessions.get(idx).is_some_and(|p| p.is_some()) {
                            self.mode = Mode::Session(idx);
                            return Ok(false);
                        }
                    }
                    // No active session to return to
                }
                Mode::Session(idx) => {
                    self.last_session = Some(idx);
                    self.mode = Mode::Dashboard;
                }
            }
            return Ok(false);
        }

        // F1-F11 switch to session (by appearance order)
        if let KeyCode::F(n) = key.code {
            if n >= 1 && n <= 11 {
                let pos = (n - 1) as usize;
                let order = session_order(&self.sessions);
                if let Some(&sess_idx) = order.get(pos) {
                    // Resize PTY to match terminal
                    if let Some(Some(pty)) = self.pty_sessions.get(sess_idx) {
                        // Status bar takes 1 row
                        let pty_rows = rows.saturating_sub(1);
                        let _ = pty.resize(pty_rows, cols);
                    }
                    self.last_session = Some(sess_idx);
                    self.mode = Mode::Session(sess_idx);
                }
                return Ok(false);
            }
        }

        match self.mode {
            Mode::DirPicker => {
                if let Some(ref mut picker) = self.dir_picker {
                    match picker.handle_key(key) {
                        DirPickerAction::Cancel => {
                            self.dir_picker = None;
                            self.mode = Mode::Dashboard;
                        }
                        DirPickerAction::Select(dir) => {
                            self.dir_picker = None;
                            // Status bar takes 1 row
                            let pty_rows = rows.saturating_sub(1);
                            self.spawn_session(dir, pty_rows, cols)?;
                        }
                        DirPickerAction::None => {}
                    }
                }
            }
            Mode::Dashboard => {
                self.handle_dashboard_input(key, rows, cols)?;
            }
            Mode::Session(idx) => {
                self.handle_session_input(key, idx)?;
            }
        }

        Ok(false)
    }

    fn handle_dashboard_input(&mut self, key: KeyEvent, rows: u16, cols: u16) -> Result<()> {
        let order = session_order(&self.sessions);
        let sel = self.nav.selected_session(&order);

        match key.code {
            KeyCode::Char('N') => {
                // Open dir picker for new directory
                let cwd = std::env::current_dir()
                    .ok()
                    .map(|p| display_path(&p.display().to_string()));
                self.dir_picker = Some(DirPicker::new(self.recent_dirs.directories.clone(), cwd));
                self.mode = Mode::DirPicker;
            }
            KeyCode::Char('n') => {
                // New session in same directory as selected session
                if let Some(sess_idx) = sel {
                    let dir = self.sessions[sess_idx].directory.clone();
                    let pty_rows = rows.saturating_sub(1);
                    self.spawn_session(dir, pty_rows, cols)?;
                } else {
                    // No sessions, open dir picker
                    let cwd = std::env::current_dir()
                        .ok()
                        .map(|p| display_path(&p.display().to_string()));
                    self.dir_picker = Some(DirPicker::new(self.recent_dirs.directories.clone(), cwd));
                    self.mode = Mode::DirPicker;
                }
            }
            KeyCode::Char('x') => {
                // Close selected session
                if let Some(sess_idx) = sel {
                    self.close_session(sess_idx);
                }
            }
            KeyCode::Char('X') => {
                // Close all sessions in selected project (with confirmation)
                if let Some(sess_idx) = sel {
                    self.confirm_close_project = Some(self.sessions[sess_idx].directory.clone());
                    self.confirm_selection = false; // Default to No (safer)
                }
            }
            KeyCode::Left => self.nav.move_left(),
            KeyCode::Right => self.nav.move_right(),
            KeyCode::Up => self.nav.move_up(),
            KeyCode::Down => self.nav.move_down(),
            KeyCode::Enter => {
                if let Some(sess_idx) = sel {
                    if self.pty_sessions[sess_idx].is_none() {
                        // Dead session — restore it
                        self.restore_session(sess_idx, rows, cols)?;
                    } else {
                        // Active session — switch to it
                        let pty_rows = rows.saturating_sub(1);
                        if let Some(Some(pty)) = self.pty_sessions.get(sess_idx) {
                            let _ = pty.resize(pty_rows, cols);
                        }
                        self.last_session = Some(sess_idx);
                        self.mode = Mode::Session(sess_idx);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_session_input(&mut self, key: KeyEvent, idx: usize) -> Result<()> {
        if idx >= self.pty_sessions.len() {
            return Ok(());
        }

        if let Some(ref pty) = self.pty_sessions[idx] {
            let bytes = key_to_bytes(key);
            if !bytes.is_empty() {
                let _ = pty.write(&bytes);
            }
        }

        Ok(())
    }

    fn restore_session(&mut self, index: usize, rows: u16, cols: u16) -> Result<()> {
        if index >= self.sessions.len() {
            return Ok(());
        }
        // Don't restore if already has an active PTY
        if self.pty_sessions[index].is_some() {
            self.mode = Mode::Session(index);
            return Ok(());
        }

        let directory = self.sessions[index].directory.clone();
        let session_rows = rows.saturating_sub(1);
        let pty = PtySession::spawn(&self.config.general.default_shell, &directory, session_rows, cols)?;

        // If there's a claude conversation id, resume it
        if let Some(ref conv_id) = self.sessions[index].claude_conversation_id.clone() {
            let cmd = format!("claude --resume {}\r\n", conv_id);
            let _ = pty.write(cmd.as_bytes());
        } else {
            // Write a newline to kick the shell into showing a prompt
            let _ = pty.write(b"\n");
        }

        self.pty_sessions[index] = Some(pty);
        // Reset vt100 parser to current terminal size
        self.vt_parsers[index] = vt100::Parser::new(session_rows, cols, 0);
        self.sessions[index].state = SessionState::ShellOnly;
        self.animations[index].set_state(SessionState::ShellOnly);
        self.last_session = Some(index);
        self.mode = Mode::Session(index);
        Ok(())
    }

    fn tick_animations(&mut self, dt: Duration) {
        for anim in &mut self.animations {
            anim.tick(dt);
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let width = area.width * percent_x / 100;
    let height = area.height * percent_y / 100;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn render_confirm_overlay(frame: &mut ratatui::Frame, area: Rect, project_dir: &str, yes_selected: bool) {
    use ratatui::widgets::{Block, Borders, Clear, Padding};
    use ratatui::style::{Color, Modifier, Style};

    Clear.render(area, frame.buffer_mut());

    let block = Block::default()
        .title(" Confirm Close ")
        .title_style(Style::default().fg(Color::Rgb(255, 100, 100)).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(200, 80, 80)))
        .padding(Padding::uniform(1));

    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());

    if inner.height >= 3 && inner.width >= 20 {
        let msg = format!("Close all sessions in {}?", project_dir);
        let msg_style = Style::default().fg(Color::Rgb(220, 220, 240));
        frame.buffer_mut().set_string(inner.x, inner.y, &msg, msg_style);

        // Draw buttons on the third line (inner.y + 2)
        let btn_y = inner.y + 2;

        let yes_label = "[ Yes ]";
        let no_label  = "[ No ]";

        let selected_style = Style::default()
            .fg(Color::Rgb(20, 20, 30))
            .bg(Color::Rgb(200, 80, 80))
            .add_modifier(Modifier::BOLD);
        let normal_style = Style::default()
            .fg(Color::Rgb(150, 150, 170))
            .add_modifier(Modifier::DIM);

        let (yes_style, no_style) = if yes_selected {
            (selected_style, normal_style)
        } else {
            (normal_style, selected_style)
        };

        frame.buffer_mut().set_string(inner.x, btn_y, yes_label, yes_style);
        frame.buffer_mut().set_string(inner.x + yes_label.len() as u16 + 4, btn_y, no_label, no_style);

        // Navigation hint
        let hint_style = Style::default().fg(Color::Rgb(100, 100, 120));
        let hint = "◄ ► to switch  Enter to confirm";
        if inner.height >= 5 {
            frame.buffer_mut().set_string(inner.x, inner.y + 4, hint, hint_style);
        }
    }
}

use ratatui::widgets::Widget;

fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Char(c) if ctrl => {
            // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
            let byte = (c as u8).wrapping_sub(b'a').wrapping_add(1);
            if byte <= 26 {
                vec![byte]
            } else {
                vec![]
            }
        }
        KeyCode::Char(c) if alt => {
            let mut bytes = vec![0x1b];
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            bytes
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf).as_bytes().to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}

fn rand_seed() -> u64 {
    use std::time::SystemTime;
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_nanos() as u64 ^ dur.as_micros() as u64
}

pub fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut app = App::new()?;

    loop {
        app.render(terminal)?;

        let timeout = match app.mode {
            Mode::Dashboard | Mode::DirPicker => Duration::from_millis(33),
            Mode::Session(_) => Duration::from_millis(16),
        };

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    let size = terminal.size()?;
                    let should_quit = app.handle_input(key, size.height, size.width)?;
                    if should_quit {
                        app.save_state();
                        return Ok(());
                    }
                }
                Event::Resize(cols, rows) => {
                    let session_rows = rows.saturating_sub(1);
                    // Resize all active PTY sessions
                    for pty_opt in &app.pty_sessions {
                        if let Some(pty) = pty_opt {
                            let _ = pty.resize(session_rows, cols);
                        }
                    }
                    // Also resize all vt parsers
                    for parser in &mut app.vt_parsers {
                        parser.screen_mut().set_size(session_rows, cols);
                    }
                }
                _ => {}
            }
        }

        // Process PTY output
        app.process_pty_output();

        // Periodic state save (every 10 seconds)
        let now = Instant::now();
        if now.duration_since(app.last_save) >= Duration::from_secs(10) {
            app.save_state();
            app.last_save = now;
        }

        // Tick animations (mostly useful in dashboard)
        let dt = now.duration_since(app.last_tick);
        app.last_tick = now;
        if matches!(app.mode, Mode::Dashboard | Mode::DirPicker) {
            app.tick_animations(dt);
        }
    }
}
