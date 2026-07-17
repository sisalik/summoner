use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::session::SessionState;

const HOOK_SCRIPT: &str = r#"#!/bin/bash
# Summoner Claude Code hook — reports state changes via file
input=$(cat)

# Extract fields with simple parameter expansion (no jq needed)
extract() {
    local tmp="${input##*"\"$1\":\""}"
    printf '%s' "${tmp%%\"*}"
}

event=$(extract hook_event_name)
sid=$(extract session_id)
tool=$(extract tool_name)
notif_type=$(extract notification_type)
agent_id=$(extract agent_id)
trigger=$(extract trigger)

# Notifications that don't change session state would otherwise clobber the last
# meaningful event (idle_prompt fires periodically and would hide Stop)
if [ "$event" = "Notification" ]; then
    case "$notif_type" in
        permission_prompt|agent_needs_input|elicitation_dialog) ;;
        *) exit 0 ;;
    esac
fi

# Third field carries the event-specific payload
extra="$tool"
[ "$event" = "Notification" ] && extra="$notif_type"
[ "$event" = "SubagentStart" ] || [ "$event" = "SubagentStop" ] && extra="$agent_id"
[ "$event" = "PreCompact" ] || [ "$event" = "PostCompact" ] && extra="$trigger"

# Hook process tree: shell → claude → bash → this script
# Walk up to find the shell PID
find_shell_pid() {
    local pid=$PPID
    while [ -n "$pid" ] && [ "$pid" -gt 1 ] 2>/dev/null; do
        local ppid
        ppid=$(awk '{print $4}' "/proc/$pid/stat" 2>/dev/null) || break
        local comm
        comm=$(cat "/proc/$ppid/comm" 2>/dev/null) || break
        case "$comm" in
            bash|zsh|fish|sh|dash|ksh|tcsh|csh)
                echo "$ppid"
                return
                ;;
        esac
        pid=$ppid
    done
}

shell_pid=$(find_shell_pid)
[ -z "$shell_pid" ] && exit 0

dir="$HOME/.summoner/claude-states"
mkdir -p "$dir" 2>/dev/null

# Track active subagents via marker files (don't touch main state file)
if [ "$event" = "SubagentStart" ] && [ -n "$agent_id" ]; then
    mkdir -p "$dir/$shell_pid.agents" 2>/dev/null
    touch "$dir/$shell_pid.agents/$agent_id"
elif [ "$event" = "SubagentStop" ] && [ -n "$agent_id" ]; then
    rm -f "$dir/$shell_pid.agents/$agent_id"
    rmdir "$dir/$shell_pid.agents" 2>/dev/null
else
    echo "$event $sid $extra" > "$dir/$shell_pid"
    [ "$event" = "SessionEnd" ] && rm -rf "$dir/$shell_pid.agents"
fi
exit 0
"#;

const HOOK_COMMAND: &str = "bash ~/.summoner/hooks/claude-state.sh";

const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "Stop",
    "Notification",
    "SessionEnd",
    "PreToolUse",
    "PostToolUse",
    "SubagentStart",
    "SubagentStop",
    "PreCompact",
    "PostCompact",
];

