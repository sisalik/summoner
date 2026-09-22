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
  claude_settings.rs   # ~/.claude/settings.json: read as an object (malformed is an error,
                       #   never an empty map), write once via temp file + rename, back up
                       #   the original on the first edit
  hooks.rs             # Claude Code hook installation (~/.claude/settings.json) and
                       #   state file reading (~/.summoner/claude-states/{session id})
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
    smart.rs           # Smart vs raw selection -> SpanSet (the copied cells + the text)
    markdown.rs        # Style runs -> markdown: `code`, emphasis, `##` headings,
                       #   [label](url), ``` fences
    table.rs           # Box-drawn tables -> GFM pipe tables
    links.rs           # OSC 8 hyperlinks + URL detection across soft wraps,
                       #   opening via wslview/xdg-open
    status_bar.rs      # Bottom bar: F-key session tabs with state icons, F12 hint;
                       #   shows the hovered link's target in place of the tabs
    dir_picker.rs      # Modal: recent dirs + fuzzy search (nucleo)
    session_switcher.rs # Shift+F12 overlay: fuzzy session search (nucleo), MRU
                       #   order, waiting pinned, current session dimmed
vendor/
  vt100/               # Patched vt100 0.16.2 (stream-row API, OSC 8) — see its README
tests/
  creature_test.rs
  config_test.rs
  terminal_test.rs
  ui_test.rs
  selection_test.rs
  smart_test.rs
  links_test.rs
  switcher_test.rs
