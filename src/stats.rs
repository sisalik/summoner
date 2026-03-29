pub fn format_xp(tokens: u64) -> String {
    if tokens < 1_000 {
        format!("✦ {}", tokens)
    } else if tokens < 1_000_000 {
        format!("✦ {:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("✦ {:.1}M", tokens as f64 / 1_000_000.0)
    }
}

const LEVEL_THRESHOLDS: &[(u64, u8)] = &[
    (25_000_000, 10),
    (10_000_000, 9),
    (5_000_000, 8),
    (2_500_000, 7),
    (1_000_000, 6),
    (500_000, 5),
    (150_000, 4),
    (50_000, 3),
    (10_000, 2),
];

pub fn level_from_tokens(tokens: u64) -> u8 {
    for &(threshold, level) in LEVEL_THRESHOLDS {
        if tokens >= threshold {
            return level;
        }
    }
    1
}

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

/// Parse a JSONL conversation file from a byte offset.
/// Returns (new_tokens, new_messages, new_offset).
pub fn parse_jsonl_stats(path: &Path, offset: u64) -> (u64, u32, u64) {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (0, 0, offset),
    };

    if offset > 0
        && file.seek(SeekFrom::Start(offset)).is_err() {
            return (0, 0, offset);
        }

    let reader = BufReader::new(&file);
    let mut tokens: u64 = 0;
    let mut messages: u32 = 0;
    let mut bytes_read: u64 = offset;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        bytes_read += line.len() as u64 + 1;

        let parsed: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_type = parsed.get("type").and_then(|t| t.as_str());
        match msg_type {
            Some("user") | Some("assistant") => {
                messages += 1;
            }
            _ => continue,
        }

        if let Some(usage) = parsed.get("message").and_then(|m| m.get("usage")) {
            let input = usage.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let output = usage.get("outputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
            tokens += input + output;
        }
    }

    (tokens, messages, bytes_read)
}

use std::path::PathBuf;

fn dir_to_project_hash(directory: &str) -> String {
    directory.replace('/', "-")
}

pub fn find_jsonl_path(claude_dir: &Path, project_directory: &str, session_id: &str) -> Option<PathBuf> {
    let hash = dir_to_project_hash(project_directory);
    let session_dir = claude_dir.join("projects").join(&hash).join(session_id);
    if !session_dir.is_dir() { return None; }
    let entries = std::fs::read_dir(&session_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            return Some(path);
        }
    }
    None
}

/// Tool display for creature status line — uses emojis with space separator.
pub fn tool_display(tool_name: &str) -> String {
    match tool_name {
        "Edit" => "\u{270f}\u{fe0f} Editing".into(),   // ✏️ Editing
        "Write" => "\u{1f4dd} Writing".into(),          // 📝 Writing
        "Read" => "\u{1f4d6} Reading".into(),           // 📖 Reading
        "Bash" => "\u{2699}\u{fe0f} Running".into(),    // ⚙️ Running
        "Glob" | "Grep" => "\u{1f50d} Searching".into(), // 🔍 Searching
        "Agent" => "\u{1f916} Delegating".into(),       // 🤖 Delegating
        "WebSearch" => "\u{1f310} Browsing".into(),     // 🌐 Browsing
        "WebFetch" => "\u{1f310} Fetching".into(),      // 🌐 Fetching
        other => format!("\u{1f527} {}", other),         // 🔧 Other
    }
}
