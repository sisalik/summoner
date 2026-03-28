use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub new_session: NewSessionConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub default_shell: String,
    pub animation_speed: f64,
    pub status_bar_poll_interval: u64,
    pub session_prune_days: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewSessionConfig {
    pub recent_dirs_count: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                default_shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into()),
                animation_speed: 1.0,
                status_bar_poll_interval: 2,
                session_prune_days: 30,
            },
            new_session: NewSessionConfig {
                recent_dirs_count: 15,
            },
        }
    }
}

impl AppConfig {
    pub fn save(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir)?;
        let path = dir.join("config.toml");
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionStore {
    #[serde(rename = "session", default)]
    pub sessions: Vec<SessionEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionEntry {
    pub id: Uuid,
    pub directory: String,
    pub creature_seed: u64,
    pub creature_template: String,
    pub claude_conversation_id: Option<String>,
    pub last_active: DateTime<Utc>,
    pub active: bool,
}

impl SessionStore {
    pub fn save(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir)?;
        let path = dir.join("sessions.toml");
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("sessions.toml");
        if !path.exists() {
            return Ok(Self { sessions: vec![] });
        }
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecentDirs {
    #[serde(rename = "directory", default)]
    pub directories: Vec<String>,
}

impl RecentDirs {
    pub fn save(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir)?;
        let path = dir.join("recent_dirs.toml");
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("recent_dirs.toml");
        if !path.exists() {
            return Ok(Self { directories: vec![] });
        }
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn add(&mut self, dir: String, max: usize) {
        self.directories.retain(|d| d != &dir);
        self.directories.insert(0, dir);
        self.directories.truncate(max);
    }
}
