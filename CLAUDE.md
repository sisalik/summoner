# Summoner

A gamified terminal session manager for Claude Code. Manages multiple concurrent PTY sessions across projects, with procedurally-generated pixel-art creatures that reflect each session's state. Single Rust binary, no tmux/Zellij dependency.

## Build & Run

```bash
cargo build --release
cargo run --release
cargo test
```

## Project Structure

```
src/
  main.rs              # Entry point, initializes Ratatui terminal, calls app::run
  lib.rs               # Module declarations
  app.rs               # Main event loop, state machine (Mode: Dashboard/Session/DirPicker),
                       #   session lifecycle, input routing, periodic state saves
  terminal.rs          # PtySession: PTY spawning via portable-pty, reader thread + MPSC channel,
                       #   write/resize/cwd(/proc)/try_wait
  session.rs           # Session struct, SessionState enum (Working/Waiting/Idle/Sleeping/
                       #   Disconnected/ShellOnly), project grouping (session_order)
  config.rs            # AppConfig, SessionStore, RecentDirs — all TOML, stored in ~/.summoner/
  claude.rs            # Claude process detection: find_claude_child, find_conversation_id
  hooks.rs             # Claude Code hook installation (~/.claude/settings.json) and
                       #   state file reading (~/.summoner/claude-states/{pid})
  creature/
    generate.rs        # Sprite generation: Xorshift PRNG, Bollinger mask, symmetry + edge detection
    templates.rs       # 5 creature archetypes (bipedal, quadruped, blob, winged, serpentine)
    animate.rs         # Animation state machine: locomotion/bounce/breathe/sleep per SessionState
    render.rs          # Half-block Unicode rendering, state-based color palettes
  ui/
    dashboard.rs       # Project card grid with creatures, state labels, selection
    dashboard_nav.rs   # Grid navigation (arrow keys, group-aware movement)
    session_view.rs    # vt100::Screen -> Ratatui buffer (colors, modifiers, cursor)
    status_bar.rs      # Bottom bar: F-key session tabs with state icons, F12 hint
    dir_picker.rs      # Modal: recent dirs + fuzzy search (nucleo)
tests/
  creature_test.rs
  config_test.rs
  terminal_test.rs
  ui_test.rs
```

## Architecture

**Main loop** (`app.rs`): render -> poll input (33ms dashboard / 16ms session) -> handle input -> process PTY output -> save state (every 10s).

**PTY I/O**: Each session has a `PtySession` with a background reader thread feeding chunks via MPSC channel into a `vt100::Parser`. The parser's screen state is rendered directly to the Ratatui buffer.

**Claude state detection**: Primary path reads hook state files written by a shell script (`~/.summoner/hooks/claude-state.sh`) invoked by Claude Code's hook API. Fallback checks for claude child process.

**Session persistence**: `SessionStore` (TOML) saves all sessions every 10s and on exit. Disconnected sessions restore by re-spawning PTY in same directory and optionally running `claude --resume <conversation_id>`.

**Input routing**: Global keys (Ctrl+C/Q, F12 toggle, F1-F11 session switch) are handled first, then mode-specific handlers (dashboard nav, PTY forwarding via `key_to_bytes`, dir picker).

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` + `crossterm` | TUI rendering and terminal I/O |
| `portable-pty` | Cross-platform PTY spawning |
| `vt100` | Terminal emulation (ANSI parsing) |
| `nucleo-matcher` | Fuzzy matching in dir picker |
| `serde` + `toml` | Config/session serialization |
| `uuid` | Session identifiers |
| `chrono` | Timestamps for session pruning |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SUMMONER_ALLOW_ALT_SCREEN` | unset | Set to `1` to stop stripping alternate-screen escape sequences (`\e[?1049h/l`, `\e[?47h/l`) from PTY output, and to stop forcing `CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1` on spawned sessions. By default Summoner both strips these sequences and exports `CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1` so Claude Code (which renders fullscreen via the alternate screen since v2.1.89) keeps its conversation in vt100's native scrollback. Note: agent-view re-attached background sessions force fullscreen regardless and cannot be kept in scrollback. Only set this if the upstream program has fixed its scrollback handling. |

## Config Files (`~/.summoner/`)

- `config.toml` — shell, animation speed, prune days
- `sessions.toml` — persisted sessions (id, dir, creature seed/template, conversation id, timestamps)
- `recent_dirs.toml` — directory history for picker
- `hooks/claude-state.sh` — auto-installed hook script
- `claude-states/{pid}` — per-shell state files written by hooks
