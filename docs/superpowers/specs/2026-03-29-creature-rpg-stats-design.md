# Creature RPG Stats & Usage Dashboard

Add RPG-style stats to creature cards, a context window health bar, git diff stats in card titles, and a global usage status bar. Replace the redundant state label (already conveyed by animation + color) with richer, game-like session info.

## Decisions

- **Card stats line**: replaces state label — shows active tool (or state fallback), level, and XP
- **Health bar**: context window usage %, color-coded, below stats line
- **Card title**: appends git uncommitted `+additions-deletions` after project name
- **Global status bar**: daily messages, total tokens, 5-hour and weekly usage % with reset countdowns
- **Data sources**: Claude Code hooks (tool events), statusLine JSON (context window, rate limits), JSONL conversation files (tokens, messages), `git diff --stat` (uncommitted lines)
- **StatusLine integration**: wrapper script around existing ccstatusline, writes JSON to state file per session
- **No oauth/usage API**: rate limits come from statusLine's `rate_limits` field instead
- **Emoji style**: use emoji throughout for playful RPG feel

## Data Collection

### 1. Enhanced Hook Script

Extend the existing `~/.summoner/hooks/claude-state.sh` to also handle `PreToolUse` and `PostToolUse` events. Write to the existing state file format with additional fields.

**New events to register:**
- `PreToolUse` — extract `tool_name` from stdin JSON
- `PostToolUse` — extract `tool_name` from stdin JSON

**State file format** (`~/.summoner/claude-states/{shell_pid}`):

Current: `<event> <session_id>`
New: `<event> <session_id> [tool_name]`

The third field is optional — only present for PreToolUse/PostToolUse events. Existing parsing in `hooks.rs` remains backward-compatible (it reads the first two fields via `split_whitespace`).

**Tool display mapping** in Rust:

| `tool_name` value | Display label |
|---|---|
| `Edit` | `✏️ Editing` |
| `Write` | `📝 Writing` |
| `Read` | `📖 Reading` |
| `Bash` | `⚙️ Running` |
| `Glob` | `🔍 Searching` |
| `Grep` | `🔍 Searching` |
| `Agent` | `🤖 Delegating` |
| `WebSearch` | `🌐 Browsing` |
| `WebFetch` | `🌐 Fetching` |
| Other | `🔧 {tool_name}` |

When a `PostToolUse` is received, the active tool is cleared. When the session is not actively using a tool, fall back to the state icon + label (e.g. `⚡ Working`, `◆ Idle`, `❓ Waiting`).

### 2. StatusLine Wrapper

Replace the direct ccstatusline invocation with a wrapper that tees the JSON to a per-session file that Summoner reads.

**Wrapper script** (`~/.summoner/hooks/statusline-wrapper.sh`):
- Reads JSON from stdin
- Extracts `session_id` from the JSON
- Writes the full JSON to `~/.summoner/statusline-states/{session_id}.json`
- Pipes the same JSON to `npx -y ccstatusline@latest`

**Settings change** in `~/.claude/settings.json`:
```json
{
  "statusLine": {
    "type": "command",
    "command": "bash ~/.summoner/hooks/statusline-wrapper.sh",
    "padding": 0
  }
}
```

Summoner installs this on startup (similar to how it installs hooks). If the user's existing statusLine is not ccstatusline, preserve whatever command they had and pipe to that instead. Store the original command in `~/.summoner/config.toml` under `statusline_passthrough`.

**StatusLine JSON fields consumed by Summoner:**

| Field | Use |
|---|---|
| `context_window.used_percentage` | Health bar on creature card |
| `context_window.context_window_size` | Health bar tooltip/detail |
| `rate_limits.five_hour.used_percentage` | Global status bar |
| `rate_limits.five_hour.resets_at` | Global status bar countdown |
| `rate_limits.seven_day.used_percentage` | Global status bar |
| `rate_limits.seven_day.resets_at` | Global status bar countdown |
| `session_id` | Map statusLine data to the correct session (matches `claude_conversation_id` on Session) |
| `model.display_name` | Potential future use (RPG "class") |

### 3. JSONL Conversation Files

