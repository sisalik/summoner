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
            Self::Working => "⚡",
            Self::Waiting => "❓",
            Self::Idle => "◆",
            Self::Sleeping => "☽",
            Self::Disconnected => "✕",
            Self::ShellOnly => "▸",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Idle => "idle",
            Self::Sleeping => "sleeping",
            Self::Disconnected => "disconnected",
            Self::ShellOnly => "shell",
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
