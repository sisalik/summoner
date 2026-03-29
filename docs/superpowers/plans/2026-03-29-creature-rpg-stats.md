# Creature RPG Stats & Usage Dashboard — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add RPG-style stats (level, XP, active tool, context health bar), git diff stats in card titles, and a global usage status bar to the Summoner dashboard.

**Architecture:** Four new data collection modules (`stats.rs`, `statusline.rs`, `git.rs`, enhanced `hooks.rs`) feed into new `SessionStats` and `GlobalStats` structs on `App`. The dashboard UI gains a stats line + health bar per creature, colored git diff in card titles, and a global usage bar. A statusLine wrapper script tees JSON to disk for Summoner to read.

**Tech Stack:** Rust, ratatui, serde_json (already dep), chrono (already dep), `git` CLI for diff stats, bash wrapper script for statusLine.

---

### Task 1: Stats Module — XP Formatting and Level Calculation

**Files:**
- Create: `src/stats.rs`
- Modify: `src/lib.rs`
- Create: `tests/stats_test.rs`

- [ ] **Step 1: Write failing tests for `format_xp`**

Create `tests/stats_test.rs`:

```rust
use summoner::stats::{format_xp, level_from_tokens};

#[test]
fn format_xp_raw_below_1000() {
    assert_eq!(format_xp(0), "✦0");
    assert_eq!(format_xp(847), "✦847");
    assert_eq!(format_xp(999), "✦999");
}

#[test]
fn format_xp_thousands() {
    assert_eq!(format_xp(1000), "✦1.0k");
    assert_eq!(format_xp(12_400), "✦12.4k");
    assert_eq!(format_xp(999_999), "✦1000.0k");
}

#[test]
fn format_xp_millions() {
    assert_eq!(format_xp(1_000_000), "✦1.0M");
    assert_eq!(format_xp(1_200_000), "✦1.2M");
    assert_eq!(format_xp(25_500_000), "✦25.5M");
}

#[test]
fn level_thresholds() {
    assert_eq!(level_from_tokens(0), 1);
    assert_eq!(level_from_tokens(9_999), 1);
    assert_eq!(level_from_tokens(10_000), 2);
    assert_eq!(level_from_tokens(49_999), 2);
    assert_eq!(level_from_tokens(50_000), 3);
    assert_eq!(level_from_tokens(500_000), 5);
    assert_eq!(level_from_tokens(25_000_000), 10);
    assert_eq!(level_from_tokens(100_000_000), 10);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test stats_test 2>&1 | head -20`
Expected: compilation error — `summoner::stats` doesn't exist.

- [ ] **Step 3: Implement `stats.rs` with `format_xp` and `level_from_tokens`**

Create `src/stats.rs`:

```rust
/// Format a token count as compact XP string.
pub fn format_xp(tokens: u64) -> String {
    if tokens < 1_000 {
        format!("✦{}", tokens)
    } else if tokens < 1_000_000 {
        format!("✦{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("✦{:.1}M", tokens as f64 / 1_000_000.0)
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

/// Calculate level from total tokens consumed.
pub fn level_from_tokens(tokens: u64) -> u8 {
    for &(threshold, level) in LEVEL_THRESHOLDS {
        if tokens >= threshold {
            return level;
        }
    }
    1
}
```

Add to `src/lib.rs`:

```rust
pub mod stats;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test stats_test -v`
Expected: all 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stats.rs src/lib.rs tests/stats_test.rs
git commit -m "feat: add stats module with XP formatting and level calculation"
```

---

### Task 2: Stats Module — JSONL Parser

**Files:**
- Modify: `src/stats.rs`
- Modify: `tests/stats_test.rs`

- [ ] **Step 1: Write failing tests for JSONL parsing**

Append to `tests/stats_test.rs`:

```rust
use summoner::stats::parse_jsonl_stats;
use std::io::Write;

#[test]
fn parse_jsonl_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.jsonl");
    std::fs::write(&path, "").unwrap();
    let (tokens, messages, offset) = parse_jsonl_stats(&path, 0);
    assert_eq!(tokens, 0);
    assert_eq!(messages, 0);
    assert_eq!(offset, 0);
}

#[test]
fn parse_jsonl_counts_tokens_and_messages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.jsonl");
    let mut f = std::fs::File::create(&path).unwrap();
    // User message (no usage field)
    writeln!(f, r#"{{"type":"user","message":{{"role":"user","content":"hello"}}}}"#).unwrap();
    // Assistant message with usage
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"hi","usage":{{"inputTokens":100,"outputTokens":50,"cacheReadInputTokens":20,"cacheCreationInputTokens":10}}}}}}"#).unwrap();
    // Another assistant message
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"ok","usage":{{"inputTokens":200,"outputTokens":100,"cacheReadInputTokens":0,"cacheCreationInputTokens":0}}}}}}"#).unwrap();

    let (tokens, messages, offset) = parse_jsonl_stats(&path, 0);
    assert_eq!(tokens, 450); // 100+50 + 200+100
    assert_eq!(messages, 3); // 1 user + 2 assistant
    assert!(offset > 0);
}

