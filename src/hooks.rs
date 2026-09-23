use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::claude_settings;
use crate::session::SessionState;

/// Environment variable Summoner exports into every PTY it spawns, carrying
/// the session's id. Claude Code passes its environment on to hook commands,
/// so the hook script uses it both to find the state file to write and to
/// exit immediately when Claude Code was launched outside Summoner.
pub const SESSION_ENV: &str = "SUMMONER_SESSION";

/// The hook runs on Claude Code's critical path — PreToolUse blocks the tool,
/// PostToolUse blocks the result, UserPromptSubmit blocks the prompt — so it is
/// written to spawn no external process at all on the common path: bash
/// builtins read a bounded prefix of the payload and match fields with the
/// regex engine, which is linear. A PostToolUse payload embeds the whole tool
/// response (megabytes for an image read), and the fields the hook needs all
/// precede it, so it reads 4 KB and never looks at the rest. Pattern
/// expansion (`${x##*pat}`) is avoided on purpose: it is quadratic in the
/// input, and cost a quarter of a second on an 8 KB slice.
const HOOK_SCRIPT: &str = r##"#!/bin/bash
# Summoner Claude Code hook — reports state changes via file.
# Kept free of external processes on the common path; see src/hooks.rs.

# Launched outside Summoner: nothing to report, and don't read the payload
[[ $SUMMONER_SESSION =~ ^[A-Za-z0-9_-]+$ ]] || exit 0

# Only the rare branches below spawn a process; keep them off the Windows
# PATH entries WSL appends, which cost a drvfs stat each on a miss
PATH=/usr/local/bin:/usr/bin:/bin

# The fields we need precede tool_input/tool_response, so 4 KB is enough
IFS= read -r -N 4096 input

