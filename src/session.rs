use ratatui::style::Color;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Working,
    Waiting,
    Idle,
    Sleeping,
    Disconnected,
    ShellOnly,
}

impl SessionState {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Working => "\u{26a1}",     // ⚡
            Self::Waiting => "\u{1f4ac}",    // 💬
            Self::Idle => "\u{1f4a4}",       // 💤
            Self::Sleeping => "\u{1f319}",   // 🌙
            Self::Disconnected => "\u{274c}", // ❌
            Self::ShellOnly => "\u{1f4bb}",  // 💻
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Working => "Working",
            Self::Waiting => "Waiting",
            Self::Idle => "Idle",
            Self::Sleeping => "Sleeping",
            Self::Disconnected => "Disconnected",
            Self::ShellOnly => "Shell",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Working => Color::Rgb(0, 200, 120),
            Self::Waiting => Color::Rgb(255, 180, 50),
            Self::Idle => Color::Rgb(100, 120, 220),
            Self::Sleeping => Color::Rgb(80, 90, 120),
            Self::Disconnected => Color::Rgb(100, 100, 100),
            Self::ShellOnly => Color::Rgb(200, 200, 210),
        }
    }
}

pub struct Session {
    pub id: Uuid,
    pub directory: String,
    pub creature_seed: u64,
    pub creature_template: String,
    pub state: SessionState,
    pub claude_conversation_id: Option<String>,
    pub name: String,
}

impl Session {
    pub fn new(directory: String, creature_seed: u64, creature_template: String) -> Self {
        let name = std::path::Path::new(&directory)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Self {
            id: Uuid::new_v4(),
            directory,
            creature_seed,
            creature_template,
            state: SessionState::ShellOnly,
            claude_conversation_id: None,
            name,
        }
    }
}

/// Groups sessions by their directory for dashboard display.
pub struct ProjectGroup {
    pub directory: String,
    pub name: String,
    pub sessions: Vec<usize>, // indices into the session list
}

pub fn group_by_project(sessions: &[Session]) -> Vec<ProjectGroup> {
    let mut groups: Vec<ProjectGroup> = Vec::new();

    for (idx, session) in sessions.iter().enumerate() {
        if let Some(group) = groups.iter_mut().find(|g| g.directory == session.directory) {
            group.sessions.push(idx);
        } else {
            groups.push(ProjectGroup {
                directory: session.directory.clone(),
                name: session.name.clone(),
                sessions: vec![idx],
            });
        }
    }

    groups
}

/// RPG stats for a session, updated from JSONL and hooks.
pub struct SessionStats {
    pub total_tokens: u64,
    pub message_count: u32,
    pub active_tool: Option<String>,
    pub context_pct: Option<u8>,
    pub jsonl_offset: u64,
    pub jsonl_path: Option<std::path::PathBuf>,
}

impl Default for SessionStats {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStats {
    pub fn new() -> Self {
        Self {
            total_tokens: 0,
            message_count: 0,
            active_tool: None,
            context_pct: None,
            jsonl_offset: 0,
            jsonl_path: None,
        }
    }
}

/// Global usage stats shown in the dashboard status bar.
pub struct GlobalStats {
    pub daily_messages: u32,
    pub daily_tokens: u64,
    pub five_hour_pct: Option<u8>,
    pub five_hour_resets_at: Option<i64>,
    pub seven_day_pct: Option<u8>,
    pub seven_day_resets_at: Option<i64>,
    pub last_daily_reset: chrono::NaiveDate,
}

impl Default for GlobalStats {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalStats {
    pub fn new() -> Self {
        Self {
            daily_messages: 0,
            daily_tokens: 0,
            five_hour_pct: None,
            five_hour_resets_at: None,
            seven_day_pct: None,
            seven_day_resets_at: None,
            last_daily_reset: chrono::Local::now().date_naive(),
        }
    }

    pub fn check_daily_reset(&mut self) {
        let today = chrono::Local::now().date_naive();
        if today != self.last_daily_reset {
            self.daily_messages = 0;
            self.daily_tokens = 0;
            self.last_daily_reset = today;
        }
    }
}

/// Returns flat-position → session-index mapping in appearance order (grouped by project).
/// Position 0 is F1, position 1 is F2, etc.
pub fn session_order(sessions: &[Session]) -> Vec<usize> {
    let groups = group_by_project(sessions);
    let mut order = Vec::new();
    for group in &groups {
        order.extend(&group.sessions);
    }
    order
}