#[test]
fn parse_jsonl_incremental_from_offset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.jsonl");
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, r#"{{"type":"user","message":{{"role":"user","content":"hello"}}}}"#).unwrap();
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"hi","usage":{{"inputTokens":100,"outputTokens":50,"cacheReadInputTokens":0,"cacheCreationInputTokens":0}}}}}}"#).unwrap();

    let (tokens1, messages1, offset1) = parse_jsonl_stats(&path, 0);
    assert_eq!(tokens1, 150);
    assert_eq!(messages1, 2);

    // Append more data
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"more","usage":{{"inputTokens":300,"outputTokens":200,"cacheReadInputTokens":0,"cacheCreationInputTokens":0}}}}}}"#).unwrap();

    let (tokens2, messages2, _offset2) = parse_jsonl_stats(&path, offset1);
    assert_eq!(tokens2, 500); // only new: 300+200
    assert_eq!(messages2, 1); // only new message
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test stats_test parse_jsonl 2>&1 | head -10`
Expected: compilation error — `parse_jsonl_stats` doesn't exist.

- [ ] **Step 3: Implement `parse_jsonl_stats`**

Add to `src/stats.rs`:

```rust
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

/// Parse a JSONL conversation file from a byte offset.
/// Returns (new_tokens, new_messages, new_offset).
/// `new_tokens` and `new_messages` are counts from the offset onward only.
pub fn parse_jsonl_stats(path: &Path, offset: u64) -> (u64, u32, u64) {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (0, 0, offset),
    };

    if offset > 0 {
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return (0, 0, offset);
        }
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
        bytes_read += line.len() as u64 + 1; // +1 for newline

        let parsed: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Count any line with type "user" or "assistant" as a message
        let msg_type = parsed.get("type").and_then(|t| t.as_str());
        match msg_type {
            Some("user") | Some("assistant") => {
                messages += 1;
            }
            _ => continue,
        }

        // Extract token counts from assistant messages
        if let Some(usage) = parsed
            .get("message")
            .and_then(|m| m.get("usage"))
        {
            let input = usage.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let output = usage.get("outputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
            tokens += input + output;
        }
    }

    (tokens, messages, bytes_read)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test stats_test -v`
Expected: all 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stats.rs tests/stats_test.rs
git commit -m "feat: add JSONL conversation parser for token and message counting"
```

---

### Task 3: Stats Module — JSONL File Locator

**Files:**
- Modify: `src/stats.rs`
- Modify: `tests/stats_test.rs`

- [ ] **Step 1: Write failing test for JSONL file location**

Append to `tests/stats_test.rs`:

```rust
use summoner::stats::find_jsonl_path;

#[test]
fn find_jsonl_path_locates_file() {
    let dir = tempfile::tempdir().unwrap();
    // Simulate ~/.claude/projects/{hash}/{session_id}/
    let project_hash = "-home-test-myproject";
    let session_id = "abc-123";
    let session_dir = dir.path()
        .join("projects")
        .join(project_hash)
        .join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    // Create a JSONL file
    let jsonl = session_dir.join("agent-xyz.jsonl");
    std::fs::write(&jsonl, "{}\n").unwrap();

    let result = find_jsonl_path(dir.path(), "/home/test/myproject", session_id);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), jsonl);
}

#[test]
fn find_jsonl_path_returns_none_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    let result = find_jsonl_path(dir.path(), "/home/test/myproject", "no-such-id");
    assert!(result.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test stats_test find_jsonl 2>&1 | head -10`
Expected: compilation error.

- [ ] **Step 3: Implement `find_jsonl_path`**

Add to `src/stats.rs`:

```rust
use std::path::PathBuf;

/// Convert a directory path to Claude Code's project hash format.
/// e.g. "/home/siim/dev/summoner" → "-home-siim-dev-summoner"
fn dir_to_project_hash(directory: &str) -> String {
    directory.replace('/', "-")
}

/// Find the main JSONL conversation file for a session.
/// `claude_dir` is the base ~/.claude directory.
/// Returns the path to the first .jsonl file found in the session directory.
pub fn find_jsonl_path(claude_dir: &Path, project_directory: &str, session_id: &str) -> Option<PathBuf> {
    let hash = dir_to_project_hash(project_directory);
    let session_dir = claude_dir.join("projects").join(&hash).join(session_id);

    if !session_dir.is_dir() {
        return None;
    }

    // Find the first .jsonl file (the main conversation log)
    let entries = std::fs::read_dir(&session_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            return Some(path);
        }
    }

    None
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test stats_test -v`
Expected: all 9 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stats.rs tests/stats_test.rs
git commit -m "feat: add JSONL file locator for Claude conversation data"
```

---

### Task 4: Tool Display Mapping

**Files:**
- Modify: `src/stats.rs`
- Modify: `tests/stats_test.rs`

- [ ] **Step 1: Write failing tests for tool display**

Append to `tests/stats_test.rs`:

```rust
use summoner::stats::tool_display;

#[test]
fn tool_display_known_tools() {
    assert_eq!(tool_display("Edit"), "✏️ Editing");
    assert_eq!(tool_display("Write"), "📝 Writing");
    assert_eq!(tool_display("Read"), "📖 Reading");
    assert_eq!(tool_display("Bash"), "⚙️ Running");
    assert_eq!(tool_display("Glob"), "🔍 Searching");
    assert_eq!(tool_display("Grep"), "🔍 Searching");
    assert_eq!(tool_display("Agent"), "🤖 Delegating");
    assert_eq!(tool_display("WebSearch"), "🌐 Browsing");
    assert_eq!(tool_display("WebFetch"), "🌐 Fetching");
}

