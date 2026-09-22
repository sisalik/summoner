use std::fs;
use std::path::Path;

use crate::claude_settings;

const WRAPPER_SCRIPT_HEAD: &str = r#"#!/bin/bash
# Summoner statusLine wrapper — tees JSON to state file, pipes to downstream
IFS= read -r -d '' input

# Extract session_id
sid_tmp="${input##*"\"session_id\":\""}"
session_id="${sid_tmp%%\"*}"

if [ -n "$session_id" ]; then
    dir="$HOME/.summoner/statusline-states"
    mkdir -p "$dir" 2>/dev/null
    printf '%s' "$input" > "$dir/$session_id.json"
fi
"#;

const WRAPPER_COMMAND: &str = "bash ~/.summoner/hooks/statusline-wrapper.sh";

/// Extract the passthrough command from an existing wrapper script, if any.
fn extract_passthrough_from_script(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    // Look for: printf '%s' "$input" | <command>
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("printf '%s' \"$input\" | ")
            && !rest.is_empty() {
                return Some(rest.to_string());
            }
    }
    None
}

/// Install the statusLine wrapper script and update ~/.claude/settings.json.
pub fn install_wrapper(summoner_dir: &Path) -> std::io::Result<()> {
    install_wrapper_at(summoner_dir, &claude_settings::path()?)
}

/// As `install_wrapper`, against a given settings file.
pub fn install_wrapper_at(summoner_dir: &Path, settings_path: &Path) -> std::io::Result<()> {
    let hooks_dir = summoner_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let mut settings = claude_settings::read(settings_path)?;
    let original = settings.clone();

    let current_cmd = settings.get("statusLine")
        .and_then(|sl| sl.get("command"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    // If wrapper is already set as the command, the passthrough was captured on
    // first install. Read it from the existing script to preserve it.
    // If a different command is set, that's our passthrough target.
    let passthrough = if current_cmd.is_empty() || current_cmd == WRAPPER_COMMAND {
        // Try to extract passthrough from existing script
        let script_path = hooks_dir.join("statusline-wrapper.sh");
        extract_passthrough_from_script(&script_path).unwrap_or_default()
    } else {
        current_cmd.to_string()
    };

    // Build script: always write state file, conditionally pipe to downstream
    let mut script = WRAPPER_SCRIPT_HEAD.to_string();
    if !passthrough.is_empty() {
        script.push_str(&format!(
            "\n# Pipe to downstream command (original statusLine)\nprintf '%s' \"$input\" | {}\n",
            passthrough
        ));
    }
    let script_path = hooks_dir.join("statusline-wrapper.sh");
    fs::write(&script_path, &script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
    }

    settings.insert("statusLine".into(), serde_json::json!({
        "type": "command",
        "command": WRAPPER_COMMAND,
        "padding": 0
    }));

    if settings == original {
        return Ok(());
    }
    claude_settings::write(settings_path, &settings)
}

/// Remove the statusLine wrapper from ~/.claude/settings.json, restoring
/// any passthrough command that was captured on install.
pub fn uninstall_wrapper() {
    let _ = uninstall_wrapper_inner();
}

fn uninstall_wrapper_inner() -> std::io::Result<()> {
    let home = dirs::home_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?;
    uninstall_wrapper_at(
        &home.join(".summoner/hooks/statusline-wrapper.sh"),
        &claude_settings::path()?,
    )
}

/// As `uninstall_wrapper`, against a given wrapper script and settings file.
pub fn uninstall_wrapper_at(script_path: &Path, settings_path: &Path) -> std::io::Result<()> {
    if !settings_path.exists() {
        return Ok(());
    }

    // The passthrough command captured on install, to be restored
    let passthrough = extract_passthrough_from_script(script_path);

    let mut settings = claude_settings::read(settings_path)?;

    // Only touch statusLine if it's currently ours
    let is_ours = settings.get("statusLine")
        .and_then(|sl| sl.get("command"))
        .and_then(|c| c.as_str())
        == Some(WRAPPER_COMMAND);

    if is_ours {
        match passthrough {
            Some(passthrough_cmd) => {
                settings.insert("statusLine".into(), serde_json::json!({
                    "type": "command",
                    "command": passthrough_cmd
                }));
            }
            None => {
                settings.remove("statusLine");
            }
        }
        claude_settings::write(settings_path, &settings)?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct StatusLineData {
    pub session_id: String,
    pub transcript_path: Option<String>,
    pub context_pct: Option<u8>,
    pub five_hour_pct: Option<u8>,
    pub five_hour_resets_at: Option<i64>,
    pub seven_day_pct: Option<u8>,
    pub seven_day_resets_at: Option<i64>,
    pub modified_at: std::time::SystemTime,
}

impl StatusLineData {
    pub fn from_json(json: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let session_id = v.get("session_id")?.as_str()?.to_string();
        let transcript_path = v.get("transcript_path")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());

        let context_pct = v.get("context_window")
            .and_then(|cw| cw.get("used_percentage"))
            .and_then(|p| p.as_u64())
            .map(|p| p.min(255) as u8);

        let five_hour = v.get("rate_limits").and_then(|rl| rl.get("five_hour"));
        let five_hour_pct = five_hour
            .and_then(|fh| fh.get("used_percentage"))
            .and_then(|p| p.as_u64())
            .map(|p| p.min(255) as u8);
        let five_hour_resets_at = five_hour
            .and_then(|fh| fh.get("resets_at"))
            .and_then(|r| r.as_i64());

        let seven_day = v.get("rate_limits").and_then(|rl| rl.get("seven_day"));
        let seven_day_pct = seven_day
            .and_then(|sd| sd.get("used_percentage"))
            .and_then(|p| p.as_u64())
            .map(|p| p.min(255) as u8);
        let seven_day_resets_at = seven_day
            .and_then(|sd| sd.get("resets_at"))
            .and_then(|r| r.as_i64());

        Some(Self {
            session_id,
            transcript_path,
            context_pct,
            five_hour_pct,
            five_hour_resets_at,
            seven_day_pct,
            seven_day_resets_at,
            modified_at: std::time::SystemTime::UNIX_EPOCH,
        })
    }

    pub fn read_all(summoner_dir: &Path) -> Vec<StatusLineData> {
        let dir = summoner_dir.join("statusline-states");
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        let mut results = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                && let Ok(meta) = path.metadata()
                    && let Ok(content) = fs::read_to_string(&path)
                        && let Some(mut data) = Self::from_json(&content) {
                            data.modified_at = meta.modified()
                                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                            results.push(data);
                        }
        }
        results
    }
}

/// Remove all statusLine state files. Called on startup to clear stale data
/// from previous sessions.
pub fn clear_stale_files(summoner_dir: &Path) {
    let dir = summoner_dir.join("statusline-states");
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }
}
