use summoner::config::{AppConfig, SessionEntry, SessionStore, RecentDirs};
use chrono::Utc;
use uuid::Uuid;
use tempfile::TempDir;

#[test]
fn config_defaults_are_sensible() {
    let config = AppConfig::default();
    assert_eq!(config.general.animation_speed, 1.0);
    assert_eq!(config.general.status_bar_poll_interval, 2);
    assert_eq!(config.general.session_prune_days, 30);
    assert_eq!(config.new_session.recent_dirs_count, 15);
}

#[test]
fn config_round_trips_through_toml() {
    let config = AppConfig::default();
    let toml_str = toml::to_string_pretty(&config).unwrap();
    let loaded: AppConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(loaded.general.animation_speed, config.general.animation_speed);
    assert_eq!(loaded.general.status_bar_poll_interval, config.general.status_bar_poll_interval);
}

#[test]
fn session_store_round_trips_through_toml() {
    let store = SessionStore {
        sessions: vec![
            SessionEntry {
                id: Uuid::new_v4(),
                directory: "/home/test/project".into(),
                creature_seed: 12345,
                creature_template: "bipedal".into(),
                claude_conversation_id: Some("conv-abc".into()),
                last_active: Utc::now(),
                active: false,
            },
        ],
    };
    let toml_str = toml::to_string_pretty(&store).unwrap();
    let loaded: SessionStore = toml::from_str(&toml_str).unwrap();
    assert_eq!(loaded.sessions.len(), 1);
    assert_eq!(loaded.sessions[0].directory, "/home/test/project");
    assert_eq!(loaded.sessions[0].creature_seed, 12345);
    assert!(loaded.sessions[0].claude_conversation_id.is_some());
}

#[test]
fn recent_dirs_round_trips() {
    let dirs = RecentDirs {
        directories: vec!["/home/test/a".into(), "/home/test/b".into()],
    };
    let toml_str = toml::to_string_pretty(&dirs).unwrap();
    let loaded: RecentDirs = toml::from_str(&toml_str).unwrap();
    assert_eq!(loaded.directories, dirs.directories);
}

#[test]
fn save_and_load_config_from_disk() {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig::default();
    config.save(tmp.path()).unwrap();
    let loaded = AppConfig::load(tmp.path()).unwrap();
    assert_eq!(loaded.general.animation_speed, 1.0);
}

#[test]
fn save_and_load_session_store_from_disk() {
    let tmp = TempDir::new().unwrap();
    let store = SessionStore { sessions: vec![] };
    store.save(tmp.path()).unwrap();
    let loaded = SessionStore::load(tmp.path()).unwrap();
    assert!(loaded.sessions.is_empty());
}

#[test]
fn load_missing_config_returns_default() {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig::load(tmp.path()).unwrap();
    assert_eq!(config.general.animation_speed, 1.0);
}
