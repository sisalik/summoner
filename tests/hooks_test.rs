use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use summoner::hooks::{
    configure_settings_file, hook_script, read_hook_state, state_file_mtime,
    unconfigure_settings_file,
};
use summoner::session::SessionState;
use tempfile::TempDir;

const KEY: &str = "sess-4242";

fn states_dir(dir: &Path) -> std::path::PathBuf {
    dir.join("claude-states")
}

fn write_state(dir: &Path, content: &str) {
    let states = states_dir(dir);
    fs::create_dir_all(&states).unwrap();
    fs::write(states.join(KEY), content).unwrap();
}

fn add_subagent_marker(dir: &Path) {
    let agents = states_dir(dir).join(format!("{}.agents", KEY));
    fs::create_dir_all(&agents).unwrap();
    fs::write(agents.join("agent-1"), "").unwrap();
}

fn state_for(dir: &Path, content: &str) -> Option<SessionState> {
    write_state(dir, content);
    read_hook_state(dir, KEY).0
}

// --- read_hook_state -------------------------------------------------------

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
    let (_, session_id, active_tool) = read_hook_state(tmp.path(), KEY);
    assert_eq!(session_id.as_deref(), Some("abc-123"));
    assert_eq!(active_tool.as_deref(), Some("Bash"));
}

#[test]
fn session_end_clears_state_file() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(state_for(tmp.path(), "SessionEnd sid"), None);
    assert!(!states_dir(tmp.path()).join(KEY).exists());
}

#[test]
fn state_file_mtime_tracks_written_file() {
    let tmp = TempDir::new().unwrap();
    assert!(state_file_mtime(tmp.path(), KEY).is_none());
    write_state(tmp.path(), "Stop sid");
    assert!(state_file_mtime(tmp.path(), KEY).is_some());
}

// --- the hook script itself, run under bash --------------------------------
//
// The hook reads only a bounded prefix of stdin and must never depend on the
// payload's size. Key order below mirrors what Claude Code sends: the common
// fields first, then the event-specific ones, with tool_response last.

/// A hook payload in Claude Code's shape. `tail` is appended raw after the
/// event fields (a tool_response, for instance).
fn payload(event: &str, fields: &[(&str, &str)], tail: &str) -> String {
    let mut s = format!(
        "{{\"session_id\":\"abc-123\",\"transcript_path\":\"/t/abc-123.jsonl\",\
         \"cwd\":\"/home/u/proj\",\"permission_mode\":\"default\",\"hook_event_name\":\"{}\"",
        event
    );
    for (k, v) in fields {
        s.push_str(&format!(",\"{}\":\"{}\"", k, v));
    }
    s.push_str(tail);
    s.push('}');
    s
}

fn tool_payload(event: &str, tool: &str, response_len: usize) -> String {
    let tail = format!(
        ",\"tool_input\":{{\"file_path\":\"/x\"}},\"tool_response\":\"{}\",\"tool_use_id\":\"toolu_1\"",
        "A".repeat(response_len)
    );
    payload(event, &[("tool_name", tool)], &tail)
}

/// Run the installed hook script with `home` as $HOME. `session` is the value
/// of SUMMONER_SESSION, or None to leave it unset as it is outside Summoner.
fn run_hook(home: &Path, session: Option<&str>, input: &str) -> i32 {
    let script = home.join("claude-state.sh");
    fs::write(&script, hook_script()).unwrap();
    let mut cmd = Command::new("bash");
    cmd.arg(&script)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(s) = session {
        cmd.env("SUMMONER_SESSION", s);
    }
    let mut child = cmd.spawn().expect("bash");
    let mut stdin = child.stdin.take().unwrap();
    let bytes = input.as_bytes().to_vec();
    // The script may exit before reading everything; a broken pipe is expected
    let writer = std::thread::spawn(move || { let _ = stdin.write_all(&bytes); });
    let status = child.wait().unwrap();
    writer.join().unwrap();
    status.code().unwrap_or(-1)
}

fn state_content(home: &Path, key: &str) -> Option<String> {
    fs::read_to_string(home.join(".summoner").join("claude-states").join(key))
        .ok()
        .map(|s| s.trim().to_string())
}

fn summoner_dir(home: &Path) -> std::path::PathBuf {
    home.join(".summoner")
}

#[test]
fn script_records_tool_event_regardless_of_payload_size() {
    let tmp = TempDir::new().unwrap();
    for len in [0, 9_000, 5_000_000] {
        assert_eq!(run_hook(tmp.path(), Some(KEY), &tool_payload("PostToolUse", "Read", len)), 0);
        assert_eq!(state_content(tmp.path(), KEY).as_deref(), Some("PostToolUse abc-123 Read"), "len {}", len);
    }
    let (state, sid, tool) = read_hook_state(&summoner_dir(tmp.path()), KEY);
    assert_eq!(state, Some(SessionState::Working));
    assert_eq!(sid.as_deref(), Some("abc-123"));
    assert_eq!(tool, None, "only PreToolUse names an active tool");

    assert_eq!(run_hook(tmp.path(), Some(KEY), &tool_payload("PreToolUse", "Bash", 0)), 0);
    let (_, _, tool) = read_hook_state(&summoner_dir(tmp.path()), KEY);
    assert_eq!(tool.as_deref(), Some("Bash"));
}