/// Install the hook script and configure ~/.claude/settings.json.
/// Also cleans up stale state files from prior runs.
pub fn install_hooks(summoner_dir: &Path) {
    let _ = install_hook_script(summoner_dir);
    let _ = configure_claude_settings();
    clear_all_state_files(summoner_dir);
    crate::statusline::install_wrapper(summoner_dir);
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

/// Remove all state files. Called on startup to avoid stale PID reuse.
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

/// Remove the state file for a specific shell PID.
pub fn clear_state_file(summoner_dir: &Path, shell_pid: u32) {
    let _ = fs::remove_file(state_file_path(summoner_dir, shell_pid));
    let agents_dir = summoner_dir.join("claude-states").join(format!("{}.agents", shell_pid));
    let _ = fs::remove_dir_all(agents_dir);
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
    let claude_dir = dirs::home_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?
        .join(".claude");
    fs::create_dir_all(&claude_dir)?;
    let settings_path = claude_dir.join("settings.json");

    // Read existing settings or start fresh
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let hooks = settings
        .as_object_mut()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "settings not object"))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let hooks_obj = match hooks.as_object_mut() {
        Some(obj) => obj,
        None => return Ok(()),
    };

    let our_hook = serde_json::json!({
        "type": "command",
        "command": HOOK_COMMAND
    });
    let mut changed = false;

    for &event in HOOK_EVENTS {
        let event_hooks = hooks_obj
            .entry(event)
            .or_insert_with(|| serde_json::json!([]));

        let arr = match event_hooks.as_array_mut() {
            Some(a) => a,
            None => continue,
        };

        // Check if our hook is already present
        let already_present = arr.iter().any(|entry| {
            entry.get("hooks")
                .and_then(|h| h.as_array())
                .map(|hooks| hooks.iter().any(|h| {
                    h.get("command").and_then(|c| c.as_str()) == Some(HOOK_COMMAND)
                }))
                .unwrap_or(false)
        });

        if !already_present {
            arr.push(serde_json::json!({
                "matcher": "",
                "hooks": [our_hook]
            }));
            changed = true;
        }
    }

    if changed {
        let content = serde_json::to_string_pretty(&settings)
            .map_err(std::io::Error::other)?;
        fs::write(&settings_path, content)?;
    }
    Ok(())
}

fn unconfigure_claude_settings() -> std::io::Result<()> {
    let settings_path = dirs::home_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?
        .join(".claude")
        .join("settings.json");

    if !settings_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&settings_path)?;
    let mut settings: serde_json::Value = serde_json::from_str(&content)
        .map_err(std::io::Error::other)?;

    let mut changed = false;

    // Remove our hook entries
    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for &event in HOOK_EVENTS {
            if let Some(arr) = hooks.get_mut(event).and_then(|v| v.as_array_mut()) {
                let before = arr.len();
                arr.retain(|entry| {
                    !entry.get("hooks")
                        .and_then(|h| h.as_array())
                        .map(|hooks| hooks.iter().any(|h| {
                            h.get("command").and_then(|c| c.as_str()) == Some(HOOK_COMMAND)
                        }))
                        .unwrap_or(false)
                });
                if arr.len() != before {
                    changed = true;
                }
            }
        }
        // Clean up empty event arrays
        let empty_events: Vec<String> = hooks.iter()
            .filter(|(_, v)| v.as_array().map(|a| a.is_empty()).unwrap_or(false))
            .map(|(k, _)| k.clone())
            .collect();
        for key in empty_events {
            hooks.remove(&key);
            changed = true;
        }
        // Remove hooks key if empty
        if hooks.is_empty() {
            // mark for removal below
        }
    }
    if settings.get("hooks").and_then(|h| h.as_object()).map(|o| o.is_empty()).unwrap_or(false) {
        settings.as_object_mut().unwrap().remove("hooks");
        changed = true;
    }

    if changed {
        let content = serde_json::to_string_pretty(&settings)
            .map_err(std::io::Error::other)?;
        fs::write(&settings_path, content)?;
    }
    Ok(())
}

fn state_file_path(summoner_dir: &Path, shell_pid: u32) -> PathBuf {
    summoner_dir.join("claude-states").join(shell_pid.to_string())
}

/// Last time a hook event was recorded for a shell PID.
pub fn state_file_mtime(summoner_dir: &Path, shell_pid: u32) -> Option<SystemTime> {
    fs::metadata(state_file_path(summoner_dir, shell_pid))
        .and_then(|m| m.modified())
        .ok()
}

/// Count active subagents for a shell PID by counting marker files.
fn active_subagent_count(summoner_dir: &Path, shell_pid: u32) -> usize {
    let agents_dir = summoner_dir.join("claude-states").join(format!("{}.agents", shell_pid));
    fs::read_dir(agents_dir).map(|entries| entries.count()).unwrap_or(0)
}

/// Read hook state and session ID in a single file read.
/// Returns (state, session_id, active_tool) where state is None if no hook data or Claude exited.
pub fn read_hook_state(summoner_dir: &Path, shell_pid: u32) -> (Option<SessionState>, Option<String>, Option<String>) {
    let state_file = state_file_path(summoner_dir, shell_pid);
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

    let has_active_subagents = active_subagent_count(summoner_dir, shell_pid) > 0;

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
