use std::fs;
use std::path::Path;

use summoner::hooks::{read_hook_state, state_file_mtime};
use summoner::session::SessionState;
use tempfile::TempDir;

const PID: u32 = 4242;

fn write_state(dir: &Path, content: &str) {
    let states = dir.join("claude-states");
    fs::create_dir_all(&states).unwrap();
    fs::write(states.join(PID.to_string()), content).unwrap();
}

fn add_subagent_marker(dir: &Path) {
    let agents = dir.join("claude-states").join(format!("{}.agents", PID));
    fs::create_dir_all(&agents).unwrap();
    fs::write(agents.join("agent-1"), "").unwrap();
}

fn state_for(dir: &Path, content: &str) -> Option<SessionState> {
    write_state(dir, content);
    read_hook_state(dir, PID).0
}

#[test]
fn idle_notification_keeps_session_idle() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(state_for(tmp.path(), "Notification sid idle_prompt"), Some(SessionState::Idle));
}

#[test]
fn permission_prompts_report_waiting() {
    let tmp = TempDir::new().unwrap();
    for notif in ["permission_prompt", "agent_needs_input", "elicitation_dialog"] {
        let content = format!("Notification sid {}", notif);
        assert_eq!(state_for(tmp.path(), &content), Some(SessionState::Waiting), "{}", notif);
    }
}

#[test]
fn unactionable_notification_leaves_state_unchanged() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(state_for(tmp.path(), "Notification sid auth_success"), None);
}

#[test]
fn compaction_reports_working() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(state_for(tmp.path(), "PreCompact sid auto"), Some(SessionState::Working));
    assert_eq!(state_for(tmp.path(), "PreCompact sid manual"), Some(SessionState::Working));
    assert_eq!(state_for(tmp.path(), "PostCompact sid auto"), Some(SessionState::Working));
}

#[test]
fn manual_compaction_returns_to_idle() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(state_for(tmp.path(), "PostCompact sid manual"), Some(SessionState::Idle));
}

#[test]
fn manual_compaction_with_subagent_stays_working() {
    let tmp = TempDir::new().unwrap();
    add_subagent_marker(tmp.path());
    assert_eq!(state_for(tmp.path(), "PostCompact sid manual"), Some(SessionState::Working));
}

#[test]
fn stop_reports_idle_unless_subagents_active() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(state_for(tmp.path(), "Stop sid"), Some(SessionState::Idle));
    add_subagent_marker(tmp.path());
    assert_eq!(state_for(tmp.path(), "Stop sid"), Some(SessionState::Working));
}

#[test]
fn tool_events_report_working() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(state_for(tmp.path(), "PreToolUse sid Bash"), Some(SessionState::Working));
    assert_eq!(state_for(tmp.path(), "PostToolUse sid Bash"), Some(SessionState::Working));
    assert_eq!(state_for(tmp.path(), "UserPromptSubmit sid"), Some(SessionState::Working));
}

#[test]
fn session_id_and_active_tool_are_reported() {
    let tmp = TempDir::new().unwrap();
    write_state(tmp.path(), "PreToolUse abc-123 Bash");
    let (_, session_id, active_tool) = read_hook_state(tmp.path(), PID);
    assert_eq!(session_id.as_deref(), Some("abc-123"));
    assert_eq!(active_tool.as_deref(), Some("Bash"));
}

#[test]
fn session_end_clears_state_file() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(state_for(tmp.path(), "SessionEnd sid"), None);
    assert!(!tmp.path().join("claude-states").join(PID.to_string()).exists());
}

#[test]
fn state_file_mtime_tracks_written_file() {
    let tmp = TempDir::new().unwrap();
    assert!(state_file_mtime(tmp.path(), PID).is_none());
    write_state(tmp.path(), "Stop sid");
    assert!(state_file_mtime(tmp.path(), PID).is_some());
}
