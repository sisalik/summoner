use std::fs;
use std::path::{Path, PathBuf};

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
echo "$event $sid" > "$dir/$shell_pid"
"#;

const HOOK_COMMAND: &str = "bash ~/.summoner/hooks/claude-state.sh";

const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "Stop",
    "Notification",
    "SessionEnd",
];

/// Install the hook script and configure ~/.claude/settings.json.
/// Also cleans up stale state files from prior runs.
pub fn install_hooks(summoner_dir: &Path) {
    let _ = install_hook_script(summoner_dir);
    let _ = configure_claude_settings();
    clear_all_state_files(summoner_dir);
}

/// Remove all state files. Called on startup to avoid stale PID reuse.
pub fn clear_all_state_files(summoner_dir: &Path) {
    let dir = summoner_dir.join("claude-states");
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Remove the state file for a specific shell PID.
pub fn clear_state_file(summoner_dir: &Path, shell_pid: u32) {
    let _ = fs::remove_file(state_file_path(summoner_dir, shell_pid));
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
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(&settings_path, content)?;
    }
    Ok(())
}

fn state_file_path(summoner_dir: &Path, shell_pid: u32) -> PathBuf {
    summoner_dir.join("claude-states").join(shell_pid.to_string())
}

/// Read hook state and session ID in a single file read.
/// Returns (state, session_id) where state is None if no hook data or Claude exited.
pub fn read_hook_state(summoner_dir: &Path, shell_pid: u32) -> (Option<SessionState>, Option<String>) {
    let state_file = state_file_path(summoner_dir, shell_pid);
    let content = match fs::read_to_string(&state_file) {
        Ok(c) => c,
        Err(_) => return (None, None),
    };
    let mut parts = content.split_whitespace();
    let event = match parts.next() {
        Some(e) => e,
        None => return (None, None),
    };
    let session_id = parts.next().map(|s| s.to_string());

    let state = match event {
        "UserPromptSubmit" => Some(SessionState::Working),
        "Stop" | "SessionStart" => Some(SessionState::Idle),
        "Notification" => Some(SessionState::Waiting),
        "SessionEnd" => {
            let _ = fs::remove_file(&state_file);
            return (None, None);
        }
        _ => None,
    };

    (state, session_id)
}
