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
    generate.rs        # Xorshift PRNG, CellKind/Sprite buffers
    skeleton.rs        # 5 archetypes (bipedal, quadruped, blob, winged, serpentine):
                       #   chain points, distance constraints, two-bone-IK limbs
    physics.rs         # Verlet integration, constraint solver, two-bone IK (bend_dir)
    locomotion/        # Per-SessionState kinematic drivers, one file per archetype;
                       #   personality structs sampled from a rest-pose hash —
                       #   see docs/creature-animation.md
    outline.rs         # Skeleton -> pixels: SDF capsule rasterization, edge detection
    render.rs          # Half-block Unicode rendering, state palettes, capsule shading
  ui/
    dashboard.rs       # Project card grid with creatures, state labels, selection
    dashboard_nav.rs   # Grid navigation (arrow keys, group-aware movement)
    session_view.rs    # vt100::Screen -> Ratatui buffer (colors, modifiers, cursor,
                       #   selection highlight, URL underline)
    selection.rs       # Mouse selection state in stream-row coordinates,
                       #   word/line select, OSC 52 clipboard
    smart.rs           # Smart vs raw selection -> SpanSet (the exact copied cells)
    links.rs           # URL detection across soft wraps, opening via wslview/xdg-open
    status_bar.rs      # Bottom bar: F-key session tabs with state icons, F12 hint
    dir_picker.rs      # Modal: recent dirs + fuzzy search (nucleo)
vendor/
  vt100/               # Patched vt100 0.16.2 (stream-row API) — see its README
tests/
  creature_test.rs
  config_test.rs
  terminal_test.rs
  ui_test.rs
  selection_test.rs
  smart_test.rs
  links_test.rs
```

## Architecture

**Main loop** (`app.rs`): render -> poll input (33ms dashboard / 16ms session) -> handle input -> process PTY output -> save state (every 10s).

**PTY I/O**: Each session has a `PtySession` with a background reader thread feeding chunks via MPSC channel into a `vt100::Parser`. The parser's screen state is rendered directly to the Ratatui buffer.

**Claude state detection**: Primary path reads hook state files written by a shell script (`~/.summoner/hooks/claude-state.sh`) invoked by Claude Code's hook API. Fallback checks for claude child process.

**Session persistence**: `SessionStore` (TOML) saves all sessions every 10s and on exit. Disconnected sessions restore by re-spawning PTY in same directory and optionally running `claude --resume <conversation_id>`.

**Input routing**: Global keys (Ctrl+C/Q, F12 toggle, F1-F11 session switch) are handled first, then mode-specific handlers (dashboard nav, PTY forwarding via `key_to_bytes`, dir picker).

**Text selection**: Summoner runs nested inside the host terminal, which sees only the rendered grid — so selection, copy and link clicking are all in-app. Mouse is captured (`main.rs`), never forwarded to the inner PTY.

- Coordinates are *stream rows*: `screen.scrolled_lines() - screen.scrollback() + viewport_row`, via `selection::viewport_top`. `scrolled_lines` counts every row that ever scrolled off the top, so a selection stays glued to its text while output streams and while scrolling. It comes from the patched vt100 in `vendor/vt100` (`[patch.crates-io]`) — upstream exposes no such counter, and it can't be derived once the scrollback ring evicts rows.
- Drag selects smart (join soft wraps, strip `⏺ │ ⎿ >` gutters and box borders, dedent per gutter depth, reflow prose the producing program wrapped); Alt+drag selects raw. Chrome and indents are measured against the *whole* line, not the selected part — otherwise starting a drag on a line's first character makes it look un-indented and cancels the block's dedent. Double-click selects a word (underscores included, so `snake_case` is one word) or a whole URL; triple-click selects a logical line. Ctrl+C copies via OSC 52.
- `smart::compute_spans` returns the exact cells that will be copied, and the renderer highlights those cells — the highlight is what the clipboard gets. It is recomputed every frame, which is what keeps it correct during streaming and live dragging.
- URLs are found by scanning the visible rows, joining soft-wrapped rows first (`links::scan_screen`). They render underlined; Ctrl+click opens them. Under WSL the Windows browser wins (wslview, then `powershell.exe -EncodedCommand`, then `cmd.exe start`) and `xdg-open` is the last resort, since on WSL it opens a Linux browser. PowerShell is handed a base64 UTF-16LE script so no layer re-parses the URL — `&` in a query string would otherwise break it.
- Both per-frame scans are on the render path (~160 µs idle at 200x50). Keep them allocation-light — borrow cell text via `Cow` rather than building a `String` per cell.

**Creature animation**: Design principles, canvas constraints, and the dev/verification workflow (`--test-creatures`, `--render-creature`, both behind the `dev-creature` feature) are documented in [docs/creature-animation.md](docs/creature-animation.md). Read it before changing anything under `src/creature/`.

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` + `crossterm` | TUI rendering and terminal I/O |
| `portable-pty` | Cross-platform PTY spawning |
| `vt100` | Terminal emulation (ANSI parsing) — patched fork in `vendor/vt100` |
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