#[test]
fn tool_display_unknown_tool() {
    assert_eq!(tool_display("CustomTool"), "🔧 CustomTool");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test stats_test tool_display 2>&1 | head -10`
Expected: compilation error.

- [ ] **Step 3: Implement `tool_display`**

Add to `src/stats.rs`:

```rust
/// Map a Claude Code tool name to a display label with emoji.
pub fn tool_display(tool_name: &str) -> String {
    match tool_name {
        "Edit" => "✏️ Editing".into(),
        "Write" => "📝 Writing".into(),
        "Read" => "📖 Reading".into(),
        "Bash" => "⚙️ Running".into(),
        "Glob" | "Grep" => "🔍 Searching".into(),
        "Agent" => "🤖 Delegating".into(),
        "WebSearch" => "🌐 Browsing".into(),
        "WebFetch" => "🌐 Fetching".into(),
        other => format!("🔧 {}", other),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test stats_test -v`
Expected: all 11 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stats.rs tests/stats_test.rs
git commit -m "feat: add tool name to emoji display mapping"
```

---

### Task 5: Git Diff Stats Module

**Files:**
- Create: `src/git.rs`
- Modify: `src/lib.rs`
- Create: `tests/git_test.rs`

- [ ] **Step 1: Write failing tests for git shortstat parsing**

Create `tests/git_test.rs`:

```rust
use summoner::git::{parse_shortstat, format_diff_compact};

#[test]
fn parse_shortstat_full_output() {
    let output = " 3 files changed, 47 insertions(+), 12 deletions(-)";
    let (adds, dels) = parse_shortstat(output);
    assert_eq!(adds, 47);
    assert_eq!(dels, 12);
}

#[test]
fn parse_shortstat_insertions_only() {
    let output = " 1 file changed, 5 insertions(+)";
    let (adds, dels) = parse_shortstat(output);
    assert_eq!(adds, 5);
    assert_eq!(dels, 0);
}

#[test]
fn parse_shortstat_deletions_only() {
    let output = " 2 files changed, 10 deletions(-)";
    let (adds, dels) = parse_shortstat(output);
    assert_eq!(adds, 0);
    assert_eq!(dels, 10);
}

#[test]
fn parse_shortstat_empty_output() {
    let (adds, dels) = parse_shortstat("");
    assert_eq!(adds, 0);
    assert_eq!(dels, 0);
}

#[test]
fn format_diff_compact_small_numbers() {
    assert_eq!(format_diff_compact(47, 12), "+47-12");
}

#[test]
fn format_diff_compact_large_numbers() {
    assert_eq!(format_diff_compact(1200, 340), "+1.2k-340");
}

#[test]
fn format_diff_compact_zero() {
    assert_eq!(format_diff_compact(0, 0), "");
}

#[test]
fn format_diff_compact_only_additions() {
    assert_eq!(format_diff_compact(5, 0), "+5");
}

#[test]
fn format_diff_compact_only_deletions() {
    assert_eq!(format_diff_compact(0, 8), "-8");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test git_test 2>&1 | head -10`
Expected: compilation error.

- [ ] **Step 3: Implement `git.rs`**

Create `src/git.rs`:

```rust
use std::collections::HashMap;
use std::process::Command;
use std::time::Instant;

/// Parse `git diff --shortstat` output into (insertions, deletions).
pub fn parse_shortstat(output: &str) -> (u32, u32) {
    let mut adds: u32 = 0;
    let mut dels: u32 = 0;

    // Format: " 3 files changed, 47 insertions(+), 12 deletions(-)"
    for part in output.split(',') {
        let part = part.trim();
        if part.contains("insertion") {
            if let Some(n) = part.split_whitespace().next().and_then(|s| s.parse().ok()) {
                adds = n;
            }
        } else if part.contains("deletion") {
            if let Some(n) = part.split_whitespace().next().and_then(|s| s.parse().ok()) {
                dels = n;
            }
        }
    }

    (adds, dels)
}

/// Format diff stats compactly: "+47-12", "+1.2k-340", or "" if clean.
pub fn format_diff_compact(additions: u32, deletions: u32) -> String {
    if additions == 0 && deletions == 0 {
        return String::new();
    }

    let mut result = String::new();
    if additions > 0 {
        result.push('+');
        result.push_str(&format_compact_number(additions));
    }
    if deletions > 0 {
        result.push('-');
        result.push_str(&format_compact_number(deletions));
    }
    result
}

fn format_compact_number(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

/// Cached git diff stats per directory.
pub struct GitDiffCache {
    cache: HashMap<String, CachedDiff>,
    poll_interval_secs: u64,
}

struct CachedDiff {
    pub additions: u32,
    pub deletions: u32,
    pub last_checked: Instant,
}

impl GitDiffCache {
    pub fn new(poll_interval_secs: u64) -> Self {
        Self {
            cache: HashMap::new(),
            poll_interval_secs,
        }
    }

    /// Get diff stats for a directory, polling git if stale.
    pub fn get(&mut self, directory: &str) -> (u32, u32) {
        let now = Instant::now();

        if let Some(cached) = self.cache.get(directory) {
            if now.duration_since(cached.last_checked).as_secs() < self.poll_interval_secs {
                return (cached.additions, cached.deletions);
            }
        }

        let (adds, dels) = run_git_shortstat(directory);
        self.cache.insert(directory.to_string(), CachedDiff {
            additions: adds,
            deletions: dels,
            last_checked: now,
        });
        (adds, dels)
    }
}

fn run_git_shortstat(directory: &str) -> (u32, u32) {
    let output = Command::new("git")
        .args(["diff", "--shortstat"])
        .current_dir(directory)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_shortstat(&stdout)
        }
        Err(_) => (0, 0),
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod git;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test git_test -v`
Expected: all 9 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/git.rs src/lib.rs tests/git_test.rs
git commit -m "feat: add git diff stats module with shortstat parsing and caching"
```

---

### Task 6: StatusLine Module — JSON Parser and Wrapper Script

**Files:**
- Create: `src/statusline.rs`
- Modify: `src/lib.rs`
- Create: `tests/statusline_test.rs`

- [ ] **Step 1: Write failing tests for statusLine JSON parsing**

Create `tests/statusline_test.rs`:

```rust
use summoner::statusline::StatusLineData;

#[test]
fn parse_statusline_json_full() {
    let json = r#"{
        "session_id": "abc-123",
        "context_window": {
            "used_percentage": 42,
            "remaining_percentage": 58,
            "context_window_size": 1000000
        },
        "rate_limits": {
            "five_hour": {
                "used_percentage": 35,
                "resets_at": 1774020000
            },
            "seven_day": {
                "used_percentage": 67,
                "resets_at": 1774540000
            }
        }
    }"#;

    let data = StatusLineData::from_json(json).unwrap();
    assert_eq!(data.session_id, "abc-123");
    assert_eq!(data.context_pct, Some(42));
    assert_eq!(data.five_hour_pct, Some(35));
    assert_eq!(data.five_hour_resets_at, Some(1774020000));
    assert_eq!(data.seven_day_pct, Some(67));
    assert_eq!(data.seven_day_resets_at, Some(1774540000));
}

#[test]
fn parse_statusline_json_missing_rate_limits() {
    let json = r#"{
        "session_id": "abc-123",
        "context_window": {
            "used_percentage": 10
        }
    }"#;

    let data = StatusLineData::from_json(json).unwrap();
    assert_eq!(data.session_id, "abc-123");
    assert_eq!(data.context_pct, Some(10));
    assert_eq!(data.five_hour_pct, None);
    assert_eq!(data.seven_day_pct, None);
}

#[test]
fn parse_statusline_json_missing_session_id() {
    let json = r#"{"context_window": {"used_percentage": 5}}"#;
    let result = StatusLineData::from_json(json);
    assert!(result.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test statusline_test 2>&1 | head -10`
Expected: compilation error.

- [ ] **Step 3: Implement `statusline.rs` — parser**

Create `src/statusline.rs`:

```rust
use std::fs;
use std::path::Path;

/// Parsed statusLine data relevant to Summoner.
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
    /// Parse from a statusLine JSON string. Returns None if session_id is missing.
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

    /// Read all statusLine state files from the summoner directory.
    /// Returns a vec of parsed data, one per session.
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
```

Add to `src/lib.rs`:

```rust
pub mod statusline;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test statusline_test -v`
Expected: all 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/statusline.rs src/lib.rs tests/statusline_test.rs
git commit -m "feat: add statusLine JSON parser module"
```

---

### Task 7: StatusLine Wrapper Script and Installation

**Files:**
- Modify: `src/statusline.rs`
- Modify: `src/hooks.rs`

- [ ] **Step 1: Add wrapper script constant and installer to `statusline.rs`**

Add to the top of `src/statusline.rs`:

```rust
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
/// Preserves the existing statusLine command as a passthrough.
pub fn install_wrapper(summoner_dir: &Path) {
    let _ = install_wrapper_inner(summoner_dir);
}

fn install_wrapper_inner(summoner_dir: &Path) -> std::io::Result<()> {
    let hooks_dir = summoner_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    // Read existing claude settings to find current statusLine command
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

    // Check if wrapper is already installed
    let current_cmd = settings.get("statusLine")
        .and_then(|sl| sl.get("command"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    if current_cmd == WRAPPER_COMMAND {
        return Ok(()); // Already installed
    }

    // Store current command as passthrough (if it's not empty and not already our wrapper)
    let passthrough = if current_cmd.is_empty() || current_cmd == WRAPPER_COMMAND {
        String::new()
    } else {
        current_cmd.to_string()
    };

    // Write wrapper script with passthrough baked in
    let script = WRAPPER_SCRIPT.replace("__PASSTHROUGH__", &passthrough);
    let script_path = hooks_dir.join("statusline-wrapper.sh");
    fs::write(&script_path, &script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
    }

    // Update settings.json
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
```

- [ ] **Step 2: Call `install_wrapper` from `hooks::install_hooks`**

In `src/hooks.rs`, modify `install_hooks` to also install the statusLine wrapper:

```rust
pub fn install_hooks(summoner_dir: &Path) {
    let _ = install_hook_script(summoner_dir);
    let _ = configure_claude_settings();
    clear_all_state_files(summoner_dir);
    crate::statusline::install_wrapper(summoner_dir);
}
```

- [ ] **Step 3: Run existing tests to ensure no regressions**

Run: `cargo test 2>&1 | tail -5`
Expected: all existing tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/statusline.rs src/hooks.rs
git commit -m "feat: add statusLine wrapper script installer"
```

---

### Task 8: Enhanced Hook Script — Tool Tracking

**Files:**
- Modify: `src/hooks.rs`

- [ ] **Step 1: Update hook script to extract tool_name for PreToolUse/PostToolUse**

In `src/hooks.rs`, update `HOOK_SCRIPT` to also extract `tool_name`:

```rust
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
echo "$event $sid $tool" > "$dir/$shell_pid"
"#;
```

- [ ] **Step 2: Add `PreToolUse` and `PostToolUse` to `HOOK_EVENTS`**

```rust
const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "Stop",
    "Notification",
    "SessionEnd",
    "PreToolUse",
    "PostToolUse",
];
```

- [ ] **Step 3: Update `read_hook_state` to return tool name**

Change the return type and parsing in `read_hook_state`:

```rust
/// Read hook state, session ID, and active tool name.
/// Returns (state, session_id, tool_name).
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

    let state = match event {
        "UserPromptSubmit" => Some(SessionState::Working),
        "Stop" | "SessionStart" => Some(SessionState::Idle),
        "Notification" => Some(SessionState::Waiting),
        "PreToolUse" => Some(SessionState::Working),
        "PostToolUse" => Some(SessionState::Working),
        "SessionEnd" => {
            let _ = fs::remove_file(&state_file);
            return (None, None, None);
        }
        _ => None,
    };

    // For PostToolUse, clear the tool name (tool finished)
    let active_tool = match event {
        "PreToolUse" => tool_name,
        _ => None,
    };

    (state, session_id, active_tool)
}
```

- [ ] **Step 4: Update call site in `app.rs`**

In `src/app.rs`, update the `read_hook_state` call in `process_pty_output` (around line 252):

```rust
let (hook_state, hook_session_id, hook_tool) = shell_pid
    .map(|pid| hooks::read_hook_state(&self.config_dir, pid))
    .unwrap_or((None, None, None));
```

Store `hook_tool` — we'll wire it to `SessionStats` in a later task. For now, just capture it with `let _hook_tool = hook_tool;` so it compiles without warnings.

- [ ] **Step 5: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass. (The existing hook tests in config_test.rs don't call `read_hook_state` directly.)

- [ ] **Step 6: Commit**

```bash
git add src/hooks.rs src/app.rs
git commit -m "feat: enhance hooks with PreToolUse/PostToolUse for active tool tracking"
```

---

### Task 9: SessionStats and GlobalStats — Data Structures and Wiring

**Files:**
- Modify: `src/session.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Add `SessionStats` to `session.rs`**

Add at the bottom of `src/session.rs`:

```rust
/// RPG stats for a session, updated from JSONL and hooks.
pub struct SessionStats {
    pub total_tokens: u64,
    pub message_count: u32,
    pub active_tool: Option<String>,
    pub context_pct: Option<u8>,
    pub jsonl_offset: u64,
    pub jsonl_path: Option<std::path::PathBuf>,
}

impl SessionStats {
    pub fn new() -> Self {
        Self {
            total_tokens: 0,
            message_count: 0,
            active_tool: None,
            context_pct: None,
            jsonl_offset: 0,
            jsonl_path: None,
        }
    }
}
```

- [ ] **Step 2: Add `GlobalStats` to `session.rs`**

```rust
/// Global usage stats shown in the dashboard status bar.
pub struct GlobalStats {
    pub daily_messages: u32,
    pub daily_tokens: u64,
    pub five_hour_pct: Option<u8>,
    pub five_hour_resets_at: Option<i64>,
    pub seven_day_pct: Option<u8>,
    pub seven_day_resets_at: Option<i64>,
    pub last_daily_reset: chrono::NaiveDate,
}

impl GlobalStats {
    pub fn new() -> Self {
        Self {
            daily_messages: 0,
            daily_tokens: 0,
            five_hour_pct: None,
            five_hour_resets_at: None,
            seven_day_pct: None,
            seven_day_resets_at: None,
            last_daily_reset: chrono::Local::now().date_naive(),
        }
    }

    /// Reset daily counters if the date has changed.
    pub fn check_daily_reset(&mut self) {
        let today = chrono::Local::now().date_naive();
        if today != self.last_daily_reset {
            self.daily_messages = 0;
            self.daily_tokens = 0;
            self.last_daily_reset = today;
        }
    }
}
```

- [ ] **Step 3: Add stats vecs and git cache to `App` struct in `app.rs`**

Add fields to the `App` struct:

```rust
use crate::session::{Session, SessionState, SessionStats, GlobalStats, session_order};
use crate::git::GitDiffCache;
```

In `struct App`:

```rust
    session_stats: Vec<SessionStats>,
    global_stats: GlobalStats,
    git_cache: GitDiffCache,
    last_stats_update: Instant,
```

- [ ] **Step 4: Initialize new fields in `App::new()`**

In the `App::new()` function, initialize `session_stats` alongside sessions:

```rust
let mut session_stats = Vec::new();
```

Inside the restore loop, after pushing to `sessions`:

```rust
session_stats.push(SessionStats::new());
```

In the `Ok(Self { ... })` block, add:

```rust
session_stats,
global_stats: GlobalStats::new(),
git_cache: GitDiffCache::new(30),
last_stats_update: Instant::now(),
```

- [ ] **Step 5: Keep `session_stats` in sync with session add/remove**

In `spawn_session`, after `self.sessions.push(session)`:

```rust
self.session_stats.push(SessionStats::new());
```

In `close_session`, after `self.sessions.remove(idx)`:

```rust
self.session_stats.remove(idx);
```

- [ ] **Step 6: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/session.rs src/app.rs
git commit -m "feat: add SessionStats, GlobalStats, and GitDiffCache to App"
```

---

### Task 10: Periodic Stats Collection in App Tick

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add `update_stats` method to `App`**

Add this method to `impl App`:

```rust
fn update_stats(&mut self) {
    let now = Instant::now();
    if now.duration_since(self.last_stats_update).as_secs() < 60 {
        return;
    }
    self.last_stats_update = now;

    self.global_stats.check_daily_reset();

    let claude_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude");

    // Update per-session stats from JSONL
    for i in 0..self.sessions.len() {
        let session = &self.sessions[i];
        let stats = &mut self.session_stats[i];

        // Locate JSONL file if not yet found
        if stats.jsonl_path.is_none() {
            if let Some(ref conv_id) = session.claude_conversation_id {
                stats.jsonl_path = crate::stats::find_jsonl_path(
                    &claude_dir,
                    &session.directory,
                    conv_id,
                );
            }
        }

        // Parse JSONL incrementally
        if let Some(ref path) = stats.jsonl_path {
            let (new_tokens, new_messages, new_offset) =
                crate::stats::parse_jsonl_stats(path, stats.jsonl_offset);
            stats.total_tokens += new_tokens;
            stats.message_count += new_messages;
            stats.jsonl_offset = new_offset;

            // Accumulate into daily totals
            self.global_stats.daily_tokens += new_tokens;
            self.global_stats.daily_messages += new_messages;
        }
    }

    // Update statusLine data
    let sl_data = crate::statusline::StatusLineData::read_all(&self.config_dir);
    for sl in &sl_data {
        // Match statusLine session_id to our session's claude_conversation_id
        for i in 0..self.sessions.len() {
            if self.sessions[i].claude_conversation_id.as_deref() == Some(&sl.session_id) {
                self.session_stats[i].context_pct = sl.context_pct;
            }
        }

        // Update global rate limits from the most recent statusLine data
        if sl.five_hour_pct.is_some() {
            self.global_stats.five_hour_pct = sl.five_hour_pct;
            self.global_stats.five_hour_resets_at = sl.five_hour_resets_at;
        }
        if sl.seven_day_pct.is_some() {
            self.global_stats.seven_day_pct = sl.seven_day_pct;
            self.global_stats.seven_day_resets_at = sl.seven_day_resets_at;
        }
    }
}
```

- [ ] **Step 2: Wire active tool from hooks into session_stats**

In `process_pty_output`, replace `let _hook_tool = hook_tool;` with:

```rust
if i < self.session_stats.len() {
    self.session_stats[i].active_tool = hook_tool;
}
```

- [ ] **Step 3: Call `update_stats` in the main loop**

In the `run()` function, after `app.process_pty_output()`:

```rust
app.update_stats();
```

- [ ] **Step 4: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: add periodic stats collection from JSONL, statusLine, and hooks"
```

---

### Task 11: Dashboard UI — Stats Line and Health Bar

**Files:**
- Modify: `src/ui/dashboard.rs`

- [ ] **Step 1: Update `Dashboard` to accept stats**

Modify the `Dashboard` struct and constructor to accept session stats and global stats:

```rust
use crate::session::{group_by_project, Session, SessionState, SessionStats, GlobalStats};
use crate::git::GitDiffCache;
use crate::stats;

pub struct Dashboard<'a> {
    sessions: &'a [Session],
    session_stats: &'a [SessionStats],
    global_stats: &'a GlobalStats,
    rasters: &'a [RasterResult],
    nav: &'a DashboardNav,
    git_cache: &'a mut GitDiffCache,
}

impl<'a> Dashboard<'a> {
    pub fn new(
        sessions: &'a [Session],
        session_stats: &'a [SessionStats],
        global_stats: &'a GlobalStats,
        rasters: &'a [RasterResult],
        nav: &'a DashboardNav,
        git_cache: &'a mut GitDiffCache,
    ) -> Self {
        Self { sessions, session_stats, global_stats, rasters, nav, git_cache }
    }
}
```

Note: `git_cache` needs `&mut` because `get()` may poll git. This means `Dashboard` can't implement `Widget` (which takes `self` by value with immutable refs). Remove the `impl Widget for Dashboard` block entirely and replace it with a direct method:

```rust
impl<'a> Dashboard<'a> {
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        // ... existing render body, moved from Widget impl ...
    }
}
```

Update the call in `app.rs` accordingly — call `dashboard.render(main_area, frame.buffer_mut())` instead of `frame.render_widget(dashboard, main_area)`.

- [ ] **Step 2: Increase card height for health bar row**

Update the card height calculation:

```rust
// card inner height: padding(1) + creature(creature_render_h) + stats_line(1) + health_bar(1) + padding(1)
let card_inner_height = 1 + creature_render_h + 1 + 1 + 1;
```

- [ ] **Step 3: Replace state label with stats line**

Replace the state label rendering block (currently draws `session.state.label()`) with:

```rust
// Draw stats line below creature: "⚡ Editing  Lv.3  ✦12.4k"
let stats_y = inner.y + creature_render_h;
if stats_y < card_area.y + card_area.height.saturating_sub(2) {
    let stat = if sess_idx < self.session_stats.len() {
        &self.session_stats[sess_idx]
    } else {
        // Fallback: use a static default (shouldn't happen)
        &SessionStats::new()  // won't compile with ref to temp — see note below
    };

    // Active tool or state fallback
    let activity = if let Some(ref tool) = stat.active_tool {
        stats::tool_display(tool)
    } else {
        format!("{} {}", session.state.icon(), session.state.label())
    };

    let level = stats::level_from_tokens(stat.total_tokens);
    let xp = stats::format_xp(stat.total_tokens);
    let stats_text = format!("{}  Lv.{}  {}", activity, level, xp);
    let stats_len = stats_text.chars().count() as u16;
    let stats_x = cx + cw.saturating_sub(stats_len) / 2;

    let stats_style = Style::default().fg(session.state.color());
    draw_text(stats_x, stats_y, &stats_text, stats_style, card_area, buf);
}
```

Handle the `sess_idx < self.session_stats.len()` check by creating a local default:

```rust
let default_stats = SessionStats::new();
let stat = self.session_stats.get(sess_idx).unwrap_or(&default_stats);
```

- [ ] **Step 4: Add health bar rendering below stats line**

```rust
// Draw health bar below stats line
let bar_y = stats_y + 1;
if bar_y < card_area.y + card_area.height.saturating_sub(1) {
    let is_claude = session.claude_conversation_id.is_some()
        || session.state == SessionState::Working
        || session.state == SessionState::Waiting
        || session.state == SessionState::Idle;

    if is_claude {
        let default_stats = SessionStats::new();
        let stat = self.session_stats.get(sess_idx).unwrap_or(&default_stats);
        let pct = stat.context_pct.unwrap_or(0);

        // Heart emoji and bar color based on percentage
        let (heart, bar_color) = match pct {
            0..=50 => ("💚", Color::Rgb(129, 199, 132)),
            51..=75 => ("💛", Color::Rgb(255, 213, 79)),
            76..=90 => ("🧡", Color::Rgb(255, 183, 77)),
            _ => ("❤️", Color::Rgb(229, 115, 115)),
        };

        // Bar width: use creature width minus space for heart(2) + space(1) + pct(4-5)
        let bar_total = cw.saturating_sub(8) as usize; // leave room for heart + " XX%"
        let filled = (bar_total as u64 * pct as u64 / 100).min(bar_total as u64) as usize;
        let empty = bar_total.saturating_sub(filled);

        let pct_str = if stat.context_pct.is_some() {
            format!(" {}%", pct)
        } else {
            " ---%".to_string()
        };

        let bar_str = format!(
            "{}{}{}{}",
            heart,
            "▓".repeat(filled),
            "░".repeat(empty),
            pct_str,
        );

        let bar_len = 2 + filled + empty + pct_str.len(); // heart is 2 wide
        let bar_x = cx + cw.saturating_sub(bar_len as u16) / 2;

        // Draw heart (2 cells wide)
        draw_text(bar_x, bar_y, heart, Style::default().fg(bar_color), card_area, buf);

        // Draw filled portion
        let filled_str: String = "▓".repeat(filled);
        draw_text(bar_x + 2, bar_y, &filled_str, Style::default().fg(bar_color), card_area, buf);

        // Draw empty portion
        let empty_str: String = "░".repeat(empty);
        draw_text(
            bar_x + 2 + filled as u16,
            bar_y,
            &empty_str,
            Style::default().fg(Color::Rgb(85, 85, 85)),
            card_area,
            buf,
        );

        // Draw percentage
        let pct_color = if pct >= 90 { Color::Rgb(229, 115, 115) } else { Color::Rgb(136, 136, 136) };
        draw_text(
            bar_x + 2 + bar_total as u16,
            bar_y,
            &pct_str,
            Style::default().fg(pct_color),
            card_area,
            buf,
        );
    }
}
```

- [ ] **Step 5: Update `app.rs` render call to pass stats and git_cache**

In `App::render`, update the Dashboard construction:

```rust
Mode::Dashboard | Mode::DirPicker => {
    let mut dashboard = Dashboard::new(
        &self.sessions,
        &self.session_stats,
        &self.global_stats,
        &self.sprites,
        &self.nav,
        &mut self.git_cache,
    );
    dashboard.render(main_area, frame.buffer_mut());
```

- [ ] **Step 6: Build and verify it compiles**

Run: `cargo build 2>&1 | tail -10`
Expected: compiles cleanly.

- [ ] **Step 7: Commit**

```bash
git add src/ui/dashboard.rs src/app.rs
git commit -m "feat: add RPG stats line and context health bar to creature cards"
```

---

### Task 12: Dashboard UI — Git Diff in Card Titles

**Files:**
- Modify: `src/ui/dashboard.rs`

- [ ] **Step 1: Replace Block title with custom colored title rendering**

In the card rendering loop, replace the `Block::title()` usage with a borderless block + manual title drawing. After `block.render(card_area, buf)`, draw the title manually:

```rust
// Build title with git diff stats
let (git_adds, git_dels) = self.git_cache.get(&group.directory);
let diff_str = crate::git::format_diff_compact(git_adds, git_dels);

// Draw title: " name +47-12 " on top border
let title_x = card_area.x + 2;
let title_y = card_area.y;

// Project name
let name_str = format!(" {} ", display_name);
draw_text(title_x, title_y, &name_str, title_style, card_area, buf);

// Git diff after name (colored)
if !diff_str.is_empty() {
    let diff_x = title_x + name_str.len() as u16;

    // Parse the diff string to color additions green and deletions red
    // Format is like "+47-12" or "+1.2k-340"
    let mut x = diff_x;
    let mut chars = diff_str.chars().peekable();
    let mut current_color = Color::Rgb(129, 199, 132); // green for additions

    while let Some(ch) = chars.next() {
        if ch == '-' && x > diff_x {
            // Switch to red for deletions (skip leading minus)
            current_color = Color::Rgb(229, 115, 115);
        }
        let s = ch.to_string();
        draw_text(x, title_y, &s, Style::default().fg(current_color), card_area, buf);
        x += 1;
    }

    // Trailing space
    draw_text(x, title_y, " ", Style::default().fg(Color::Rgb(60, 60, 80)), card_area, buf);
}
```

Remove the `title_str` and `.title(title_str.as_str())` from the Block builder — use `Block::default().borders(...).border_style(...)` without a title, then draw the title manually.

- [ ] **Step 2: Build and verify**

Run: `cargo build 2>&1 | tail -10`
Expected: compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add src/ui/dashboard.rs
git commit -m "feat: add colored git diff stats to card titles"
```

---

### Task 13: Dashboard UI — Global Usage Status Bar

**Files:**
- Modify: `src/ui/dashboard.rs`

- [ ] **Step 1: Add usage bar rendering function**

Add a function to draw the global usage bar:

```rust
fn draw_usage_bar(
    global_stats: &GlobalStats,
    area: Rect,
    buf: &mut Buffer,
) {
    // Background
    let bg_style = Style::default().bg(Color::Rgb(25, 25, 35));
    for x in area.x..area.x + area.width {
        if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
            cell.set_symbol(" ");
            cell.set_style(bg_style);
        }
    }

    let mut x = area.x + 1;

    // 📨 N msgs
    let msgs_text = format!("📨 {} msgs", global_stats.daily_messages);
    let msgs_style = Style::default().fg(Color::Rgb(120, 120, 140)).bg(Color::Rgb(25, 25, 35));
    draw_text(x, area.y, &msgs_text, msgs_style, area, buf);
    x += msgs_text.chars().count() as u16 + 1; // +1 for emoji width

    // Separator
    draw_text(x, area.y, " │ ", Style::default().fg(Color::Rgb(85, 85, 85)).bg(Color::Rgb(25, 25, 35)), area, buf);
    x += 3;

    // ✦ Nk tokens
    let tokens_text = format!("✦ {}", format_tokens_compact(global_stats.daily_tokens));
    let tokens_style = Style::default().fg(Color::Rgb(255, 213, 79)).bg(Color::Rgb(25, 25, 35));
    draw_text(x, area.y, &tokens_text, tokens_style, area, buf);
    x += tokens_text.chars().count() as u16;

    // Separator
    draw_text(x, area.y, " │ ", Style::default().fg(Color::Rgb(85, 85, 85)).bg(Color::Rgb(25, 25, 35)), area, buf);
    x += 3;

    // ⏳ N% resets Xh Ym
    let five_hr = match global_stats.five_hour_pct {
        Some(pct) => {
            let reset = format_reset_countdown(global_stats.five_hour_resets_at);
            format!("⏳ {}% {}", pct, reset)
        }
        None => "⏳ ---".to_string(),
    };
    let five_hr_color = pct_color(global_stats.five_hour_pct, Color::Rgb(79, 195, 247));
    draw_text(x, area.y, &five_hr, Style::default().fg(five_hr_color).bg(Color::Rgb(25, 25, 35)), area, buf);
    x += five_hr.chars().count() as u16 + 1;

    // Separator
    draw_text(x, area.y, " │ ", Style::default().fg(Color::Rgb(85, 85, 85)).bg(Color::Rgb(25, 25, 35)), area, buf);
    x += 3;

    // 📅 N% resets Day
    let seven_day = match global_stats.seven_day_pct {
        Some(pct) => {
            let reset = format_reset_countdown(global_stats.seven_day_resets_at);
            format!("📅 {}% {}", pct, reset)
        }
        None => "📅 ---".to_string(),
    };
    let seven_day_color = pct_color(global_stats.seven_day_pct, Color::Rgb(129, 199, 132));
    draw_text(x, area.y, &seven_day, Style::default().fg(seven_day_color).bg(Color::Rgb(25, 25, 35)), area, buf);
}

