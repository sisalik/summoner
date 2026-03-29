# Summoner

A gamified terminal session manager for Claude Code. Run multiple Claude Code sessions side by side, each represented by a procedurally-generated pixel-art creature that reflects the session's state — working, waiting, idle, or sleeping.

Sessions persist across restarts: close Summoner and reopen it later to find your sessions waiting to be resumed, complete with their Claude conversation context.

## Features

- **Multi-session management** — run multiple Claude Code (or plain shell) sessions in parallel
- **Project grouping** — sessions are grouped by working directory
- **Dashboard view** — grid of project cards with animated creatures showing session state
- **Session persistence** — sessions survive app restarts; Claude conversations resume automatically
- **Claude state detection** — hooks into Claude Code's event system for real-time state tracking
- **Keyboard-driven** — F1-F11 for quick session switching, F12 to toggle dashboard

## Install

Requires Rust 1.85+ (edition 2024).

```bash
cargo install --path .
```

This installs the `summoner` binary to `~/.cargo/bin/`, which should be on your PATH.

## Usage

```bash
summoner
```

### Keys

| Key | Action |
|-----|--------|
| `F1`–`F11` | Switch to session by index |
| `F12` | Toggle between dashboard and last session |
| `Enter` | Enter selected session |
| `n` | New session in same project directory |
| `N` | New session (pick directory) |
| `x` | Close selected session |
| `X` | Close all sessions in selected project |
| `Arrow keys` | Navigate dashboard |
| `Ctrl+C` / `Ctrl+Q` | Quit (sessions are saved) |

## How It Works

Summoner spawns PTY sessions and renders them via a VT100 emulator. It detects Claude Code's state through hooks installed in `~/.claude/settings.json` — these write state files that Summoner reads to update each creature's animation and color.

Disconnected sessions are restored by re-spawning a shell in the original directory and running `claude --resume <conversation_id>` if a conversation was active.

## Config

Configuration lives in `~/.summoner/`:

| File | Purpose |
|------|---------|
| `config.toml` | Shell, animation speed, session prune days |
| `sessions.toml` | Persisted session state |
| `recent_dirs.toml` | Directory history for the picker |