[[ $input =~ \"hook_event_name\":\"([^\"]*)\" ]] || exit 0
event=${BASH_REMATCH[1]}
sid=
[[ $input =~ \"session_id\":\"([^\"]*)\" ]] && sid=${BASH_REMATCH[1]}

# Third field carries the event-specific payload
extra=
case $event in
    PreToolUse|PostToolUse)
        [[ $input =~ \"tool_name\":\"([^\"]*)\" ]] && extra=${BASH_REMATCH[1]} ;;
    Notification)
        [[ $input =~ \"notification_type\":\"([^\"]*)\" ]] && extra=${BASH_REMATCH[1]}
        # Notifications that don't change session state would otherwise clobber
        # the last meaningful event (idle_prompt fires periodically and would
        # hide Stop). The settings matcher filters these too; this is a backstop.
        case $extra in
            permission_prompt|agent_needs_input|elicitation_dialog) ;;
            *) exit 0 ;;
        esac ;;
    SubagentStart|SubagentStop)
        [[ $input =~ \"agent_id\":\"([^\"]*)\" ]] && extra=${BASH_REMATCH[1]} ;;
    PreCompact|PostCompact)
        [[ $input =~ \"trigger\":\"([^\"]*)\" ]] && extra=${BASH_REMATCH[1]} ;;
    StopFailure)
        [[ $input =~ \"error\":\"([^\"]*)\" ]] && extra=${BASH_REMATCH[1]} ;;
esac

dir="$HOME/.summoner/claude-states"
[ -d "$dir" ] || mkdir -p "$dir" 2>/dev/null
state="$dir/$SUMMONER_SESSION"
agents="$state.agents"

case $event in
    # Track active subagents via marker files (don't touch main state file)
    SubagentStart)
        [ -n "$extra" ] || exit 0
        [ -d "$agents" ] || mkdir -p "$agents" 2>/dev/null
        : > "$agents/$extra" ;;
    SubagentStop)
        [ -n "$extra" ] || exit 0
        rm -f "$agents/$extra"
        rmdir "$agents" 2>/dev/null ;;
    SessionEnd)
        echo "$event $sid $extra" > "$state"
        rm -rf "$agents" ;;
    *)
        echo "$event $sid $extra" > "$state" ;;
esac
exit 0
"##;

const HOOK_COMMAND: &str = "bash ~/.summoner/hooks/claude-state.sh";

/// Upper bound on a single hook run. The script takes a few milliseconds;
/// Claude Code's default is ten minutes, and a wedged hook would stall the
/// tool call it is attached to for that long.
const HOOK_TIMEOUT_SECS: u64 = 5;

/// Events the hook subscribes to, with the matcher Claude Code applies before
/// spawning it. Only three notification types change a session's state, so
/// the rest (idle_prompt above all, which fires periodically) never cost a
/// process.
const HOOK_EVENTS: &[(&str, &str)] = &[
    ("SessionStart", ""),
    ("UserPromptSubmit", ""),
    ("Stop", ""),
    ("StopFailure", ""),
    ("Notification", "permission_prompt|agent_needs_input|elicitation_dialog"),
    ("SessionEnd", ""),
    ("PreToolUse", ""),
    ("PostToolUse", ""),
    ("SubagentStart", ""),
    ("SubagentStop", ""),
    ("PreCompact", ""),
    ("PostCompact", ""),
];

/// The hook script as installed, for tests that exercise it under bash.
pub fn hook_script() -> &'static str {
    HOOK_SCRIPT
}

/// Install the hook script and configure ~/.claude/settings.json.
/// Also cleans up stale state files from prior runs.
///
/// Returns one line per step that failed. Nothing here is fatal — Summoner
/// still works as a session manager without state detection — but a silent
/// failure would leave every creature asleep with no explanation.
pub fn install_hooks(summoner_dir: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    if let Err(e) = install_hook_script(summoner_dir) {
        problems.push(format!("hook script not installed: {}", e));
    }
    if let Err(e) = configure_claude_settings() {
        problems.push(format!("~/.claude/settings.json not updated: {}", e));
    }
    clear_all_state_files(summoner_dir);
    if let Err(e) = crate::statusline::install_wrapper(summoner_dir) {
        problems.push(format!("statusLine wrapper not installed: {}", e));
    }
    problems
}

/// Remove hook entries from ~/.claude/settings.json, delete scripts and state files.
pub fn uninstall_hooks(summoner_dir: &Path) {
    let _ = unconfigure_claude_settings();
    crate::statusline::uninstall_wrapper();
    // Remove hook scripts
    let hooks_dir = summoner_dir.join("hooks");
    let _ = fs::remove_file(hooks_dir.join("claude-state.sh"));
    let _ = fs::remove_file(hooks_dir.join("statusline-wrapper.sh"));
    let _ = fs::remove_dir(&hooks_dir); // only succeeds if empty
    // Remove state directories
    clear_all_state_files(summoner_dir);
    let _ = fs::remove_dir(summoner_dir.join("claude-states"));
    crate::statusline::clear_stale_files(summoner_dir);
    let _ = fs::remove_dir(summoner_dir.join("statusline-states"));
}

/// Remove all state files. Called on startup so nothing from a previous run
/// is mistaken for a live session.
pub fn clear_all_state_files(summoner_dir: &Path) {
    let dir = summoner_dir.join("claude-states");
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let _ = fs::remove_dir_all(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// Remove the state file and subagent markers for a session.
pub fn clear_state_file(summoner_dir: &Path, session_key: &str) {
    let _ = fs::remove_file(state_file_path(summoner_dir, session_key));
    let _ = fs::remove_dir_all(agents_dir_path(summoner_dir, session_key));
}

fn install_hook_script(summoner_dir: &Path) -> std::io::Result<()> {
    let hooks_dir = summoner_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;
    let script_path = hooks_dir.join("claude-state.sh");
    // Skip write if content already matches
    if fs::read_to_string(&script_path).ok().as_deref() == Some(HOOK_SCRIPT) {
        return Ok(());
    }
    fs::write(&script_path, HOOK_SCRIPT)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn configure_claude_settings() -> std::io::Result<()> {
    configure_settings_file(&claude_settings::path()?).map(|_| ())
}

fn is_our_entry(entry: &serde_json::Value) -> bool {
    entry.get("hooks")
        .and_then(|h| h.as_array())
        .map(|hooks| hooks.iter().any(|h| {
            h.get("command").and_then(|c| c.as_str()) == Some(HOOK_COMMAND)
        }))
        .unwrap_or(false)
}

fn our_entry(matcher: &str) -> serde_json::Value {
    serde_json::json!({
        "matcher": matcher,
        "hooks": [{
            "type": "command",
            "command": HOOK_COMMAND,
            "timeout": HOOK_TIMEOUT_SECS,
        }]
    })
}

/// Add Summoner's hook entries to a Claude Code settings file, replacing any
/// earlier version of them (an older install had no matcher or timeout) and
/// leaving everything else untouched. Returns whether the file was written.
pub fn configure_settings_file(settings_path: &Path) -> std::io::Result<bool> {
    let mut settings = claude_settings::read(settings_path)?;
    let original = settings.clone();

    let hooks = settings
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let hooks_obj = match hooks.as_object_mut() {
        Some(obj) => obj,
        None => return Ok(false),
    };

    for &(event, matcher) in HOOK_EVENTS {
        let event_hooks = hooks_obj
            .entry(event)
            .or_insert_with(|| serde_json::json!([]));

        let arr = match event_hooks.as_array_mut() {
            Some(a) => a,
            None => continue,
        };

        // Replace in place so a stale entry is upgraded and keeps its position
        let wanted = our_entry(matcher);
        let first = arr.iter().position(is_our_entry);
        arr.retain(|e| !is_our_entry(e));
        match first {
            Some(pos) => arr.insert(pos.min(arr.len()), wanted),
            None => arr.push(wanted),
        }
    }

    if settings == original {
        return Ok(false);
    }
    claude_settings::write(settings_path, &settings)?;
    Ok(true)
}

fn unconfigure_claude_settings() -> std::io::Result<()> {
    unconfigure_settings_file(&claude_settings::path()?).map(|_| ())
}

/// Remove Summoner's hook entries from a Claude Code settings file, leaving
/// everything else untouched. Returns whether the file was written.
pub fn unconfigure_settings_file(settings_path: &Path) -> std::io::Result<bool> {
    if !settings_path.exists() {
        return Ok(false);
    }
    let mut settings = claude_settings::read(settings_path)?;
    let original = settings.clone();

    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for &(event, _) in HOOK_EVENTS {
            if let Some(arr) = hooks.get_mut(event).and_then(|v| v.as_array_mut()) {
                arr.retain(|entry| !is_our_entry(entry));
            }
        }
        // Clean up empty event arrays
        hooks.retain(|_, v| !v.as_array().map(|a| a.is_empty()).unwrap_or(false));
    }
    if settings.get("hooks").and_then(|h| h.as_object()).map(|o| o.is_empty()).unwrap_or(false) {
        settings.remove("hooks");
    }

    if settings == original {
        return Ok(false);
    }
    claude_settings::write(settings_path, &settings)?;
    Ok(true)
}

fn state_file_path(summoner_dir: &Path, session_key: &str) -> PathBuf {
    summoner_dir.join("claude-states").join(session_key)
}

fn agents_dir_path(summoner_dir: &Path, session_key: &str) -> PathBuf {
    summoner_dir.join("claude-states").join(format!("{}.agents", session_key))
}

/// Last time a hook event was recorded for a session.
pub fn state_file_mtime(summoner_dir: &Path, session_key: &str) -> Option<SystemTime> {
    fs::metadata(state_file_path(summoner_dir, session_key))
        .and_then(|m| m.modified())
        .ok()
}

/// Count active subagents for a session by counting marker files.
fn active_subagent_count(summoner_dir: &Path, session_key: &str) -> usize {
    fs::read_dir(agents_dir_path(summoner_dir, session_key))
        .map(|entries| entries.count())
        .unwrap_or(0)
}

/// Read hook state and session ID in a single file read.
/// Returns (state, session_id, active_tool) where state is None if no hook data or Claude exited.
pub fn read_hook_state(summoner_dir: &Path, session_key: &str) -> (Option<SessionState>, Option<String>, Option<String>) {
    let state_file = state_file_path(summoner_dir, session_key);
    let content = match fs::read_to_string(&state_file) {
        Ok(c) => c,
        Err(_) => return (None, None, None),
    };
    let mut parts = content.split_whitespace();
    let event = match parts.next() {
        Some(e) => e,
        None => return (None, None, None),
    };
    let session_id = parts.next().map(|s| s.to_string());
    let tool_name = parts.next().map(|s| s.to_string());

    let has_active_subagents = active_subagent_count(summoner_dir, session_key) > 0;

    let idle_or_working = |has_active_subagents: bool| {
        if has_active_subagents {
            Some(SessionState::Working)
        } else {
            Some(SessionState::Idle)
        }
    };

    let state = match event {
        "UserPromptSubmit" => Some(SessionState::Working),
        "Stop" | "SessionStart" => idle_or_working(has_active_subagents),
        // Fired instead of Stop when an API error ended the turn. It wins over
        // running subagents: the main thread is stuck until the user acts.
        "StopFailure" => Some(SessionState::Errored),
        "Notification" => match tool_name.as_deref() {
            // agent_needs_input covers permission prompts bubbling up from subagents
            Some("permission_prompt" | "agent_needs_input" | "elicitation_dialog") => {
                Some(SessionState::Waiting)
            }
            // A pre-whitelist hook script may still write these
            Some("idle_prompt") => Some(SessionState::Idle),
            _ => None,
        },
        "PreToolUse" => Some(SessionState::Working),
        "PostToolUse" => Some(SessionState::Working),
        // Auto-compaction happens mid-turn, so more events follow; manual /compact
        // is issued from an idle prompt and returns there
        "PreCompact" => Some(SessionState::Working),
        "PostCompact" => {
            if tool_name.as_deref() == Some("auto") {
                Some(SessionState::Working)
            } else {
                idle_or_working(has_active_subagents)
            }
        }
        "SessionEnd" => {
            let _ = fs::remove_file(&state_file);
            return (None, None, None);
        }
        _ => None,
    };

    let active_tool = match event {
        "PreToolUse" => tool_name,
        _ => None,
    };

    (state, session_id, active_tool)
}