```

## Architecture

**Main loop** (`app.rs`): render -> poll input (33ms dashboard / 16ms session) -> handle input -> process PTY output -> save state (every 10s).

**PTY I/O**: Each session has a `PtySession` with a background reader thread feeding chunks via MPSC channel into a `vt100::Parser`. The parser's screen state is rendered directly to the Ratatui buffer.

**Claude state detection**: Primary path reads hook state files written by a shell script (`~/.summoner/hooks/claude-state.sh`) invoked by Claude Code's hook API. Fallback checks for claude child process. Every PTY is spawned with `SUMMONER_SESSION=<session id>` in its environment, which Claude Code passes on to hook commands; the script keys its state file on that and exits on its first line when the variable is absent, so Claude Code launched outside Summoner pays nothing. The hook sits on Claude Code's critical path (PreToolUse blocks the tool, PostToolUse blocks the result), so the script spawns no external process on the common path: it reads a 4 KB prefix of the payload with the `read` builtin and matches fields with `=~`. Do not reintroduce `${x##*pat}` expansions (quadratic, ~40 ms each on 8 KB) or `command -v` probes (a PATH miss under WSL stats ~70 drvfs directories, ~150 ms). `tests/hooks_test.rs` runs the script under bash against payloads up to 5 MB.

**Claude Code settings**: `~/.claude/settings.json` is the user's file, holding their permissions, environment and their own hooks, and both `hooks.rs` and `statusline.rs` edit it in place. All of that I/O goes through `claude_settings.rs`: a file that does not parse as a JSON object is an error rather than an empty map, so a read-modify-write can never replace the user's settings with Summoner's entries alone, and a write lands in a temp file that is renamed over the original, so an interrupted write cannot truncate it. The first edit copies the original to `settings.json.summoner-bak`. Errors reach the user: `install_hooks` returns one line per failed step, shown on the dashboard and by `summoner install`.

**Session persistence**: `SessionStore` (TOML) saves all sessions every 10s and on exit. Disconnected sessions restore by re-spawning PTY in same directory and optionally running `claude --resume <conversation_id>`.

**Input routing**: Global keys (Ctrl+C/Q, Shift+F12 session switcher, F12 toggle, F1-F11 session switch) are handled first, then mode-specific handlers (dashboard nav, PTY forwarding via `key_to_bytes`, dir picker). The session switcher is an overlay (`Option<SessionSwitcher>`), not a `Mode` — it renders on top of whichever screen is active and swallows keys and mouse while open.

**Text selection**: Summoner runs nested inside the host terminal, which sees only the rendered grid — so selection, copy and link clicking are all in-app. Mouse is captured (`main.rs`), never forwarded to the inner PTY.

- Coordinates are *stream rows*: `screen.scrolled_lines() - screen.scrollback() + viewport_row`, via `selection::viewport_top`. `scrolled_lines` counts every row that ever scrolled off the top, so a selection stays glued to its text while output streams and while scrolling. It comes from the patched vt100 in `vendor/vt100` (`[patch.crates-io]`) — upstream exposes no such counter, and it can't be derived once the scrollback ring evicts rows.
- Drag selects smart (join soft wraps, strip `⏺ ⎿ >` item gutters and `│ ┃ ▏ ▎ ▍ ▌` quote bars plus box borders, dedent per gutter depth, reflow prose the producing program wrapped, and put the markdown back — see below); Alt+drag selects raw, which is the escape hatch when the text is going into a shell rather than a document. Item markers start a block, so the line above one keeps its break; bars are only containers every line of the block carries, so quoted prose still reflows. Chrome and indents are measured against the *whole* line, not the selected part — otherwise starting a drag on a line's first character makes it look un-indented and cancels the block's dedent. Double-click selects a word (underscores included, so `snake_case` is one word) or a whole URL; triple-click selects a logical line. Ctrl+C copies via OSC 52.
- `smart::compute_spans` returns the cells the copy is *made from*, and the renderer highlights the column range they span on each row. In raw mode that is the clipboard text character for character. In smart mode the clipboard also carries punctuation smart mode added — backticks, `*`/`**`/`***`, a `## ` prefix, a table's `|` and `---`, ``` fences, the `[…](…)` around a link target — so nothing is ever copied whose source cell is not highlighted, but the converse no longer holds. Every byte goes through `smart::Sink`, whose `cell`/`cell_as`/`marker` methods are the only place that distinction is drawn. Spans are recomputed every frame, which is what keeps them correct during streaming and live dragging.
- Smart mode emits markdown, inferred from the rendered attributes rather than from a hardcoded palette. The body colour is the modal foreground of the selection's own cells, and a run that differs from it becomes `` `code` `` — but only when the selection looks like prose at all (`markdown::guards`: under a 60% modal share, or over 16 distinct colours, switches inline code off), the cells are not dim, the line still has body-coloured text of its own, and the run is not a link. Italic/bold/both map to `*`/`**`/`***` as one four-valued property, so the delimiters cannot interleave, and a run is trimmed inward past its padding because CommonMark will not accept a delimiter flanked by a space. A whole line of italic + underline is a heading; the level is not recoverable from the grid, so every heading is `##`. Box-drawn tables become GFM tables (`ui::table`), anchored on a rule line and taking their column boundaries from that rule's junction glyphs — a `│` inside cell text must not invent a column. Detection runs *before* box borders are dropped and gutters stripped, because both destroy the geometry, and table and code-block regions are then excluded from the dedent, the wrap-width estimate and prose reflow. Header alignment is read from the body cells first and the header only as a fallback, since Claude Code centres every header whatever the column really is. Fenced code blocks are found by palette size, because nothing structural marks them: measured against a live session, Claude Code prints no fence characters, no language tag, no gutter glyph, no background tint and no extra indent for a code block. What it does do is syntax-highlight it — prose reaches for exactly one extra colour (the inline-code one, ~16% of its glyphs), a highlighted block for four or more (~50%) — so `markdown::detect_code_blocks` judges a whole paragraph on that. A block in a language Claude Code cannot highlight is drawn in the body colour and is genuinely indistinguishable from prose; it stays prose, and there is nothing on the grid that would let it do better. Detecting a block matters beyond the fence: a region is exempt from reflow, and without that the longest line of a code block pulls the line below it up, since the wrap width is estimated from the widest line and the widest line is always "full".
- Links come from two sources, both assembled by `links::scan_screen`. OSC 8 hyperlinks are read off the grid: the patched vt100 stamps an interned link id on every cell of the label, so a label that wraps, scrolls or repaints keeps its target. Bare URLs are found by scanning the visible rows, joining soft-wrapped rows first. OSC 8 spans are listed first, so a label that reads like a URL still resolves to the target the program declared, not the one it printed. Only `http(s)` targets become spans — `links::is_openable` gates both what is drawn as clickable and what `open_url` will launch, so nothing is ever underlined that a click would ignore. Both render underlined; Ctrl+click opens them, and hovering one shows its real target in the status bar, since an OSC 8 label can name anywhere. Under WSL the Windows browser wins (wslview, then `powershell.exe -EncodedCommand`, then `cmd.exe start`) and `xdg-open` is the last resort, since on WSL it opens a Linux browser. PowerShell is handed a base64 UTF-16LE script so no layer re-parses the URL — `&` in a query string would otherwise break it.
- All three per-frame scans are on the render path (~160 µs idle at 200x50; a full-screen smart selection costs ~350 µs a frame on top, most of it carrying one `CellStyle` per cell). Keep them allocation-light — borrow cell text via `Cow` rather than building a `String` per cell, and build logical lines in place rather than assembling and then copying them.

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
- `settings.json.summoner-bak` (in `~/.claude/`) — the settings as Summoner first found them
- `claude-states/{session id}` — per-session state files written by hooks (`{session id}.agents/` holds one marker per running subagent)