#[test]
fn script_does_nothing_outside_summoner() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_hook(tmp.path(), None, &tool_payload("PostToolUse", "Read", 100)), 0);
    assert!(!tmp.path().join(".summoner").exists());
}

#[test]
fn script_rejects_a_session_key_that_is_not_a_plain_name() {
    let tmp = TempDir::new().unwrap();
    for key in ["../escape", "a/b", "", "has space"] {
        assert_eq!(run_hook(tmp.path(), Some(key), &tool_payload("Stop", "", 0)), 0);
        assert!(!tmp.path().join(".summoner").exists(), "key {:?}", key);
    }
}

#[test]
fn script_writes_events_without_a_stray_third_field() {
    let tmp = TempDir::new().unwrap();
    run_hook(tmp.path(), Some(KEY), &payload("Stop", &[], ""));
    assert_eq!(state_content(tmp.path(), KEY).as_deref(), Some("Stop abc-123"));
    run_hook(tmp.path(), Some(KEY), &payload("UserPromptSubmit", &[("prompt", "hi")], ""));
    assert_eq!(state_content(tmp.path(), KEY).as_deref(), Some("UserPromptSubmit abc-123"));
    run_hook(tmp.path(), Some(KEY), &payload("SessionStart", &[("source", "startup")], ""));
    assert_eq!(state_content(tmp.path(), KEY).as_deref(), Some("SessionStart abc-123"));
}

#[test]
fn script_filters_notifications_that_do_not_change_state() {
    let tmp = TempDir::new().unwrap();
    run_hook(tmp.path(), Some(KEY), &payload("Stop", &[], ""));
    for notif in ["idle_prompt", "auth_success", "agent_completed"] {
        run_hook(tmp.path(), Some(KEY), &payload("Notification", &[("message", "m"), ("notification_type", notif)], ""));
        assert_eq!(state_content(tmp.path(), KEY).as_deref(), Some("Stop abc-123"), "{}", notif);
    }
    run_hook(tmp.path(), Some(KEY), &payload("Notification", &[("message", "m"), ("notification_type", "permission_prompt")], ""));
    assert_eq!(state_content(tmp.path(), KEY).as_deref(), Some("Notification abc-123 permission_prompt"));
    assert_eq!(read_hook_state(&summoner_dir(tmp.path()), KEY).0, Some(SessionState::Waiting));
}

#[test]
fn script_tracks_subagents_in_marker_files() {
    let tmp = TempDir::new().unwrap();
    let sd = summoner_dir(tmp.path());
    run_hook(tmp.path(), Some(KEY), &payload("Stop", &[], ""));
    assert_eq!(read_hook_state(&sd, KEY).0, Some(SessionState::Idle));

    run_hook(tmp.path(), Some(KEY), &payload("SubagentStart", &[("agent_id", "agent-1"), ("agent_type", "Explore")], ""));
    assert!(sd.join("claude-states").join(format!("{}.agents", KEY)).join("agent-1").exists());
    assert_eq!(state_content(tmp.path(), KEY).as_deref(), Some("Stop abc-123"), "markers leave the state file alone");
    assert_eq!(read_hook_state(&sd, KEY).0, Some(SessionState::Working));

    run_hook(tmp.path(), Some(KEY), &payload("SubagentStop", &[("agent_id", "agent-1")], ""));
    assert!(!sd.join("claude-states").join(format!("{}.agents", KEY)).exists());
    assert_eq!(read_hook_state(&sd, KEY).0, Some(SessionState::Idle));
}

#[test]
fn script_session_end_removes_subagent_markers() {
    let tmp = TempDir::new().unwrap();
    let sd = summoner_dir(tmp.path());
    run_hook(tmp.path(), Some(KEY), &payload("SubagentStart", &[("agent_id", "agent-1")], ""));
    run_hook(tmp.path(), Some(KEY), &payload("SessionEnd", &[("reason", "exit")], ""));
    assert!(!sd.join("claude-states").join(format!("{}.agents", KEY)).exists());
    assert_eq!(read_hook_state(&sd, KEY).0, None);
    assert!(!sd.join("claude-states").join(KEY).exists(), "reading a SessionEnd consumes it");
}