Parse JSONL conversation files for per-session token and message counts.

**Locating the JSONL file:** Claude Code stores conversations at `~/.claude/projects/{project_hash}/{session_id}/`. The `project_hash` is derived from the working directory path (e.g. `-home-siim-dev-summoner`). The `session_id` matches the session's `claude_conversation_id`. Scan for the main JSONL file in this directory (typically named after an agent ID, e.g. `agent-{id}.jsonl`).

**Data extracted per session:**
- Total input + output tokens → XP value
- Message count (user + assistant lines)

**When to parse:**
- On session creation / restoration — read existing JSONL to get cumulative stats
- Periodically (every ~60s) — re-read to update as the session progresses
- Keep a byte offset to avoid re-parsing the entire file each time (seek to last known position, read new lines)

**Session-to-JSONL mapping:** Each Summoner session has a `claude_conversation_id` (from hooks or claude.rs detection). Use this as the `session_id` directory name when locating JSONL files. If `claude_conversation_id` is None, skip JSONL parsing for that session.

**Level calculation** from total tokens:

| Level | Token threshold |
|---|---|
| Lv.1 | 0 |
| Lv.2 | 10k |
| Lv.3 | 50k |
| Lv.4 | 150k |
| Lv.5 | 500k |
| Lv.6 | 1M |
| Lv.7 | 2.5M |
| Lv.8 | 5M |
| Lv.9 | 10M |
| Lv.10 | 25M |

Exponential curve — early levels come fast, later levels represent marathon sessions. These thresholds can be tuned.

**XP display format:**
- `< 1000` → raw number (e.g. `✦847`)
- `1000–999999` → `✦12.4k`
- `≥ 1000000` → `✦1.2M`

### 4. Git Diff Stats

Run `git diff --stat` in each project's working directory to get uncommitted line counts.

**Polling:** Every ~30 seconds, spawn `git diff --shortstat` for each unique project directory. Parse output like `3 files changed, 47 insertions(+), 12 deletions(-)`.

**Display format in card title:**
- Changes present: `summoner +47-12`
- Large diffs: `summoner +1.2k-340`
- Clean tree: `summoner` (no suffix)
- Colors: additions in green, deletions in red

**Caching:** Store per-directory, shared across sessions in the same project. Only re-run git if the directory has active sessions.

## UI Layout Changes

### Creature Card — Stats Line + Health Bar

Replace the single state label row (line 253-261 in `dashboard.rs`) with two rows: stats line and health bar.

**Stats line (row 1):**
```
⚡ Editing  Lv.3  ✦12.4k
```
- Left: active tool or state fallback (icon + label)
- Center-right: level
- Right: XP

Centered as a group under the creature, same as the current label.

**Health bar (row 2):**
```
💚▓▓▓▓▓▓▓▓░░░░░░░░░░ 42%
```
- Emoji heart prefix: 💚 green (0-50%), 💛 yellow (50-75%), 🧡 orange (75-90%), ❤️ red (90-100%)
- Filled portion: `▓` characters in matching color
- Empty portion: `░` characters in dim gray
- Percentage number at end, colored red when ≥90%
- Bar width: adapts to available card width (creature width as guide)
- Only shown for Claude sessions. Shell-only sessions show just the state label, no bar.

**Card height impact:** `card_inner_height` gains 1 row (was: padding + creature + label + padding = 1 + creature_render_h + 1 + 1; now: 1 + creature_render_h + 1 + 1 + 1). The extra row is the health bar.

### Card Title — Git Diff

Modify the title string construction (line 208 in `dashboard.rs`):

Current: `format!(" {} ", display_name)`
New: `format!(" {} +{}-{} ", display_name, additions, deletions)` when changes exist.

Additions rendered in green (`Color::Rgb(129, 199, 132)`), deletions in red (`Color::Rgb(229, 115, 115)`). Requires custom title rendering since ratatui's `Block::title()` uses a single style — draw the title manually with per-span coloring.

### Global Status Bar

Add a new row at the bottom of the dashboard, between the card grid and the keyboard hints.

```
📨 38 msgs │ ✦ 284k tokens │ ⏳ 42% resets 4h 12m │ 📅 67% resets Thu
```

