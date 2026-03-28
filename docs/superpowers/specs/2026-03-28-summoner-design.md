# Summoner — Design Spec

A gamified terminal session manager for Claude Code. Manages multiple shell/Claude Code sessions across projects, with procedurally generated pixel-art creatures representing each session's state.

## Architecture

Single Rust binary. No external dependencies (no tmux, no Zellij).

### Module structure

```
summoner
├── app          — main event loop, state machine (Dashboard <-> Session view)
├── terminal     — PTY spawning (portable-pty), terminal emulation (vt100), I/O routing
├── session      — session lifecycle, persistence, Claude Code state detection
├── creature     — procedural generation (Bollinger masks), animation frames, rendering
├── ui
│   ├── dashboard    — F12 fullscreen grid view (projects -> creatures)
│   ├── session_view — embedded terminal rendering (vt100 screen -> Ratatui buffer)
│   └── status_bar   — 1-row bottom bar with session tabs + state icons
└── config       — settings, key bindings, session store (~/.summoner/)
```

### Core crates

| Crate | Purpose |
|---|---|
| ratatui | Immediate-mode TUI rendering |
| crossterm | Terminal backend (input events, raw mode) |
| portable-pty | Cross-platform PTY spawning and management |
| vt100 | Terminal state parsing (escape sequences -> screen buffer) |
| tokio | Async runtime for PTY I/O multiplexing |
| serde + toml | Config and session persistence |

### Event loop

Two rendering modes based on the app state:

**Session view (event-driven)**:
- PTY output events trigger redraws of the session area. Render happens when there's new data — no fixed tick. This gives the lowest possible latency between shell output and display.
- Bottom status bar polls background session states every 1-2 seconds. Each poll checks the vt100 screen's last line for Claude Code status text.
- Keyboard input (except F-keys) forwarded directly to the active session's PTY.

**Dashboard view (~30fps fixed tick)**:
- Fixed tick loop for creature animations.
- When the dashboard is not visible, creatures do not animate — no wasted work.

### State machine

Two modes:
- `Dashboard` — fullscreen grid of project cards with creatures
- `Session(id)` — fullscreen embedded terminal + bottom status bar

Transitions:
- F12 toggles between Dashboard and the last active session
- F1-F11 jump directly to session by index (from either mode)
- Closing the last session in session view returns to Dashboard

## Session & PTY Management

### Creating a session

From the dashboard:
- `n` on a selected project card -> spawns a new shell session in that project's directory
- `N` (shift+n) -> opens the new directory picker:
  1. Recent directories shown at the top (most recent first, ~10-15 entries)
  2. Fuzzy finder below — type to search filesystem paths interactively (fzf-style)
  3. Select a directory -> shell spawns there, new project card created if needed

Each new session gets:
- A unique ID (UUID)
- A procedurally generated creature (seeded from the UUID)
- Association with its project directory

### Claude Code detection

When the user launches `claude` inside a session's shell, Summoner detects it by monitoring the vt100 screen for Claude Code's status bar patterns:

| vt100 status bar text | Detected state |
|---|---|
| "esc to interrupt" | Working |
| "Esc to cancel" | Waiting for input |
| Claude Code visible, no above patterns | Idle |
| No Claude Code detected | Shell only |

Additionally, Summoner scans `~/.claude/sessions/` for session files matching the PTY's PID to capture the Claude conversation ID for later resume.

### Session persistence

Stored in `~/.summoner/sessions.toml`:

```toml
[[session]]
id = "a1b2c3d4"
directory = "/home/siim/dev/summoner"
creature_seed = 2847103956
creature_template = "bipedal"
claude_conversation_id = "abc-123-def"
last_active = 2026-03-28T21:30:00Z
active = false
```

Lifecycle:
- Shell spawns -> session entry written
- Claude Code detected -> `claude_conversation_id` updated
- Session closed -> entry marked `active = false` (kept for restore)
- App quits (clean or crash) -> all sessions saved
- On relaunch -> inactive sessions shown as disconnected/sleeping on dashboard
- User can restore a session: re-spawns shell in the directory, optionally runs `claude --resume <conversation-id>`
- Inactive sessions pruned after configurable period (default 30 days)

### Session cleanup

Sessions can be closed from the dashboard (`d` key with confirmation). Sends SIGHUP to the PTY process.

## Creature System

### Procedural generation

Uses the Bollinger mask algorithm (as implemented in zfedoran/pixel-sprite-generator):

1. Define a half-mask template (e.g., 6x14 grid) with cell values:
   - `-1` = always border
   - `0` = always empty
   - `1` = 50% chance body, 50% empty
   - `2` = 50% chance body, 50% border
2. Seed a xorshift PRNG from the session UUID
3. Randomize probabilistic cells
4. Mirror horizontally for symmetry
5. Run edge detection: body pixels adjacent to empty space become outline pixels

### Mask templates