#[test]
fn script_records_compaction_trigger() {
    let tmp = TempDir::new().unwrap();
    run_hook(tmp.path(), Some(KEY), &payload("PreCompact", &[("trigger", "auto"), ("custom_instructions", "")], ""));
    assert_eq!(state_content(tmp.path(), KEY).as_deref(), Some("PreCompact abc-123 auto"));
    run_hook(tmp.path(), Some(KEY), &payload("PostCompact", &[("trigger", "manual")], ""));
    assert_eq!(read_hook_state(&summoner_dir(tmp.path()), KEY).0, Some(SessionState::Idle));
}

#[test]
fn script_ignores_input_that_is_not_a_hook_payload() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_hook(tmp.path(), Some(KEY), ""), 0);
    assert_eq!(run_hook(tmp.path(), Some(KEY), "not json at all"), 0);
    assert_eq!(state_content(tmp.path(), KEY), None);
}

// --- settings.json ---------------------------------------------------------

const OUR_COMMAND: &str = "bash ~/.summoner/hooks/claude-state.sh";

fn our_entries<'a>(settings: &'a serde_json::Value, event: &str) -> Vec<&'a serde_json::Value> {
    settings["hooks"][event]
        .as_array()
        .map(|a| a.iter().filter(|e| e["hooks"][0]["command"] == OUR_COMMAND).collect())
        .unwrap_or_default()
}

fn read_settings(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn settings_gain_every_event_with_matcher_and_timeout() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    assert!(configure_settings_file(&path).unwrap());
    let s = read_settings(&path);
    for event in ["SessionStart", "UserPromptSubmit", "Stop", "Notification", "SessionEnd",
                  "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop", "PreCompact", "PostCompact"] {
        let ours = our_entries(&s, event);
        assert_eq!(ours.len(), 1, "{}", event);
        assert_eq!(ours[0]["hooks"][0]["timeout"], 5, "{}", event);
        assert_eq!(ours[0]["hooks"][0]["type"], "command");
    }
    assert_eq!(our_entries(&s, "Notification")[0]["matcher"], "permission_prompt|agent_needs_input|elicitation_dialog");
    assert_eq!(our_entries(&s, "PreToolUse")[0]["matcher"], "");
    assert!(!configure_settings_file(&path).unwrap(), "second install changes nothing");
}

#[test]
fn settings_keep_foreign_hooks_and_other_keys() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    fs::write(&path, r#"{
        "model": "opus",
        "hooks": {
            "SessionStart": [{"hooks": [{"type": "command", "command": "node check.js"}]}],
            "PostToolUse": [{"matcher": "Write|Edit", "hooks": [{"type": "command", "command": "lint"}]}]
        }
    }"#).unwrap();
    configure_settings_file(&path).unwrap();
    let s = read_settings(&path);
    assert_eq!(s["model"], "opus");
    assert_eq!(s["hooks"]["SessionStart"][0]["hooks"][0]["command"], "node check.js");
    assert_eq!(s["hooks"]["PostToolUse"][0]["matcher"], "Write|Edit");
    assert_eq!(s["hooks"]["PostToolUse"].as_array().unwrap().len(), 2);

    assert!(unconfigure_settings_file(&path).unwrap());
    let s = read_settings(&path);
    assert_eq!(s["model"], "opus");
    assert_eq!(s["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    assert_eq!(s["hooks"]["PostToolUse"].as_array().unwrap().len(), 1);
    assert!(s["hooks"].get("Stop").is_none(), "events that only had our entry are dropped");
}

#[test]
fn settings_upgrade_an_older_entry_in_place() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    // What an earlier Summoner wrote: no matcher filter, no timeout
    fs::write(&path, format!(r#"{{
        "hooks": {{
            "Notification": [
                {{"matcher": "", "hooks": [{{"type": "command", "command": "{0}"}}]}},
                {{"matcher": "", "hooks": [{{"type": "command", "command": "notify-send"}}]}}
            ],
            "Stop": [{{"matcher": "", "hooks": [{{"type": "command", "command": "{0}"}}]}}]
        }}
    }}"#, OUR_COMMAND)).unwrap();
    assert!(configure_settings_file(&path).unwrap());
    let s = read_settings(&path);
    let notif = s["hooks"]["Notification"].as_array().unwrap();
    assert_eq!(notif.len(), 2);
    assert_eq!(notif[0]["matcher"], "permission_prompt|agent_needs_input|elicitation_dialog", "upgraded where it was");
    assert_eq!(notif[0]["hooks"][0]["timeout"], 5);
    assert_eq!(notif[1]["hooks"][0]["command"], "notify-send");
    assert_eq!(our_entries(&s, "Stop").len(), 1);
    assert_eq!(our_entries(&s, "Stop")[0]["hooks"][0]["timeout"], 5);
}

#[test]
fn unconfigure_removes_hooks_key_when_nothing_is_left() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    fs::write(&path, r#"{"model": "opus"}"#).unwrap();
    configure_settings_file(&path).unwrap();
    unconfigure_settings_file(&path).unwrap();
    let s = read_settings(&path);
    assert!(s.get("hooks").is_none());
    assert_eq!(s["model"], "opus");
}
