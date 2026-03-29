use std::fs;
use std::path::Path;

const WRAPPER_SCRIPT: &str = r#"#!/bin/bash
# Summoner statusLine wrapper — tees JSON to state file, pipes to downstream
input=$(cat)

# Extract session_id
sid_tmp="${input##*"\"session_id\":\""}"
session_id="${sid_tmp%%\"*}"

if [ -n "$session_id" ]; then
    dir="$HOME/.summoner/statusline-states"
    mkdir -p "$dir" 2>/dev/null
    printf '%s' "$input" > "$dir/$session_id.json"
fi

# Pipe to downstream command (original statusLine)
PASSTHROUGH_CMD="__PASSTHROUGH__"
if [ -n "$PASSTHROUGH_CMD" ] && [ "$PASSTHROUGH_CMD" != "__PASSTHROUGH__" ]; then
    printf '%s' "$input" | eval "$PASSTHROUGH_CMD"
fi
"#;

const WRAPPER_COMMAND: &str = "bash ~/.summoner/hooks/statusline-wrapper.sh";

/// Install the statusLine wrapper script and update ~/.claude/settings.json.
pub fn install_wrapper(summoner_dir: &Path) {
    let _ = install_wrapper_inner(summoner_dir);
}

fn install_wrapper_inner(summoner_dir: &Path) -> std::io::Result<()> {
    let hooks_dir = summoner_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let claude_dir = dirs::home_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?
        .join(".claude");
    let settings_path = claude_dir.join("settings.json");

    let settings: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let current_cmd = settings.get("statusLine")
        .and_then(|sl| sl.get("command"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    if current_cmd == WRAPPER_COMMAND {
        return Ok(());
    }

    let passthrough = if current_cmd.is_empty() || current_cmd == WRAPPER_COMMAND {
        String::new()
    } else {
        current_cmd.to_string()
    };

    let script = WRAPPER_SCRIPT.replace("__PASSTHROUGH__", &passthrough);
    let script_path = hooks_dir.join("statusline-wrapper.sh");
    fs::write(&script_path, &script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
    }

    let mut settings = settings;
    settings["statusLine"] = serde_json::json!({
        "type": "command",
        "command": WRAPPER_COMMAND,
        "padding": 0
    });

    let content = serde_json::to_string_pretty(&settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(&settings_path, content)?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct StatusLineData {
    pub session_id: String,
    pub context_pct: Option<u8>,
    pub five_hour_pct: Option<u8>,
    pub five_hour_resets_at: Option<i64>,
    pub seven_day_pct: Option<u8>,
    pub seven_day_resets_at: Option<i64>,
}

impl StatusLineData {
    pub fn from_json(json: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let session_id = v.get("session_id")?.as_str()?.to_string();

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
            context_pct,
            five_hour_pct,
            five_hour_resets_at,
            seven_day_pct,
            seven_day_resets_at,
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
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Some(data) = Self::from_json(&content) {
                        results.push(data);
                    }
                }
            }
        }
        results
    }
}