4-5 creature archetypes, selected by `seed % num_templates`:
- Bipedal (humanoid/robot)
- Quadruped (dog/cat/beast)
- Blob (amorphous, amoeba-like)
- Winged (bird/bat/dragon)
- Serpentine (snake/worm)

### Sprite dimensions

- 10-12 pixels wide, 14-16 pixels tall
- Rendered with half-block Unicode characters (U+2580, U+2584) — 2 vertical pixels per terminal cell
- Effective terminal footprint: ~6 columns x 8 rows per creature

### Color palette by state

Color is determined entirely by session state, not by the creature's seed. This makes state instantly readable at a glance.

| State | Palette | Meaning |
|---|---|---|
| Working | Greens/teals | Active, "go" signal |
| Waiting | Amber/orange, pulsing | Needs attention |
| Idle | Blues/purples | Calm, at rest |
| Sleeping | Dark greys/muted blues | Powered down |
| Disconnected | Desaturated greys | Inactive |
| New (shell only) | White/light neutral | Blank slate |

Creature shape (silhouette) is the sole identifier for distinguishing creatures within the same state.

### Animation

Simple frame transforms applied to the base sprite:

| State | Animation style | Frames | Frame duration |
|---|---|---|---|
| Working | Locomotion — walking/running (ground), flapping (winged), slithering (serpentine) | 4 | ~200ms |
| Waiting | Bouncing in place + "!" particle above head | 3 | ~300ms |
| Idle | Slow breathing (subtle vertical squash/stretch) | 2 | ~800ms |
| Sleeping | Squashed down, "z z z" particles float upward | 3 | ~600ms |
| Disconnected | Static, greyed out, no animation | 1 | — |
| New (shell only) | Egg or terminal icon, gentle wobble | 2 | ~500ms |

Animation frames are derived programmatically from the base sprite (pixel shifts, squash/stretch, particle overlays) — not separate hand-drawn frames.

## UI

### Bottom status bar (always visible)

Single row at the very bottom of the terminal:

```
[F1 🏃 api-server] [F2 ⏳ website] [F3 💤 tools]          [F12 Dashboard]
```

- Each tab: F-key number, state icon (Unicode, colored by state palette), session name (truncated to fit)
- Active session tab highlighted (bold/inverse)
- Tab text colored by state
- Right-aligned: `F12 Dashboard` hint
- Overflow: `+N more` indicator when sessions exceed available width

### Dashboard (F12)

Fullscreen grid of project cards.

**Dynamic layout**:
- 1 project: single centered card, larger
- 2-3 projects: single row, equal width
- 4-6 projects: 2 rows, 2-3 columns
- 7+: 3 columns, vertically scrollable
- Card height scales with number of creatures inside

**Card contents**:
- Header: project directory name
- Body: creatures lined up, animated by state, colored by state
- Footer: session count, attention indicators

**Navigation** (two levels):
- Arrow keys to navigate between project cards
- `Enter` on a card to expand it / enter selection mode within the card
  - Arrow keys to select individual sessions (creatures) within the card
  - `Enter` to jump into the selected session
  - `Esc` to go back to card-level navigation
- `n` to spawn a new session in the selected project's directory
- `N` to open a new directory (recent dirs + fuzzy finder overlay)
- `d` to close the selected session (with confirmation)
- `q` to return to the last active session

### Session view

- Full terminal area minus the 1-row bottom bar
- Active session's PTY output rendered by mapping the vt100 screen buffer into a Ratatui buffer
- All keyboard input except F-keys forwarded to the PTY
- F1-F11 switch sessions, F12 opens dashboard

### New directory picker (overlay)

Appears over the dashboard when `N` is pressed:
1. Recent directories list at top (most recent first, 10-15 entries)
2. Fuzzy finder input field below — filters filesystem paths as you type
3. Enter selects, Esc cancels

## Config & Persistence

### Directory structure

```
~/.summoner/
├── config.toml        — user settings
├── sessions.toml      — saved session state
└── recent_dirs.toml   — recently used directories
```

### config.toml

```toml
[general]
default_shell = "/bin/bash"
animation_speed = 1.0          # multiplier (0.5 = half speed, 2.0 = double)
status_bar_poll_interval = 2   # seconds
session_prune_days = 30

[new_session]
recent_dirs_count = 15
```

### First launch

No config directory exists. App creates `~/.summoner/`, starts with an empty dashboard, and immediately opens the new directory fuzzy finder.

## Target environment

- **OS**: Linux (WSL2 primary target)
- **Terminal**: Windows Terminal (works out of the box for half-block rendering, true color, mouse support). WezTerm as upgrade path for Kitty graphics protocol support.
- **Shell**: User's default shell (configurable)
- **Distribution**: Single binary, installable via `cargo install`

## Future extensions (out of scope for v1)

These are noted for awareness but explicitly excluded from initial implementation:

- Stats tracking (lines written, commits made, todos parsed)
- Sound effects
- Custom creature skins/themes
- Key rebinding UI
- Remote session support
- Kitty/Sixel image protocol rendering (for higher-res creatures)
