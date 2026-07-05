use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::DefaultTerminal;

use crate::claude::{find_claude_child, find_conversation_id, is_claude_stopped};
use crate::hooks;
use crate::config::{AppConfig, RecentDirs, SessionStore, SessionEntry};
use crate::creature::locomotion::LocomotionState;
use crate::creature::outline::{rasterize_skeleton, RasterResult};
use crate::creature::skeleton::{Skeleton, archetype_index, archetype_name};
use crate::session::{Session, SessionState, SessionStats, GlobalStats, group_by_project, session_order};
use crate::git::GitDiffCache;
use crate::terminal::PtySession;
use crate::ui::dashboard::{card_metrics, layout_width, Dashboard, session_at_position};
use crate::ui::status_bar::{tab_at_x, tab_visible_range, TabHit};
use crate::ui::dashboard_nav::DashboardNav;
use crate::ui::dir_picker::{DirPicker, DirPickerAction};
use crate::ui::selection::{self, Selection};
use crate::ui::session_view::TerminalView;
use crate::ui::status_bar::StatusBar;

const SCROLLBACK_LEN: usize = 1000;

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
    locomotions: Vec<LocomotionState>,
    sprites: Vec<RasterResult>,
    nav: DashboardNav,
    dir_picker: Option<DirPicker>,
    config: AppConfig,
    recent_dirs: RecentDirs,
    config_dir: PathBuf,
    last_tick: Instant,
    confirm_close_project: Option<String>,
    confirm_selection: bool,
    confirm_quit: bool,
    confirm_quit_selection: bool,
    last_session: Option<usize>,
    last_save: Instant,
    last_term_width: u16,
    session_stats: Vec<SessionStats>,
    global_stats: GlobalStats,
    git_cache: GitDiffCache,
    last_stats_update: Instant,
    last_statusline_check: Instant,
    selection: Option<Selection>,
    session_area: Rect,
    dashboard_area: Rect,
    status_bar_area: Rect,
    strip_alt_screen: bool,
    reordering: bool,
    last_click: Option<(Instant, u16, u16)>,
    click_count: u8,
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

        // Install Claude Code hooks for state detection
        hooks::install_hooks(&config_dir);

        // Clean stale statusLine files from previous runs
        crate::statusline::clear_stale_files(&config_dir);

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
        let mut locomotions = Vec::new();
        let mut sprites = Vec::new();
        let mut session_stats = Vec::new();

        // Restore disconnected sessions from store
        for entry in &store.sessions {
            let archetype_idx = archetype_index(&entry.creature_template);
            let skeleton = Skeleton::instantiate(archetype_idx, entry.creature_seed);
            let raster = rasterize_skeleton(&skeleton);

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
            vt_parsers.push(vt100::Parser::new(24, 80, SCROLLBACK_LEN));
            locomotions.push(LocomotionState::new(skeleton, SessionState::Disconnected));
            sprites.push(raster);
            session_stats.push(SessionStats::new());
        }

        let mut nav = DashboardNav::new();
        {
            let groups = crate::session::group_by_project(&sessions);
            let group_sizes: Vec<usize> = groups.iter().map(|g| g.sessions.len()).collect();
            let layout = crate::ui::dashboard::flow_layout(&group_sizes, layout_width(80));
            nav.update_layout_with_rows(&group_sizes, layout.row_groups);
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
            locomotions,
            sprites,
            nav,
            dir_picker,
            config,
            recent_dirs,
            config_dir,
            last_tick: Instant::now(),
            confirm_close_project: None,
            confirm_selection: false,
            confirm_quit: false,
            confirm_quit_selection: true,
            last_session: None,
            last_save: Instant::now(),
            last_term_width: 80, // default, updated on first render/resize
            session_stats,
            global_stats: GlobalStats::new(),
            git_cache: GitDiffCache::new(30),
            last_stats_update: Instant::now(),
            last_statusline_check: Instant::now(),
            selection: None,
            session_area: Rect::default(),
            dashboard_area: Rect::default(),
            status_bar_area: Rect::default(),
            strip_alt_screen: std::env::var("SUMMONER_ALLOW_ALT_SCREEN")
                .map_or(true, |v| v != "1"),
            reordering: false,
            last_click: None,
            click_count: 0,
        })
    }

    fn refresh_nav_layout(&mut self) {
        let groups = crate::session::group_by_project(&self.sessions);
        let group_sizes: Vec<usize> = groups.iter().map(|g| g.sessions.len()).collect();
        let layout = crate::ui::dashboard::flow_layout(&group_sizes, layout_width(self.last_term_width));
        self.nav.update_layout_with_rows(&group_sizes, layout.row_groups);
    }

    fn swap_sessions(&mut self, a: usize, b: usize) {
        self.sessions.swap(a, b);
        self.pty_sessions.swap(a, b);
        self.vt_parsers.swap(a, b);
        self.locomotions.swap(a, b);
        self.sprites.swap(a, b);
        self.session_stats.swap(a, b);
        self.remap_indices(a, b);
        self.refresh_nav_layout();
    }

    /// Reorder all parallel arrays according to `new_order[new_pos] = old_index`.
    fn apply_order(&mut self, new_order: &[usize]) {
        let n = new_order.len();
        let mut sessions: Vec<Option<Session>> = self.sessions.drain(..).map(Some).collect();
        let mut ptys: Vec<Option<Option<PtySession>>> = self.pty_sessions.drain(..).map(Some).collect();
        let mut parsers: Vec<Option<vt100::Parser>> = self.vt_parsers.drain(..).map(Some).collect();
        let mut locos: Vec<Option<LocomotionState>> = self.locomotions.drain(..).map(Some).collect();
        let mut rasters: Vec<Option<RasterResult>> = self.sprites.drain(..).map(Some).collect();
        let mut stats: Vec<Option<SessionStats>> = self.session_stats.drain(..).map(Some).collect();
        for &old_idx in new_order {
            self.sessions.push(sessions[old_idx].take().unwrap());
            self.pty_sessions.push(ptys[old_idx].take().unwrap());
            self.vt_parsers.push(parsers[old_idx].take().unwrap());
            self.locomotions.push(locos[old_idx].take().unwrap());
            self.sprites.push(rasters[old_idx].take().unwrap());
            self.session_stats.push(stats[old_idx].take().unwrap());
        }
        let mut old_to_new = vec![0usize; n];
        for (new_pos, &old_idx) in new_order.iter().enumerate() {
            old_to_new[old_idx] = new_pos;
        }
        if let Mode::Session(ref mut idx) = self.mode
            && *idx < n { *idx = old_to_new[*idx]; }
        if let Some(ref mut idx) = self.last_session
            && *idx < n { *idx = old_to_new[*idx]; }
        self.refresh_nav_layout();
    }

    fn remap_indices(&mut self, a: usize, b: usize) {
        if let Mode::Session(ref mut idx) = self.mode {
            if *idx == a { *idx = b; }
            else if *idx == b { *idx = a; }
        }
        if let Some(ref mut idx) = self.last_session {
            if *idx == a { *idx = b; }
            else if *idx == b { *idx = a; }
        }
    }

    /// Move a session within its group or swap entire groups at boundaries.
    fn reorder_move(&mut self, direction: i8) {
        let order = session_order(&self.sessions);
        let Some(sess_idx) = self.nav.selected_session(&order) else { return };
        let groups = group_by_project(&self.sessions);
        let Some((group_idx, local_pos)) = groups.iter().enumerate()
            .find_map(|(gi, g)| {
                g.sessions.iter().position(|&si| si == sess_idx)
                    .map(|pos| (gi, pos))
            }) else { return };
        let group = &groups[group_idx];
        let new_vec_idx = if direction < 0 {
            if local_pos > 0 {
                let other = group.sessions[local_pos - 1];
                self.swap_sessions(sess_idx, other);
                other
            } else if group_idx > 0 {
                self.swap_groups(&groups, group_idx, group_idx - 1, sess_idx)
            } else {
                return;
            }
        } else {
            if local_pos + 1 < group.sessions.len() {
                let other = group.sessions[local_pos + 1];
                self.swap_sessions(sess_idx, other);
                other
            } else if group_idx + 1 < groups.len() {
                self.swap_groups(&groups, group_idx, group_idx + 1, sess_idx)
            } else {
                return;
            }
        };
        let new_order = session_order(&self.sessions);
        if let Some(new_pos) = new_order.iter().position(|&i| i == new_vec_idx) {
            self.nav.set_selected(new_pos);
            self.nav.ensure_selection_visible();
        }
    }

    /// Swap the selected session's entire group with the group on the adjacent visual row.
    fn reorder_move_vertical(&mut self, direction: i8) {
        let order = session_order(&self.sessions);
        let Some(sess_idx) = self.nav.selected_session(&order) else { return };
        let groups = group_by_project(&self.sessions);
        let Some(group_idx) = groups.iter().position(|g| g.sessions.contains(&sess_idx)) else { return };

        let target_group_idx = {
            let row_groups = self.nav.row_groups();
            let Some(row) = row_groups.iter().position(|r| r.contains(&group_idx)) else { return };
            let col = row_groups[row].iter().position(|&g| g == group_idx).unwrap_or(0);
            let target_row = if direction < 0 {
                if row == 0 { return; }
                row - 1
            } else {
                if row + 1 >= row_groups.len() { return; }
                row + 1
            };
            let tr = &row_groups[target_row];
            if tr.is_empty() { return; }
            tr[col.min(tr.len() - 1)]
        };

        if target_group_idx == group_idx { return; }

        let new_vec_idx = self.swap_groups(&groups, group_idx, target_group_idx, sess_idx);
        let new_order = session_order(&self.sessions);
        if let Some(new_pos) = new_order.iter().position(|&i| i == new_vec_idx) {
            self.nav.set_selected(new_pos);
            self.nav.ensure_selection_visible();
        }
    }

    /// Swap two groups and return the new Vec index of `tracked` session.
    fn swap_groups(&mut self, groups: &[crate::session::ProjectGroup], a: usize, b: usize, tracked: usize) -> usize {
        let mut group_indices: Vec<usize> = (0..groups.len()).collect();
        group_indices.swap(a, b);
        let mut new_order = Vec::new();
        for &gi in &group_indices {
            new_order.extend(&groups[gi].sessions);
        }
        let new_vec_idx = new_order.iter().position(|&old| old == tracked).unwrap();
        self.apply_order(&new_order);
        new_vec_idx
    }

    fn spawn_session(&mut self, directory: String, rows: u16, cols: u16) -> Result<()> {
        let seed = rand_seed();
        let enabled = crate::creature::skeleton::ENABLED_ARCHETYPES;
        let archetype_idx = enabled[self.sessions.len() % enabled.len()];
        let creature_template = archetype_name(archetype_idx).to_string();
        let skeleton = Skeleton::instantiate(archetype_idx, seed);
        let raster = rasterize_skeleton(&skeleton);

        let session = Session::new(directory.clone(), seed, creature_template);

        let pty = PtySession::spawn(&self.config.general.default_shell, &directory, rows, cols)?;

        // Clear any stale hook state file for this PID (in case of PID reuse)
        if let Some(pid) = pty.pid() {
            hooks::clear_state_file(&self.config_dir, pid);
        }

        let idx = self.sessions.len();
        self.sessions.push(session);
        self.pty_sessions.push(Some(pty));
        self.vt_parsers.push(vt100::Parser::new(rows, cols, SCROLLBACK_LEN));
        self.locomotions.push(LocomotionState::new(skeleton, SessionState::ShellOnly));
        self.sprites.push(raster);
        self.session_stats.push(SessionStats::new());

        // Update recent dirs
        let max = self.config.new_session.recent_dirs_count;
        self.recent_dirs.add(directory, max);
        let _ = self.recent_dirs.save(&self.config_dir);

        self.refresh_nav_layout();

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
        self.locomotions.remove(idx);
        self.sprites.remove(idx);
        self.session_stats.remove(idx);

        self.refresh_nav_layout();

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
                if self.strip_alt_screen {
                    let (rows, _) = self.vt_parsers[i].screen().size();
                    let filtered = rewrite_for_scrollback(chunk, rows);
                    self.vt_parsers[i].process(&filtered);
                } else {
                    self.vt_parsers[i].process(chunk);
                }
            }

            // Update working directory from /proc/PID/cwd
            if let Some(cwd) = pty.cwd()
                && cwd != self.sessions[i].directory {
                    self.sessions[i].directory = cwd.clone();
                    self.sessions[i].name = std::path::Path::new(&cwd)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    dir_changed = true;
                }

            // Primary: use Claude Code hooks for state detection
            // Detect Claude state via hooks (primary) or process check (fallback)
            let shell_pid = pty.pid();
            let (hook_state, hook_session_id, hook_tool) = shell_pid
                .map(|pid| hooks::read_hook_state(&self.config_dir, pid))
                .unwrap_or((None, None, None));

            if i < self.session_stats.len() {
                self.session_stats[i].active_tool = hook_tool;
                if hook_state.is_some() && hook_state != self.session_stats[i].prev_hook_state {
                    self.session_stats[i].last_activity = Instant::now();
                }
                self.session_stats[i].prev_hook_state = hook_state;
            }

            // Pick up conversation ID from hooks or fallback.
            // Always prefer the hook's session ID — it updates on /clear and /resume.
            if let Some(sid) = hook_session_id {
                if self.sessions[i].claude_conversation_id.as_deref() != Some(&sid) {
                    self.sessions[i].claude_conversation_id = Some(sid);
                }
            } else if self.sessions[i].claude_conversation_id.is_none()
                && let Some(pid) = shell_pid
                    && let Some(conv_id) = find_conversation_id(pid) {
                        self.sessions[i].claude_conversation_id = Some(conv_id);
                    }

            let new_state = if shell_pid.is_some_and(is_claude_stopped) {
                SessionState::Sleeping
            } else if let Some(state) = hook_state {
                // If hook says idle but no activity for 30min, show as sleeping
                if state == SessionState::Idle
                    && i < self.session_stats.len()
                    && self.session_stats[i].last_activity.elapsed() >= Duration::from_secs(30 * 60)
                {
                    SessionState::Sleeping
                } else {
                    state
                }
            } else {
                // Fallback: check if claude process is running
                let claude_running = shell_pid.and_then(find_claude_child).is_some();
                if claude_running {
                    SessionState::Idle
                } else {
                    if self.sessions[i].claude_conversation_id.is_some() {
                        self.sessions[i].claude_conversation_id = None;
                    }
                    SessionState::ShellOnly
                }
            };

            if new_state != self.sessions[i].state {
                self.sessions[i].state = new_state;
                self.locomotions[i].set_state(new_state);
            }

            // Check for child exit
            if pty.try_wait().is_some() {
                // Clean up hook state file for this shell PID
                if let Some(pid) = pty.pid() {
                    hooks::clear_state_file(&self.config_dir, pid);
                }
                self.pty_sessions[i] = None;
                if matches!(self.mode, Mode::Session(idx) if idx == i) {
                    // Currently viewing this session — mark for removal
                    remove_active = Some(i);
                } else {
                    // Background session died — mark disconnected
                    self.sessions[i].state = SessionState::Disconnected;
                    self.locomotions[i].set_state(SessionState::Disconnected);
                    // Adjust mode index if needed
                    if let Mode::Session(idx) = self.mode
                        && idx > i {
                            self.mode = Mode::Session(idx - 1);
                        }
                }
            }
        }

        if let Some(idx) = remove_active {
            self.close_session(idx);
            self.mode = Mode::Dashboard;
        }

        if dir_changed {
            self.refresh_nav_layout();
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

    fn render(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        terminal.draw(|frame| {
            let area = frame.area();
            self.last_term_width = area.width;

            let layout = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);

            let main_area = layout[0];
            let status_area = layout[1];
            self.status_bar_area = status_area;

            // Clear selection when not in session mode
            if !matches!(self.mode, Mode::Session(_)) {
                self.selection = None;
            }

            // Render main content
            match self.mode {
                Mode::Dashboard | Mode::DirPicker => {
                    self.dashboard_area = main_area;
                    let mut dashboard = Dashboard::new(
                        &self.sessions,
                        &self.session_stats,
                        &self.global_stats,
                        &self.sprites,
                        &mut self.nav,
                        &mut self.git_cache,
                        self.reordering,
                    );
                    dashboard.render(main_area, frame.buffer_mut());

                    if let Some(ref project_dir) = self.confirm_close_project {
                        let msg = format!("Close all sessions in {}?", display_path(project_dir));
                        ConfirmDialog {
                            title: "Confirm Close",
                            message: &msg,
                            yes_selected: self.confirm_selection,
                        }.render(frame, main_area);
                    }

                    // Render dir picker overlay
                    if self.mode == Mode::DirPicker
                        && let Some(ref picker) = self.dir_picker {
                            let popup_area = centered_rect(60, 60, main_area);
                            frame.render_widget(picker, popup_area);
                        }
                }
                Mode::Session(idx) => {
                    let is_disconnected = idx < self.pty_sessions.len()
                        && self.pty_sessions[idx].is_none();

                    if is_disconnected && idx < self.sessions.len() {
                        render_resume_dialog(frame, main_area, &self.sessions[idx]);
                    } else if idx < self.vt_parsers.len() {
                        self.session_area = main_area;
                        let screen = self.vt_parsers[idx].screen();
                        let view = TerminalView::new(screen)
                            .with_selection(self.selection.as_ref());
                        frame.render_widget(view, main_area);
                    }
                }
            }

            if self.confirm_quit {
                ConfirmDialog {
                    title: "Quit Summoner",
                    message: "Quit Summoner?",
                    yes_selected: self.confirm_quit_selection,
                }.render(frame, main_area);
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
        // Handle quit confirmation overlay
        if self.confirm_quit {
            match key.code {
                KeyCode::Left | KeyCode::Right => {
                    self.confirm_quit_selection = !self.confirm_quit_selection;
                }
                KeyCode::Enter => {
                    if self.confirm_quit_selection {
                        return Ok(true);
                    }
                    self.confirm_quit = false;
                    self.confirm_quit_selection = true;
                }
                KeyCode::Char('y') => return Ok(true),
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.confirm_quit = false;
                    self.confirm_quit_selection = true;
                }
                _ => {}
            }
            return Ok(false);
        }

        // Handle close-project confirmation overlay
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

        // Ctrl+Q shows quit confirmation
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('q')
        {
            self.confirm_quit = true;
            self.confirm_quit_selection = true;
            return Ok(false);
        }

        // F12 toggles between dashboard and last session
        if key.code == KeyCode::F(12) {
            match self.mode {
                Mode::Dashboard | Mode::DirPicker => {
                    // Go back to last session if one exists (active or disconnected)
                    if let Some(idx) = self.last_session
                        && idx < self.sessions.len() {
                            self.reordering = false;
                            self.mode = Mode::Session(idx);
                            return Ok(false);
                        }
                    // No session to return to
                }
                Mode::Session(idx) => {
                    self.last_session = Some(idx);
                    self.mode = Mode::Dashboard;
                }
            }
            return Ok(false);
        }

        // F1-F11 switch to session (by appearance order)
        if let KeyCode::F(n) = key.code
            && (1..=11).contains(&n) {
                let pos = (n - 1) as usize;
                let order = session_order(&self.sessions);
                if let Some(&sess_idx) = order.get(pos) {
                    self.reordering = false;
                    if let Some(Some(pty)) = self.pty_sessions.get(sess_idx) {
                        let pty_rows = rows.saturating_sub(1);
                        let _ = pty.resize(pty_rows, cols);
                    }
                    self.last_session = Some(sess_idx);
                    self.mode = Mode::Session(sess_idx);
                }
                return Ok(false);
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
                // Ctrl+C with active selection: copy and clear
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('c')
                    && self.selection.is_some()
                {
                    if let Some(ref sel) = self.selection
                        && !sel.is_empty() && idx < self.vt_parsers.len() {
                            let text = selection::extract_text(
                                self.vt_parsers[idx].screen_mut(),
                                sel,
                            );
                            if !text.is_empty() {
                                selection::copy_to_clipboard(&text);
                            }
                        }
                    self.selection = None;
                    return Ok(false);
                }
                // Disconnected session: any key resumes it
                if idx < self.pty_sessions.len() && self.pty_sessions[idx].is_none() {
                    if idx < self.sessions.len() {
                        self.restore_session(idx, rows, cols)?;
                    }
                } else {
                    self.handle_session_input(key, idx)?;
                }
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
            KeyCode::Char('r') => {
                if sel.is_some() {
                    self.reordering = !self.reordering;
                }
            }
            KeyCode::Char('R') => {
                // Regenerate creature for selected session
                if let Some(sess_idx) = sel {
                    let seed = rand_seed();
                    let enabled = crate::creature::skeleton::ENABLED_ARCHETYPES;
                    let archetype_idx = enabled[seed as usize % enabled.len()];
                    let template = archetype_name(archetype_idx).to_string();
                    let skeleton = Skeleton::instantiate(archetype_idx, seed);
                    let raster = rasterize_skeleton(&skeleton);
                    self.sessions[sess_idx].creature_seed = seed;
                    self.sessions[sess_idx].creature_template = template;
                    self.locomotions[sess_idx] = LocomotionState::new(skeleton, self.sessions[sess_idx].state);
                    self.sprites[sess_idx] = raster;
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
            KeyCode::Left if self.reordering => {
                self.reorder_move(-1);
            }
            KeyCode::Right if self.reordering => {
                self.reorder_move(1);
            }
            KeyCode::Up if self.reordering => {
                self.reorder_move_vertical(-1);
            }
            KeyCode::Down if self.reordering => {
                self.reorder_move_vertical(1);
            }
            KeyCode::Left => self.nav.move_left(),
            KeyCode::Right => self.nav.move_right(),
            KeyCode::Up => self.nav.move_up(),
            KeyCode::Down => self.nav.move_down(),
            KeyCode::Enter | KeyCode::Esc if self.reordering => {
                self.reordering = false;
            }
            KeyCode::Enter => {
                if let Some(sess_idx) = sel {
                    if self.pty_sessions[sess_idx].is_none() {
                        // Disconnected session — show resume dialog (like F-keys)
                        self.last_session = Some(sess_idx);
                        self.mode = Mode::Session(sess_idx);
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
        self.selection = None;
        if idx < self.vt_parsers.len() && self.vt_parsers[idx].screen().scrollback() > 0 {
            self.vt_parsers[idx].screen_mut().set_scrollback(0);
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

        // Clear any stale hook state file for this PID
        if let Some(pid) = pty.pid() {
            hooks::clear_state_file(&self.config_dir, pid);
        }

        // If there's a claude conversation id, resume it
        if let Some(ref conv_id) = self.sessions[index].claude_conversation_id.clone() {
            let cmd = format!("claude --resume {}\r\n", conv_id);
            let _ = pty.write(cmd.as_bytes());
        }
        // Shell sessions don't need a newline — the shell shows a prompt on startup

        self.pty_sessions[index] = Some(pty);
        // Reset vt100 parser to current terminal size
        self.vt_parsers[index] = vt100::Parser::new(session_rows, cols, SCROLLBACK_LEN);
        self.sessions[index].state = SessionState::ShellOnly;
        self.locomotions[index].set_state(SessionState::ShellOnly);
        self.last_session = Some(index);
        self.mode = Mode::Session(index);
        Ok(())
    }

    fn tick_animations(&mut self, dt: Duration) {
        for (i, loco) in self.locomotions.iter_mut().enumerate() {
            loco.tick(dt);
            self.sprites[i] = rasterize_skeleton(loco.skeleton());
        }
    }

    fn update_stats(&mut self) {
        let now = Instant::now();

        // StatusLine files are tiny — read every 5 seconds for responsive rate limit display
        if now.duration_since(self.last_statusline_check).as_secs() >= 5 {
            self.last_statusline_check = now;
            // Read statusLine data, matching only to active sessions
            let active_session_ids: Vec<String> = self.sessions.iter()
                .filter(|s| s.state != SessionState::Disconnected)
                .filter_map(|s| s.claude_conversation_id.clone())
                .collect();

            let sl_data = crate::statusline::StatusLineData::read_all(&self.config_dir);
            // Use rate limits only from the most recently modified state file
            // to avoid jumping between stale snapshots from different sessions
            let newest = sl_data.iter()
                .filter(|sl| active_session_ids.contains(&sl.session_id))
                .max_by_key(|sl| sl.modified_at);
            for sl in &sl_data {
                if !active_session_ids.contains(&sl.session_id) {
                    continue;
                }
                for i in 0..self.sessions.len() {
                    if self.sessions[i].claude_conversation_id.as_deref() == Some(&sl.session_id) {
                        self.session_stats[i].context_pct = sl.context_pct;
                        if self.session_stats[i].jsonl_path.is_none()
                            && let Some(ref tp) = sl.transcript_path {
                                let p = std::path::PathBuf::from(tp);
                                if p.is_file() {
                                    self.session_stats[i].jsonl_path = Some(p);
                                }
                            }
                    }
                }
            }
            if let Some(sl) = newest {
                if sl.five_hour_pct.is_some() {
                    self.global_stats.five_hour_pct = sl.five_hour_pct;
                    self.global_stats.five_hour_resets_at = sl.five_hour_resets_at;
                }
                if sl.seven_day_pct.is_some() {
                    self.global_stats.seven_day_pct = sl.seven_day_pct;
                    self.global_stats.seven_day_resets_at = sl.seven_day_resets_at;
                }
            }
        }

        // JSONL parsing is heavier — keep at 60 seconds
        if now.duration_since(self.last_stats_update).as_secs() < 60 {
            return;
        }
        self.last_stats_update = now;

        self.global_stats.check_daily_reset();

        let claude_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude");

        // Update per-session stats from JSONL
        for i in 0..self.sessions.len() {
            let session = &self.sessions[i];
            let stats = &mut self.session_stats[i];

            if stats.jsonl_path.is_none()
                && let Some(ref conv_id) = session.claude_conversation_id {
                    stats.jsonl_path = crate::stats::find_jsonl_path(&claude_dir, &session.directory, conv_id);
                }

            if let Some(ref path) = stats.jsonl_path.clone() {
                let (new_tokens, new_messages, new_offset) =
                    crate::stats::parse_jsonl_stats(path, stats.jsonl_offset);
                stats.total_tokens += new_tokens;
                stats.message_count += new_messages;
                stats.jsonl_offset = new_offset;
                self.global_stats.daily_tokens += new_tokens;
                self.global_stats.daily_messages += new_messages;
            }
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

fn centered_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

struct ConfirmDialog<'a> {
    title: &'a str,
    message: &'a str,
    yes_selected: bool,
}

impl ConfirmDialog<'_> {
    fn render(&self, frame: &mut ratatui::Frame, area: Rect) {
        use ratatui::widgets::{Block, Borders, Clear, Padding};
        use ratatui::style::{Color, Modifier, Style};

        let yes_label = "[ Yes ]";
        let no_label = "[ No ]";
        let hint = "\u{25c4} \u{25ba} to switch  Enter to confirm";

        // Content: message, blank, buttons, blank, hint = 5 lines
        // + 2 border + 2 padding = 9
        let content_width = self.message.len()
            .max(hint.len())
            .max(yes_label.len() + 4 + no_label.len()) as u16;
        let popup = centered_fixed(content_width + 4, 9, area);

        Clear.render(popup, frame.buffer_mut());

        let title = format!(" {} ", self.title);
        let block = Block::default()
            .title(title)
            .title_style(Style::default().fg(Color::Rgb(255, 100, 100)).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(200, 80, 80)))
            .padding(Padding::uniform(1));

        let inner = block.inner(popup);
        block.render(popup, frame.buffer_mut());

        if inner.height < 3 || inner.width < 10 {
            return;
        }

        let msg_style = Style::default().fg(Color::Rgb(220, 220, 240));
        frame.buffer_mut().set_string(inner.x, inner.y, self.message, msg_style);

        let btn_y = inner.y + 2;
        let selected_style = Style::default()
            .fg(Color::Rgb(20, 20, 30))
            .bg(Color::Rgb(200, 80, 80))
            .add_modifier(Modifier::BOLD);
        let normal_style = Style::default()
            .fg(Color::Rgb(150, 150, 170))
            .add_modifier(Modifier::DIM);
        let (yes_style, no_style) = if self.yes_selected {
            (selected_style, normal_style)
        } else {
            (normal_style, selected_style)
        };
        frame.buffer_mut().set_string(inner.x, btn_y, yes_label, yes_style);
        frame.buffer_mut().set_string(inner.x + yes_label.len() as u16 + 4, btn_y, no_label, no_style);

        let hint_style = Style::default().fg(Color::Rgb(100, 100, 120));
        frame.buffer_mut().set_string(inner.x, inner.y + 4, hint, hint_style);
    }
}

fn render_resume_dialog(frame: &mut ratatui::Frame, area: Rect, session: &Session) {
    use ratatui::widgets::{Block, Borders, Clear, Padding};
    use ratatui::style::{Color, Modifier, Style};

    let bg = Style::default().bg(Color::Rgb(20, 20, 30));
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if let Some(cell) = frame.buffer_mut().cell_mut(ratatui::layout::Position { x, y }) {
                cell.set_symbol(" ");
                cell.set_style(bg);
            }
        }
    }

    let dir_value = display_path(&session.directory);
    let claude_value = session.claude_conversation_id.as_deref().unwrap_or("none");
    let hint = "Press any key to resume this session";

    let dir_line = format!("Directory: {}", dir_value);
    let claude_line = format!("Claude session: {}", claude_value);
    let content_width = dir_line.len()
        .max(claude_line.len())
        .max(hint.len()) as u16;
    // Content: dir, blank, claude, blank, hint = 5 lines + 2 border + 2 padding = 9
    let popup_area = centered_fixed(content_width + 4, 9, area);
    Clear.render(popup_area, frame.buffer_mut());

    let block = Block::default()
        .title(" Disconnected Session ")
        .title_style(Style::default().fg(Color::Rgb(255, 180, 50)).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(100, 100, 140)))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup_area);
    block.render(popup_area, frame.buffer_mut());

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let label_style = Style::default().fg(Color::Rgb(100, 100, 130));
    let value_style = Style::default().fg(Color::Rgb(220, 220, 240));

    let dir_label = "Directory: ";
    frame.buffer_mut().set_string(inner.x, inner.y, dir_label, label_style);
    frame.buffer_mut().set_string(inner.x + dir_label.len() as u16, inner.y, &dir_value, value_style);

    let claude_label = "Claude session: ";
    frame.buffer_mut().set_string(inner.x, inner.y + 2, claude_label, label_style);
    frame.buffer_mut().set_string(inner.x + claude_label.len() as u16, inner.y + 2, claude_value, value_style);

    let hint_len = hint.len() as u16;
    let hint_x = inner.x + inner.width.saturating_sub(hint_len) / 2;
    let hint_style = Style::default()
        .fg(Color::Rgb(150, 150, 200))
        .add_modifier(Modifier::BOLD);
    frame.buffer_mut().set_string(hint_x, inner.y + 4, hint, hint_style);
}

use ratatui::widgets::Widget;

fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    // xterm modifier parameter: 1 + (shift=1, alt=2, ctrl=4)
    let xterm_mod = 1
        + if shift { 1 } else { 0 }
        + if alt { 2 } else { 0 }
        + if ctrl { 4 } else { 0 };
    let has_mod = xterm_mod > 1;

    match key.code {
        // Ctrl+char: control byte (optionally with Alt ESC prefix)
        KeyCode::Char(c) if ctrl => {
            let byte = (c as u8).wrapping_sub(b'a').wrapping_add(1);
            if byte <= 26 {
                if alt { vec![0x1b, byte] } else { vec![byte] }
            } else {
                vec![]
            }
        }
        // Alt+char: ESC prefix
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
        KeyCode::Backspace => if alt { vec![0x1b, 0x7f] } else { vec![0x7f] },
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![0x1b],
        // Cursor keys: unmodified ESC[X, modified ESC[1;<mod>X
        KeyCode::Up    => csi_final(b'A', xterm_mod, has_mod),
        KeyCode::Down  => csi_final(b'B', xterm_mod, has_mod),
        KeyCode::Right => csi_final(b'C', xterm_mod, has_mod),
        KeyCode::Left  => csi_final(b'D', xterm_mod, has_mod),
        KeyCode::Home  => csi_final(b'H', xterm_mod, has_mod),
        KeyCode::End   => csi_final(b'F', xterm_mod, has_mod),
        // Tilde keys: unmodified ESC[<n>~, modified ESC[<n>;<mod>~
        KeyCode::PageUp   => csi_tilde(5, xterm_mod, has_mod),
        KeyCode::PageDown => csi_tilde(6, xterm_mod, has_mod),
        KeyCode::Delete   => csi_tilde(3, xterm_mod, has_mod),
        KeyCode::Insert   => csi_tilde(2, xterm_mod, has_mod),
        // F-keys: F1-4 use SS3 unmodified / CSI 1;<mod> modified; F5+ use tilde form
        KeyCode::F(n) => f_key_bytes(n, xterm_mod, has_mod),
        _ => vec![],
    }
}

/// CSI sequence ending with a letter: ESC[X or ESC[1;<mod>X
fn csi_final(letter: u8, xterm_mod: u8, has_mod: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[1;{}{}", xterm_mod, letter as char).into_bytes()
    } else {
        vec![0x1b, b'[', letter]
    }
}

/// CSI tilde sequence: ESC[<n>~ or ESC[<n>;<mod>~
fn csi_tilde(n: u8, xterm_mod: u8, has_mod: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[{};{}~", n, xterm_mod).into_bytes()
    } else {
        format!("\x1b[{}~", n).into_bytes()
    }
}

/// F-key escape sequences with optional xterm modifier
fn f_key_bytes(n: u8, xterm_mod: u8, has_mod: bool) -> Vec<u8> {
    // F1-F4: SS3 P/Q/R/S unmodified, CSI 1;<mod> P/Q/R/S modified
    if n <= 4 {
        let letter = b'P' + (n - 1);
        if has_mod {
            format!("\x1b[1;{}{}", xterm_mod, letter as char).into_bytes()
        } else {
            vec![0x1b, b'O', letter]
        }
    } else {
        // F5-F12 tilde codes (with the standard gaps at 16 and 22)
        let code: u8 = match n {
            5 => 15, 6 => 17, 7 => 18, 8 => 19,
            9 => 20, 10 => 21, 11 => 23, 12 => 24,
            _ => return vec![],
        };
        csi_tilde(code, xterm_mod, has_mod)
    }
}

/// Rewrite escape sequences that would destroy scrollback in the vt100 parser.
///
/// 1. Strips alternate-screen enters/exits (ESC[?1049h/l, ESC[?47h/l) so all
///    output stays on the primary grid where scrollback works.
/// 2. Replaces ESC[2J (erase display) with ESC[{rows}S ESC[H (scroll up +
///    cursor home). vt100's `erase_all` blanks the grid without pushing rows
///    to scrollback; scroll-up preserves them.
fn rewrite_for_scrollback(input: &[u8], rows: u16) -> Vec<u8> {
    const STRIP: &[&[u8]] = &[
        b"\x1b[?1049h", b"\x1b[?1049l",
        b"\x1b[?47h", b"\x1b[?47l",
    ];
    let scroll_replacement = format!("\x1b[{}S\x1b[H", rows);
    let scroll_bytes = scroll_replacement.as_bytes();
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == 0x1b {
            if let Some(pat) = STRIP.iter().find(|p| input[i..].starts_with(p)) {
                i += pat.len();
                continue;
            }
            if input[i..].starts_with(b"\x1b[2J") {
                out.extend_from_slice(scroll_bytes);
                i += 4;
                continue;
            }
        }
        out.push(input[i]);
        i += 1;
    }
    out
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
        app.refresh_nav_layout();

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
                    app.last_term_width = cols;
                    app.selection = None;
                    app.refresh_nav_layout();
                    // Resize all active PTY sessions
                    for pty in app.pty_sessions.iter().flatten() {
                        let _ = pty.resize(session_rows, cols);
                    }
                    // Also resize all vt parsers
                    for parser in &mut app.vt_parsers {
                        parser.screen_mut().set_size(session_rows, cols);
                    }
                }
                Event::Paste(text) => {
                    if let Mode::Session(idx) = app.mode
                        && let Some(ref pty) = app.pty_sessions[idx] {
                            // Wrap in bracketed paste sequences so the child app
                            // (e.g. Claude Code) treats it as a single paste event
                            let mut buf = Vec::with_capacity(text.len() + 12);
                            buf.extend_from_slice(b"\x1b[200~");
                            buf.extend_from_slice(text.as_bytes());
                            buf.extend_from_slice(b"\x1b[201~");
                            let _ = pty.write(&buf);
                        }
                }
                Event::Mouse(mouse) => {
                    // Status bar clicks — all modes
                    if mouse.row == app.status_bar_area.y
                        && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
                    {
                        let size = terminal.size()?;
                        let active = match app.mode { Mode::Session(i) => Some(i), _ => None };
                        match tab_at_x(&app.sessions, active, app.status_bar_area, mouse.column) {
                            Some(TabHit::Tab(pos)) => {
                                let order = session_order(&app.sessions);
                                if let Some(&sess_idx) = order.get(pos) {
                                    app.reordering = false;
                                    if let Some(Some(pty)) = app.pty_sessions.get(sess_idx) {
                                        let pty_rows = size.height.saturating_sub(1);
                                        let _ = pty.resize(pty_rows, size.width);
                                    }
                                    app.last_session = Some(sess_idx);
                                    app.mode = Mode::Session(sess_idx);
                                }
                            }
                            Some(TabHit::ScrollLeft) => {
                                let order = session_order(&app.sessions);
                                let (vis_start, _) = tab_visible_range(
                                    &app.sessions, active, app.status_bar_area,
                                );
                                if vis_start > 0 {
                                    let pos = vis_start - 1;
                                    if let Some(&sess_idx) = order.get(pos) {
                                        app.reordering = false;
                                        if let Some(Some(pty)) = app.pty_sessions.get(sess_idx) {
                                            let pty_rows = size.height.saturating_sub(1);
                                            let _ = pty.resize(pty_rows, size.width);
                                        }
                                        app.last_session = Some(sess_idx);
                                        app.mode = Mode::Session(sess_idx);
                                    }
                                }
                            }
                            Some(TabHit::ScrollRight) => {
                                let order = session_order(&app.sessions);
                                let (_, vis_end) = tab_visible_range(
                                    &app.sessions, active, app.status_bar_area,
                                );
                                if let Some(&sess_idx) = order.get(vis_end) {
                                    app.reordering = false;
                                    if let Some(Some(pty)) = app.pty_sessions.get(sess_idx) {
                                        let pty_rows = size.height.saturating_sub(1);
                                        let _ = pty.resize(pty_rows, size.width);
                                    }
                                    app.last_session = Some(sess_idx);
                                    app.mode = Mode::Session(sess_idx);
                                }
                            }
                            Some(TabHit::Dashboard) => {
                                if let Mode::Session(idx) = app.mode {
                                    app.last_session = Some(idx);
                                }
                                app.mode = Mode::Dashboard;
                            }
                            None => {}
                        }
                    }
                    // Dashboard clicks
                    else if matches!(app.mode, Mode::Dashboard) {
                        match mouse.kind {
                            MouseEventKind::ScrollUp => {
                                app.nav.scroll_up();
                            }
                            MouseEventKind::ScrollDown => {
                                app.nav.scroll_down();
                            }
                            MouseEventKind::Down(MouseButton::Left) => {
                                let groups = group_by_project(&app.sessions);
                                let group_sizes: Vec<usize> = groups.iter().map(|g| g.sessions.len()).collect();
                                let content_area = Rect {
                                    x: app.dashboard_area.x,
                                    y: app.dashboard_area.y + 1,
                                    width: app.dashboard_area.width,
                                    height: app.dashboard_area.height.saturating_sub(2),
                                };
                                let (card_height, row_stride) = card_metrics();
                                if let Some(pos) = session_at_position(
                                    &group_sizes,
                                    content_area,
                                    app.nav.scroll_row(),
                                    card_height,
                                    row_stride,
                                    mouse.row,
                                    mouse.column,
                                ) {
                                    // Detect double-click
                                    let now = Instant::now();
                                    let is_multi = app.last_click.as_ref().is_some_and(|&(t, r, c)| {
                                        now.duration_since(t) < Duration::from_millis(500)
                                            && r == mouse.row && c == mouse.column
                                    });
                                    if is_multi && app.click_count == 1 {
                                        // Double-click — enter session
                                        app.click_count = 2;
                                        let order = session_order(&app.sessions);
                                        if let Some(&sess_idx) = order.get(pos) {
                                            let size = terminal.size()?;
                                            if let Some(Some(pty)) = app.pty_sessions.get(sess_idx) {
                                                let pty_rows = size.height.saturating_sub(1);
                                                let _ = pty.resize(pty_rows, size.width);
                                            }
                                            app.last_session = Some(sess_idx);
                                            app.mode = Mode::Session(sess_idx);
                                        }
                                    } else {
                                        app.click_count = 1;
                                        app.nav.set_selected(pos);
                                        app.nav.ensure_selection_visible();
                                    }
                                    app.last_click = Some((now, mouse.row, mouse.column));
                                } else {
                                    app.click_count = 0;
                                    app.last_click = None;
                                }
                            }
                            _ => {}
                        }
                    }
                    // Session view mouse handling
                    else if let Mode::Session(idx) = app.mode
                        && idx < app.vt_parsers.len() {
                            let area = app.session_area;
                            let screen = app.vt_parsers[idx].screen();
                            let scrollback = screen.scrollback();
                            match mouse.kind {
                                MouseEventKind::ScrollUp => {
                                    let new = scrollback.saturating_add(3);
                                    if new != scrollback {
                                        app.vt_parsers[idx].screen_mut().set_scrollback(new);
                                    }
                                }
                                MouseEventKind::ScrollDown => {
                                    let new = scrollback.saturating_sub(3);
                                    if new != scrollback {
                                        app.vt_parsers[idx].screen_mut().set_scrollback(new);
                                    }
                                }
                                MouseEventKind::Down(MouseButton::Left) => {
                                    app.selection = None;
                                    if mouse.row >= area.y
                                        && mouse.row < area.y + area.height
                                        && mouse.column >= area.x
                                        && mouse.column < area.x + area.width
                                    {
                                        let (abs_row, col) = selection::mouse_to_abs(
                                            mouse.row, mouse.column,
                                            area.y, area.x, area.height, area.width,
                                            scrollback,
                                        );
                                        // Detect multi-click
                                        let now = Instant::now();
                                        let is_multi = app.last_click.as_ref().is_some_and(|&(t, r, c)| {
                                            now.duration_since(t) < Duration::from_millis(500)
                                                && r == mouse.row && c == mouse.column
                                        });
                                        if is_multi && app.click_count == 1 {
                                            // Double-click — select word
                                            app.click_count = 2;
                                            app.selection = Some(selection::select_word(
                                                app.vt_parsers[idx].screen_mut(), abs_row, col,
                                            ));
                                        } else if is_multi && app.click_count == 2 {
                                            // Triple-click — select line
                                            app.click_count = 3;
                                            let (_, cols) = app.vt_parsers[idx].screen().size();
                                            app.selection = Some(selection::select_line(abs_row, cols));
                                        } else {
                                            app.click_count = 1;
                                            app.selection = Some(Selection::new(abs_row, col));
                                        }
                                        app.last_click = Some((now, mouse.row, mouse.column));
                                    }
                                }
                                MouseEventKind::Drag(MouseButton::Left) => {
                                    if let Some(ref mut sel) = app.selection {
                                        sel.dragged = true;
                                        // Edge auto-scroll
                                        if mouse.row < area.y {
                                            let new = scrollback.saturating_add(1);
                                            app.vt_parsers[idx].screen_mut().set_scrollback(new);
                                        } else if mouse.row >= area.y + area.height {
                                            let new = scrollback.saturating_sub(1);
                                            app.vt_parsers[idx].screen_mut().set_scrollback(new);
                                        }
                                        let current_sb = app.vt_parsers[idx].screen().scrollback();
                                        let (abs_row, col) = selection::mouse_to_abs(
                                            mouse.row, mouse.column,
                                            area.y, area.x, area.height, area.width,
                                            current_sb,
                                        );
                                        sel.moving = (abs_row, col);
                                    }
                                }
                                MouseEventKind::Up(MouseButton::Left) => {
                                    // Clear empty selections (click without drag)
                                    if app.selection.as_ref().is_some_and(|s| s.is_empty()) {
                                        app.selection = None;
                                    }
                                }
                                _ => {}
                            }
                        }
                }
                _ => {}
            }
        }

        // Process PTY output
        app.process_pty_output();
        app.update_stats();

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