**Layout:**
- `📨 {n} msgs` — daily message count summed across all active sessions (from each session's JSONL parsing, filtered by today's timestamps, reset at midnight)
- `✦ {n} tokens` — total tokens today summed across all active sessions (from JSONL, filtered by today's timestamps)
- `⏳ {n}% resets {countdown}` — 5-hour usage from statusLine rate_limits, countdown computed from `resets_at` unix timestamp
- `📅 {n}% resets {day}` — weekly usage, reset time shown as day of week if >24h away, otherwise as countdown

**Colors:**
- Messages: dim gray
- Tokens: gold
- 5-hour %: blue (or yellow/red if >75%/90%)
- Weekly %: green (or yellow/red if >75%/90%)

**Space:** The dashboard currently reserves 1 row at the bottom for hints. Change to 2 rows: usage bar + hints. Or combine them if terminal height is tight (show usage bar, hide hints when height < threshold).

## New Data Structures

### SessionStats

```rust
struct SessionStats {
    total_tokens: u64,        // input + output tokens
    message_count: u32,       // user + assistant messages
    active_tool: Option<String>,  // current tool name from hooks
    context_pct: Option<u8>,  // context window usage % from statusLine
    jsonl_offset: u64,        // byte offset for incremental JSONL parsing
}
```

Stored alongside each `Session` in the `App` struct (parallel vec or embedded in `Session`).

### GlobalStats

```rust
struct GlobalStats {
    daily_messages: u32,
    daily_tokens: u64,
    five_hour_pct: Option<u8>,
    five_hour_resets_at: Option<i64>,   // unix timestamp
    seven_day_pct: Option<u8>,
    seven_day_resets_at: Option<i64>,   // unix timestamp
    last_daily_reset: chrono::NaiveDate,
}
```

Stored in `App`. Daily counters reset when `last_daily_reset != today`.

### GitDiffStats

```rust
struct GitDiffStats {
    additions: u32,
    deletions: u32,
    last_checked: Instant,
}
```

Stored per project directory (HashMap in `App`).

## File Changes

| File | Change |
|---|---|
| `src/session.rs` | Add `SessionStats` struct, embed in `Session` or keep parallel |
| `src/hooks.rs` | Register `PreToolUse`/`PostToolUse` events, parse tool_name from state file, install statusLine wrapper |
| `src/app.rs` | Add `GlobalStats`, `GitDiffStats` map, periodic JSONL parsing, git diff polling, statusLine file reading |
| `src/ui/dashboard.rs` | Stats line + health bar rendering, custom title with git diff colors, extra row for status bar |
| `src/ui/status_bar.rs` | No change (this is the session-mode F-key bar) |
| New: `src/stats.rs` | JSONL parser, token/message counting, level calculation, XP formatting |
| New: `src/statusline.rs` | StatusLine JSON parsing, wrapper script installation |
| New: `src/git.rs` | `git diff --shortstat` runner, output parsing, caching |

## Edge Cases

- **StatusLine not available:** If `statusline-states/{session_id}.json` doesn't exist (e.g. new session, ccstatusline not working), show health bar as empty/gray with `---%`. Rate limits in status bar show `---`.
- **JSONL file not found:** Session may not have a conversation yet. Show Lv.1, ✦0. Parse will succeed once Claude starts responding.
- **Git not installed or not a repo:** `git diff` will fail. Show no diff stats in title (clean display).
- **Multiple sessions same project:** Git diff stats are per-directory, shared across all sessions in that project card.
- **Session restored after restart:** Re-parse JSONL from beginning to reconstruct token/message counts. The byte offset resets to 0.
- **StatusLine wrapper and existing config:** On first install, read the current `statusLine.command` from settings, store as passthrough target in config.toml, then replace with wrapper. On subsequent runs, only update if the wrapper isn't already installed.
- **Emoji rendering width:** Some terminals render emoji as 2 cells wide, some as 1. Use unicode-width crate or hardcode emoji as 2-wide for layout math. Test with common terminals (kitty, alacritty, WezTerm, Windows Terminal).
