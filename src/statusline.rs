use std::fs;
use std::path::Path;

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