fn pct_color(pct: Option<u8>, default: Color) -> Color {
    match pct {
        Some(p) if p >= 90 => Color::Rgb(229, 115, 115),
        Some(p) if p >= 75 => Color::Rgb(255, 213, 79),
        Some(_) => default,
        None => Color::Rgb(120, 120, 140),
    }
}

fn format_tokens_compact(tokens: u64) -> String {
    if tokens < 1_000 {
        format!("{} tokens", tokens)
    } else if tokens < 1_000_000 {
        format!("{:.0}k tokens", tokens as f64 / 1_000.0)
    } else {
        format!("{:.1}M tokens", tokens as f64 / 1_000_000.0)
    }
}

fn format_reset_countdown(resets_at: Option<i64>) -> String {
    let ts = match resets_at {
        Some(t) => t,
        None => return String::new(),
    };

    let now = chrono::Utc::now().timestamp();
    let remaining = ts - now;

    if remaining <= 0 {
        return "resets now".to_string();
    }

    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;

    if hours >= 24 {
        // Show day of week
        let dt = chrono::DateTime::from_timestamp(ts, 0);
        match dt {
            Some(d) => format!("resets {}", d.format("%a")),
            None => format!("resets {}h", hours),
        }
    } else if hours > 0 {
        format!("resets {}h {}m", hours, minutes)
    } else {
        format!("resets {}m", minutes)
    }
}
```

- [ ] **Step 2: Integrate usage bar into dashboard render**

In the `Dashboard::render` method, change the hint row to two rows (usage bar + hints) when height permits. Before the hint rendering, add:

```rust
// Draw usage bar above hints
let usage_y = area.y + area.height - 2;
if usage_y > area.y {
    let usage_area = Rect {
        x: area.x,
        y: usage_y,
        width: area.width,
        height: 1,
    };
    draw_usage_bar(self.global_stats, usage_area, buf);
}
```

Update `content_area` height to subtract 2 instead of 1 (for usage bar + hints):

```rust
let content_area = Rect {
    x: area.x,
    y: area.y,
    width: area.width,
    height: area.height.saturating_sub(2),
};
```

And move the hint row to the last row:

```rust
let hint_y = area.y + area.height - 1;
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | tail -10`
Expected: compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add src/ui/dashboard.rs
git commit -m "feat: add global usage status bar with daily stats and rate limits"
```

---

### Task 14: Integration Test — Full Build and Manual Verification

**Files:**
- No new files

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: all tests pass.

- [ ] **Step 2: Build release**

Run: `cargo build --release 2>&1 | tail -5`
Expected: compiles cleanly.

- [ ] **Step 3: Verify with `cargo clippy`**

Run: `cargo clippy 2>&1 | tail -20`
Expected: no errors. Fix any warnings if present.

- [ ] **Step 4: Commit any clippy fixes**

```bash
git add -A
git commit -m "fix: address clippy warnings"
```

(Skip if no changes needed.)
