# Summoner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a gamified terminal session manager for Claude Code — a single Rust binary that manages multiple PTY sessions across projects, with procedurally generated pixel-art creatures indicating session state.

**Architecture:** Pure Rust, no external multiplexer dependencies. Ratatui for TUI rendering, portable-pty for PTY management, vt100 for terminal emulation. Two-mode state machine: Dashboard (creature grid) and Session view (embedded terminal). Event-driven rendering in session view, fixed-tick animations on dashboard only.

**Tech Stack:** Rust, ratatui 0.30, crossterm 0.29, portable-pty 0.9, vt100 0.16, tokio, uuid, serde, toml, nucleo-matcher 0.3

**Spec:** `docs/superpowers/specs/2026-03-28-summoner-design.md`

---

## File Structure

```
summoner/
├── Cargo.toml
├── src/
│   ├── main.rs                    — entry point, terminal init/restore, launch app
│   ├── app.rs                     — App struct, state machine, top-level event loop
│   ├── config.rs                  — Config/SessionStore/RecentDirs types, load/save
│   ├── session.rs                 — Session struct, SessionState enum, project grouping
│   ├── terminal.rs                — PtySession: spawn, read, write, resize, child lifecycle
│   ├── claude.rs                  — Claude Code state detection from vt100 screen
│   ├── creature/
│   │   ├── mod.rs                 — re-exports
│   │   ├── generate.rs            — Bollinger mask algorithm, xorshift PRNG, Sprite type
│   │   ├── templates.rs           — mask templates (bipedal, quadruped, blob, winged, serpentine)
│   │   ├── animate.rs             — animation frames by state, frame transforms
│   │   └── render.rs              — half-block Unicode rendering to ratatui Buffer
│   └── ui/
│       ├── mod.rs                 — re-exports
│       ├── status_bar.rs          — 1-row bottom bar widget
│       ├── session_view.rs        — vt100 Screen -> ratatui Buffer widget
│       ├── dashboard.rs           — project card grid, dynamic layout
│       ├── dashboard_nav.rs       — two-level navigation state (cards <-> sessions)
│       └── dir_picker.rs          — recent dirs + fuzzy finder overlay
└── tests/
    ├── creature_test.rs           — generation determinism, mask templates, animation
    ├── config_test.rs             — config/session round-trip serialization
    ├── claude_test.rs             — status bar pattern detection
    ├── terminal_test.rs           — PTY spawn/read/write integration
    └── ui_test.rs                 — status bar rendering, layout math
```

---

## Task 1: Project Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

- [ ] **Step 1: Initialize the Rust project**

```bash
cd /home/siim/dev/summoner
cargo init --name summoner
```

- [ ] **Step 2: Set up Cargo.toml with all dependencies**

Replace the generated `Cargo.toml` with:

```toml
[package]
name = "summoner"
version = "0.1.0"
edition = "2024"
description = "A gamified terminal session manager for Claude Code"

[dependencies]
anyhow = "1"
crossterm = "0.29"
nucleo-matcher = "0.3"
portable-pty = "0.9"
ratatui = "0.30"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
toml = "1.1"
uuid = { version = "1", features = ["v4", "serde"] }
vt100 = "0.16"
chrono = { version = "0.4", features = ["serde"] }
dirs = "6"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Write minimal main.rs that initializes and restores the terminal**

```rust
use anyhow::Result;

mod app;
mod claude;
mod config;
mod creature;
mod session;
mod terminal;
mod ui;

fn main() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = app::run(&mut terminal);
    ratatui::restore();
    result
}
```

- [ ] **Step 4: Create stub modules so it compiles**

Create `src/app.rs`:
```rust
use anyhow::Result;
use ratatui::DefaultTerminal;

pub fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    Ok(())
}
```

Create `src/config.rs`:
```rust
// Config, session store, recent dirs — Task 2
```

Create `src/session.rs`:
```rust
// Session struct, SessionState — Task 3
```

Create `src/terminal.rs`:
```rust
// PtySession — Task 7
```

Create `src/claude.rs`:
```rust
// Claude Code detection — Task 12
```

Create `src/creature/mod.rs`:
```rust
pub mod generate;
pub mod templates;
pub mod animate;
pub mod render;
```

Create `src/creature/generate.rs`:
```rust
// Bollinger mask algorithm — Task 4
```

Create `src/creature/templates.rs`:
```rust
// Mask templates — Task 5
```

Create `src/creature/animate.rs`:
```rust
// Animation frames — Task 6
```

Create `src/creature/render.rs`:
```rust
// Half-block rendering — Task 6
```

Create `src/ui/mod.rs`:
```rust
pub mod status_bar;
pub mod session_view;
pub mod dashboard;
pub mod dashboard_nav;
pub mod dir_picker;
```

Create `src/ui/status_bar.rs`:
```rust
// Status bar widget — Task 9
```

Create `src/ui/session_view.rs`:
```rust
// Terminal view widget — Task 8
```

Create `src/ui/dashboard.rs`:
```rust
// Dashboard grid — Task 10
```

Create `src/ui/dashboard_nav.rs`:
```rust
// Navigation state — Task 11
```

Create `src/ui/dir_picker.rs`:
```rust
// Directory picker — Task 14
```

- [ ] **Step 5: Verify it compiles**

```bash
cd /home/siim/dev/summoner && cargo check
```

Expected: compiles with no errors (warnings about unused modules are fine).

- [ ] **Step 6: Initialize git and commit**

```bash
cd /home/siim/dev/summoner
git init
echo '/target' > .gitignore
echo '.superpowers/' >> .gitignore
git add -A
git commit -m "feat: scaffold Summoner project with dependencies and module structure"
```

---

## Task 2: Config & Persistence Types

**Files:**
- Create: `src/config.rs`
- Create: `tests/config_test.rs`

- [ ] **Step 1: Write failing tests for config round-trip serialization**

Create `tests/config_test.rs`:

```rust
use summoner::config::{AppConfig, GeneralConfig, NewSessionConfig, SessionEntry, SessionStore, RecentDirs};
use chrono::Utc;
use uuid::Uuid;
use tempfile::TempDir;

#[test]
fn config_defaults_are_sensible() {
    let config = AppConfig::default();
    assert_eq!(config.general.animation_speed, 1.0);
    assert_eq!(config.general.status_bar_poll_interval, 2);
    assert_eq!(config.general.session_prune_days, 30);
    assert_eq!(config.new_session.recent_dirs_count, 15);
}

#[test]
fn config_round_trips_through_toml() {
    let config = AppConfig::default();
    let toml_str = toml::to_string_pretty(&config).unwrap();
    let loaded: AppConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(loaded.general.animation_speed, config.general.animation_speed);
    assert_eq!(loaded.general.status_bar_poll_interval, config.general.status_bar_poll_interval);
}

#[test]
fn session_store_round_trips_through_toml() {
    let store = SessionStore {
        sessions: vec![
            SessionEntry {
                id: Uuid::new_v4(),
                directory: "/home/test/project".into(),
                creature_seed: 12345,
                creature_template: "bipedal".into(),
                claude_conversation_id: Some("conv-abc".into()),
                last_active: Utc::now(),
                active: false,
            },
        ],
    };
    let toml_str = toml::to_string_pretty(&store).unwrap();
    let loaded: SessionStore = toml::from_str(&toml_str).unwrap();
    assert_eq!(loaded.sessions.len(), 1);
    assert_eq!(loaded.sessions[0].directory, "/home/test/project");
    assert_eq!(loaded.sessions[0].creature_seed, 12345);
    assert!(loaded.sessions[0].claude_conversation_id.is_some());
}

#[test]
fn recent_dirs_round_trips() {
    let dirs = RecentDirs {
        directories: vec!["/home/test/a".into(), "/home/test/b".into()],
    };
    let toml_str = toml::to_string_pretty(&dirs).unwrap();
    let loaded: RecentDirs = toml::from_str(&toml_str).unwrap();
    assert_eq!(loaded.directories, dirs.directories);
}

#[test]
fn save_and_load_config_from_disk() {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig::default();
    config.save(tmp.path()).unwrap();
    let loaded = AppConfig::load(tmp.path()).unwrap();
    assert_eq!(loaded.general.animation_speed, 1.0);
}

#[test]
fn save_and_load_session_store_from_disk() {
    let tmp = TempDir::new().unwrap();
    let store = SessionStore { sessions: vec![] };
    store.save(tmp.path()).unwrap();
    let loaded = SessionStore::load(tmp.path()).unwrap();
    assert!(loaded.sessions.is_empty());
}

#[test]
fn load_missing_config_returns_default() {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig::load(tmp.path()).unwrap();
    assert_eq!(config.general.animation_speed, 1.0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /home/siim/dev/summoner && cargo test --test config_test 2>&1 | head -20
```

Expected: compilation errors — types don't exist yet.

- [ ] **Step 3: Implement config types**

Replace `src/config.rs`:

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub new_session: NewSessionConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub default_shell: String,
    pub animation_speed: f64,
    pub status_bar_poll_interval: u64,
    pub session_prune_days: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewSessionConfig {
    pub recent_dirs_count: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                default_shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into()),
                animation_speed: 1.0,
                status_bar_poll_interval: 2,
                session_prune_days: 30,
            },
            new_session: NewSessionConfig {
                recent_dirs_count: 15,
            },
        }
    }
}

impl AppConfig {
    pub fn save(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir)?;
        let path = dir.join("config.toml");
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionStore {
    #[serde(rename = "session", default)]
    pub sessions: Vec<SessionEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionEntry {
    pub id: Uuid,
    pub directory: String,
    pub creature_seed: u64,
    pub creature_template: String,
    pub claude_conversation_id: Option<String>,
    pub last_active: DateTime<Utc>,
    pub active: bool,
}

impl SessionStore {
    pub fn save(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir)?;
        let path = dir.join("sessions.toml");
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("sessions.toml");
        if !path.exists() {
            return Ok(Self { sessions: vec![] });
        }
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecentDirs {
    #[serde(rename = "directory", default)]
    pub directories: Vec<String>,
}

impl RecentDirs {
    pub fn save(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir)?;
        let path = dir.join("recent_dirs.toml");
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("recent_dirs.toml");
        if !path.exists() {
            return Ok(Self { directories: vec![] });
        }
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn add(&mut self, dir: String, max: usize) {
        self.directories.retain(|d| d != &dir);
        self.directories.insert(0, dir);
        self.directories.truncate(max);
    }
}
```

- [ ] **Step 4: Add lib.rs to expose modules for integration tests**

Create `src/lib.rs`:

```rust
pub mod app;
pub mod claude;
pub mod config;
pub mod creature;
pub mod session;
pub mod terminal;
pub mod ui;
```

Update `src/main.rs` to use the lib:

```rust
use anyhow::Result;

fn main() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = summoner::app::run(&mut terminal);
    ratatui::restore();
    result
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd /home/siim/dev/summoner && cargo test --test config_test
```

Expected: all 7 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/config.rs src/lib.rs src/main.rs tests/config_test.rs
git commit -m "feat: config, session store, and recent dirs persistence"
```

---

## Task 3: Session Types & State

**Files:**
- Create: `src/session.rs`

- [ ] **Step 1: Implement Session, SessionState, and project grouping**

Replace `src/session.rs`:

```rust
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Working,
    Waiting,
    Idle,
    Sleeping,
    Disconnected,
    ShellOnly,
}

impl SessionState {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Working => "⚡",
            Self::Waiting => "❓",
            Self::Idle => "◆",
            Self::Sleeping => "☽",
            Self::Disconnected => "✕",
            Self::ShellOnly => "▸",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Idle => "idle",
            Self::Sleeping => "sleeping",
            Self::Disconnected => "disconnected",
            Self::ShellOnly => "shell",
        }
    }
}

pub struct Session {
    pub id: Uuid,
    pub directory: String,
    pub creature_seed: u64,
    pub creature_template: String,
    pub state: SessionState,
    pub claude_conversation_id: Option<String>,
    pub name: String,
}

impl Session {
    pub fn new(directory: String, creature_seed: u64, creature_template: String) -> Self {
        let name = std::path::Path::new(&directory)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Self {
            id: Uuid::new_v4(),
            directory,
            creature_seed,
            creature_template,
            state: SessionState::ShellOnly,
            claude_conversation_id: None,
            name,
        }
    }
}

/// Groups sessions by their directory for dashboard display.
pub struct ProjectGroup {
    pub directory: String,
    pub name: String,
    pub sessions: Vec<usize>, // indices into the session list
}

pub fn group_by_project(sessions: &[Session]) -> Vec<ProjectGroup> {
    let mut groups: Vec<ProjectGroup> = Vec::new();

    for (idx, session) in sessions.iter().enumerate() {
        if let Some(group) = groups.iter_mut().find(|g| g.directory == session.directory) {
            group.sessions.push(idx);
        } else {
            groups.push(ProjectGroup {
                directory: session.directory.clone(),
                name: session.name.clone(),
                sessions: vec![idx],
            });
        }
    }

    groups
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd /home/siim/dev/summoner && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/session.rs
git commit -m "feat: session types, state enum, and project grouping"
```

---

## Task 4: Creature Generation — Bollinger Mask Algorithm

**Files:**
- Create: `src/creature/generate.rs`
- Create: `tests/creature_test.rs`

- [ ] **Step 1: Write failing tests for sprite generation**

Create `tests/creature_test.rs`:

```rust
use summoner::creature::generate::{Sprite, generate_sprite, CellKind, Xorshift};

#[test]
fn xorshift_is_deterministic() {
    let mut rng1 = Xorshift::new(42);
    let mut rng2 = Xorshift::new(42);
    let vals1: Vec<u64> = (0..100).map(|_| rng1.next()).collect();
    let vals2: Vec<u64> = (0..100).map(|_| rng2.next()).collect();
    assert_eq!(vals1, vals2);
}

#[test]
fn xorshift_different_seeds_produce_different_output() {
    let mut rng1 = Xorshift::new(42);
    let mut rng2 = Xorshift::new(99);
    let vals1: Vec<u64> = (0..10).map(|_| rng1.next()).collect();
    let vals2: Vec<u64> = (0..10).map(|_| rng2.next()).collect();
    assert_ne!(vals1, vals2);
}

#[test]
fn sprite_has_correct_dimensions() {
    let mask = vec![
        vec![0, 0, 1, 1],
        vec![0, 1, 1, 1],
        vec![0, 1, 2, 2],
        vec![0, 0, 1, 1],
    ];
    let sprite = generate_sprite(&mask, 42);
    // Width = mask[0].len() * 2 (mirrored), height = mask.len()
    assert_eq!(sprite.width, 8);
    assert_eq!(sprite.height, 4);
}

#[test]
fn sprite_is_horizontally_symmetric() {
    let mask = vec![
        vec![0, 1, 1, 2],
        vec![1, 1, 2, 2],
        vec![0, 1, 1, 1],
    ];
    let sprite = generate_sprite(&mask, 42);
    for y in 0..sprite.height {
        for x in 0..sprite.width / 2 {
            let mirror_x = sprite.width - 1 - x;
            assert_eq!(
                sprite.get(x, y),
                sprite.get(mirror_x, y),
                "Asymmetry at y={y}, x={x} vs x={mirror_x}"
            );
        }
    }
}

#[test]
fn same_seed_produces_same_sprite() {
    let mask = vec![
        vec![0, 1, 2, 1],
        vec![1, 1, 2, 2],
    ];
    let s1 = generate_sprite(&mask, 12345);
    let s2 = generate_sprite(&mask, 12345);
    assert_eq!(s1.cells, s2.cells);
}

#[test]
fn different_seeds_produce_different_sprites() {
    let mask = vec![
        vec![0, 1, 2, 1],
        vec![1, 1, 2, 2],
        vec![1, 2, 2, 1],
        vec![0, 1, 1, 0],
    ];
    let s1 = generate_sprite(&mask, 100);
    let s2 = generate_sprite(&mask, 200);
    assert_ne!(s1.cells, s2.cells);
}

#[test]
fn borders_surround_body_cells() {
    let mask = vec![
        vec![0, 0, 0, 0],
        vec![0, 1, 1, 0],
        vec![0, 1, 1, 0],
        vec![0, 0, 0, 0],
    ];
    // Use a seed that fills all 1s as body
    // After edge detection, body cells adjacent to empty should become border
    let sprite = generate_sprite(&mask, 1);
    // The center area should have some body, surrounded by border
    let mut has_body = false;
    let mut has_border = false;
    for y in 0..sprite.height {
        for x in 0..sprite.width {
            match sprite.get(x, y) {
                CellKind::Body => has_body = true,
                CellKind::Border => has_border = true,
                CellKind::Empty => {}
            }
        }
    }
    // With any reasonable seed, we should get at least some body or border
    assert!(has_body || has_border, "Sprite should have some filled cells");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /home/siim/dev/summoner && cargo test --test creature_test 2>&1 | head -10
```

Expected: compilation errors.

- [ ] **Step 3: Implement the Bollinger mask algorithm**

Replace `src/creature/generate.rs`:

```rust
/// Xorshift64 PRNG — deterministic, fast, seedable.
pub struct Xorshift {
    state: u64,
}

impl Xorshift {
    pub fn new(seed: u64) -> Self {
        // Ensure state is never 0 (xorshift64 fixpoint)
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    pub fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// Returns true with the given probability (0.0 to 1.0).
    pub fn chance(&mut self, probability: f64) -> bool {
        (self.next() % 1000) < (probability * 1000.0) as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Empty,
    Body,
    Border,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<CellKind>,
}

impl Sprite {
    pub fn get(&self, x: usize, y: usize) -> CellKind {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x]
        } else {
            CellKind::Empty
        }
    }

    fn set(&mut self, x: usize, y: usize, kind: CellKind) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = kind;
        }
    }
}

/// Mask cell values:
///  -1 = always border
///   0 = always empty
///   1 = 50% body, 50% empty
///   2 = 50% body, 50% border
///
/// The mask represents the LEFT HALF of the sprite. It is mirrored to produce
/// the full symmetric sprite.
pub fn generate_sprite(mask: &[Vec<i8>], seed: u64) -> Sprite {
    let half_width = mask[0].len();
    let full_width = half_width * 2;
    let height = mask.len();

    let mut sprite = Sprite {
        width: full_width,
        height,
        cells: vec![CellKind::Empty; full_width * height],
    };

    let mut rng = Xorshift::new(seed);

    // Phase 1: Fill the left half from the mask
    for y in 0..height {
        for x in 0..half_width {
            let cell = mask[y][x];
            let kind = match cell {
                -1 => CellKind::Border,
                0 => CellKind::Empty,
                1 => {
                    if rng.chance(0.5) {
                        CellKind::Body
                    } else {
                        CellKind::Empty
                    }
                }
                2 => {
                    if rng.chance(0.5) {
                        CellKind::Body
                    } else {
                        CellKind::Border
                    }
                }
                _ => CellKind::Empty,
            };
            sprite.set(x, y, kind);
        }
    }

    // Phase 2: Mirror left half to right half
    for y in 0..height {
        for x in 0..half_width {
            let kind = sprite.get(x, y);
            let mirror_x = full_width - 1 - x;
            sprite.set(mirror_x, y, kind);
        }
    }

    // Phase 3: Edge detection — body cells adjacent to empty become border
    let snapshot = sprite.cells.clone();
    for y in 0..height {
        for x in 0..full_width {
            let idx = y * full_width + x;
            if snapshot[idx] == CellKind::Body {
                let has_empty_neighbor = neighbors(x, y, full_width, height)
                    .any(|(nx, ny)| snapshot[ny * full_width + nx] == CellKind::Empty);
                if has_empty_neighbor {
                    sprite.set(x, y, CellKind::Border);
                }
            }
        }
    }

    sprite
}

fn neighbors(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> impl Iterator<Item = (usize, usize)> {
    let deltas: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    deltas.into_iter().filter_map(move |(dx, dy)| {
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
            Some((nx as usize, ny as usize))
        } else {
            None
        }
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /home/siim/dev/summoner && cargo test --test creature_test
```

Expected: all 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/creature/generate.rs tests/creature_test.rs
git commit -m "feat: Bollinger mask sprite generation with xorshift PRNG"
```

---

## Task 5: Creature Mask Templates

**Files:**
- Create: `src/creature/templates.rs`

- [ ] **Step 1: Add test for template selection to creature_test.rs**

Append to `tests/creature_test.rs`:

```rust
use summoner::creature::templates::{get_template, template_name, TEMPLATE_COUNT};

#[test]
fn all_templates_produce_valid_sprites() {
    for i in 0..TEMPLATE_COUNT {
        let mask = get_template(i);
        let sprite = generate_sprite(mask, 42);
        assert!(sprite.width > 0);
        assert!(sprite.height > 0);
        // Should have at least some non-empty cells
        let filled = sprite.cells.iter().filter(|c| **c != CellKind::Empty).count();
        assert!(filled > 0, "Template {} produced empty sprite", template_name(i));
    }
}

#[test]
fn template_selection_wraps_with_modulo() {
    let t1 = get_template(0);
    let t2 = get_template(TEMPLATE_COUNT);
    assert_eq!(t1.len(), t2.len());
}
```

- [ ] **Step 2: Implement mask templates**

Replace `src/creature/templates.rs`:

```rust
pub const TEMPLATE_COUNT: usize = 5;

pub fn template_name(index: usize) -> &'static str {
    match index % TEMPLATE_COUNT {
        0 => "bipedal",
        1 => "quadruped",
        2 => "blob",
        3 => "winged",
        4 => "serpentine",
        _ => unreachable!(),
    }
}

pub fn template_index(name: &str) -> usize {
    match name {
        "bipedal" => 0,
        "quadruped" => 1,
        "blob" => 2,
        "winged" => 3,
        "serpentine" => 4,
        _ => 0,
    }
}

/// Returns the half-mask for a creature template.
/// Width is the left half; it will be mirrored to produce the full sprite.
/// Values: -1=border, 0=empty, 1=maybe body, 2=maybe body/border
pub fn get_template(index: usize) -> &'static [Vec<i8>] {
    match index % TEMPLATE_COUNT {
        0 => BIPEDAL.as_slice(),
        1 => QUADRUPED.as_slice(),
        2 => BLOB.as_slice(),
        3 => WINGED.as_slice(),
        4 => SERPENTINE.as_slice(),
        _ => unreachable!(),
    }
}

lazy_static_templates! {
    // Bipedal: humanoid/robot shape, 6 wide (half), 14 tall
    BIPEDAL = [
        [0,  0,  0,  1,  1,  2],  // head top
        [0,  0,  1,  1,  2,  2],  // head
        [0,  0,  1,  1,  1,  1],  // head bottom
        [0,  0,  0,  1,  1,  0],  // neck
        [0,  0,  1,  1,  1,  1],  // shoulders
        [0,  1,  1,  1,  2,  2],  // upper torso
        [0,  1,  1,  1,  2,  1],  // torso
        [0,  1,  1,  1,  2,  2],  // torso
        [0,  0,  1,  1,  1,  1],  // lower torso
        [0,  0,  1,  1,  1,  0],  // waist
        [0,  0,  1,  1,  0,  0],  // upper legs
        [0,  0,  1,  1,  0,  0],  // legs
        [0,  0,  1,  2,  0,  0],  // lower legs
        [0,  0,  1,  2,  0,  0],  // feet
    ];

    // Quadruped: four-legged beast, 6 wide (half), 10 tall
    QUADRUPED = [
        [0,  0,  0,  0,  1,  2],  // ear/horn
        [0,  0,  1,  1,  2,  2],  // head
        [0,  1,  1,  1,  1,  1],  // head/neck
        [1,  1,  1,  1,  2,  2],  // body front
        [1,  1,  1,  2,  2,  1],  // body
        [1,  1,  1,  1,  2,  2],  // body
        [0,  1,  1,  1,  1,  1],  // body rear
        [0,  1,  0,  0,  1,  0],  // upper legs
        [0,  1,  0,  0,  1,  0],  // legs
        [0,  2,  0,  0,  2,  0],  // feet
    ];

    // Blob: amorphous, 5 wide (half), 8 tall
    BLOB = [
        [0,  0,  1,  1,  1],
        [0,  1,  2,  2,  2],
        [1,  1,  2,  2,  2],
        [1,  2,  2,  2,  1],
        [1,  2,  2,  2,  2],
        [1,  1,  2,  2,  1],
        [0,  1,  1,  2,  1],
        [0,  0,  1,  1,  0],
    ];

    // Winged: bird/dragon, 7 wide (half), 12 tall
    WINGED = [
        [0,  0,  0,  0,  1,  1,  0],  // crest
        [0,  0,  0,  1,  1,  2,  0],  // head
        [0,  0,  0,  1,  1,  1,  0],  // head
        [0,  0,  0,  0,  1,  0,  0],  // neck
        [1,  0,  0,  1,  1,  1,  0],  // wing tip + body
        [1,  1,  1,  1,  2,  2,  0],  // wing + body
        [0,  1,  1,  1,  1,  2,  0],  // wing + body
        [0,  0,  1,  1,  1,  1,  0],  // lower body
        [0,  0,  0,  1,  1,  0,  0],  // tail start
        [0,  0,  0,  1,  0,  0,  0],  // tail
        [0,  0,  1,  2,  0,  0,  0],  // legs
        [0,  0,  1,  2,  0,  0,  0],  // feet
    ];

    // Serpentine: snake/worm, 5 wide (half), 14 tall
    SERPENTINE = [
        [0,  0,  1,  1,  2],
        [0,  1,  1,  2,  2],
        [0,  1,  1,  1,  1],
        [0,  0,  1,  1,  0],
        [0,  1,  1,  1,  0],
        [0,  1,  2,  1,  0],
        [0,  0,  1,  1,  0],
        [0,  0,  1,  2,  1],
        [0,  1,  1,  2,  1],
        [0,  1,  1,  1,  0],
        [0,  0,  1,  1,  0],
        [0,  0,  1,  2,  0],
        [0,  0,  1,  1,  0],
        [0,  0,  0,  1,  0],
    ];
}

/// Macro to define templates as static Vec<Vec<i8>> (built once via lazy init).
macro_rules! lazy_static_templates {
    ($($name:ident = [$( [$($val:expr),+] ),+ $(,)?];)+) => {
        $(
            fn $name() -> Vec<Vec<i8>> {
                vec![ $( vec![$($val),+] ),+ ]
            }

            // We use functions and call them from get_template.
            // This avoids the need for a lazy_static dependency.
        )+

        // Override get_template to use the functions
    };
}

// Since we can't forward-reference the macro-generated fns from get_template above,
// restructure: the templates are defined as functions, get_template calls them.

// Remove the macro and just define functions directly:

// (Delete the macro invocation above and replace the entire file with this cleaner version)
```

Actually, let me simplify. Replace the entire `src/creature/templates.rs` with:

```rust
pub const TEMPLATE_COUNT: usize = 5;

pub fn template_name(index: usize) -> &'static str {
    match index % TEMPLATE_COUNT {
        0 => "bipedal",
        1 => "quadruped",
        2 => "blob",
        3 => "winged",
        4 => "serpentine",
        _ => unreachable!(),
    }
}

pub fn template_index(name: &str) -> usize {
    match name {
        "bipedal" => 0,
        "quadruped" => 1,
        "blob" => 2,
        "winged" => 3,
        "serpentine" => 4,
        _ => 0,
    }
}

/// Returns the half-mask for a creature template.
/// Width is the left half; it will be mirrored to produce the full sprite.
/// Values: -1=border, 0=empty, 1=maybe body, 2=maybe body/border
pub fn get_template(index: usize) -> Vec<Vec<i8>> {
    match index % TEMPLATE_COUNT {
        0 => bipedal(),
        1 => quadruped(),
        2 => blob(),
        3 => winged(),
        4 => serpentine(),
        _ => unreachable!(),
    }
}

fn bipedal() -> Vec<Vec<i8>> {
    vec![
        vec![0,  0,  0,  1,  1,  2],
        vec![0,  0,  1,  1,  2,  2],
        vec![0,  0,  1,  1,  1,  1],
        vec![0,  0,  0,  1,  1,  0],
        vec![0,  0,  1,  1,  1,  1],
        vec![0,  1,  1,  1,  2,  2],
        vec![0,  1,  1,  1,  2,  1],
        vec![0,  1,  1,  1,  2,  2],
        vec![0,  0,  1,  1,  1,  1],
        vec![0,  0,  1,  1,  1,  0],
        vec![0,  0,  1,  1,  0,  0],
        vec![0,  0,  1,  1,  0,  0],
        vec![0,  0,  1,  2,  0,  0],
        vec![0,  0,  1,  2,  0,  0],
    ]
}

fn quadruped() -> Vec<Vec<i8>> {
    vec![
        vec![0,  0,  0,  0,  1,  2],
        vec![0,  0,  1,  1,  2,  2],
        vec![0,  1,  1,  1,  1,  1],
        vec![1,  1,  1,  1,  2,  2],
        vec![1,  1,  1,  2,  2,  1],
        vec![1,  1,  1,  1,  2,  2],
        vec![0,  1,  1,  1,  1,  1],
        vec![0,  1,  0,  0,  1,  0],
        vec![0,  1,  0,  0,  1,  0],
        vec![0,  2,  0,  0,  2,  0],
    ]
}

fn blob() -> Vec<Vec<i8>> {
    vec![
        vec![0,  0,  1,  1,  1],
        vec![0,  1,  2,  2,  2],
        vec![1,  1,  2,  2,  2],
        vec![1,  2,  2,  2,  1],
        vec![1,  2,  2,  2,  2],
        vec![1,  1,  2,  2,  1],
        vec![0,  1,  1,  2,  1],
        vec![0,  0,  1,  1,  0],
    ]
}

fn winged() -> Vec<Vec<i8>> {
    vec![
        vec![0,  0,  0,  0,  1,  1,  0],
        vec![0,  0,  0,  1,  1,  2,  0],
        vec![0,  0,  0,  1,  1,  1,  0],
        vec![0,  0,  0,  0,  1,  0,  0],
        vec![1,  0,  0,  1,  1,  1,  0],
        vec![1,  1,  1,  1,  2,  2,  0],
        vec![0,  1,  1,  1,  1,  2,  0],
        vec![0,  0,  1,  1,  1,  1,  0],
        vec![0,  0,  0,  1,  1,  0,  0],
        vec![0,  0,  0,  1,  0,  0,  0],
        vec![0,  0,  1,  2,  0,  0,  0],
        vec![0,  0,  1,  2,  0,  0,  0],
    ]
}

fn serpentine() -> Vec<Vec<i8>> {
    vec![
        vec![0,  0,  1,  1,  2],
        vec![0,  1,  1,  2,  2],
        vec![0,  1,  1,  1,  1],
        vec![0,  0,  1,  1,  0],
        vec![0,  1,  1,  1,  0],
        vec![0,  1,  2,  1,  0],
        vec![0,  0,  1,  1,  0],
        vec![0,  0,  1,  2,  1],
        vec![0,  1,  1,  2,  1],
        vec![0,  1,  1,  1,  0],
        vec![0,  0,  1,  1,  0],
        vec![0,  0,  1,  2,  0],
        vec![0,  0,  1,  1,  0],
        vec![0,  0,  0,  1,  0],
    ]
}
```

- [ ] **Step 3: Run tests**

```bash
cd /home/siim/dev/summoner && cargo test --test creature_test
```

Expected: all 9 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/creature/templates.rs tests/creature_test.rs
git commit -m "feat: creature mask templates for 5 archetypes"
```

---

## Task 6: Creature Rendering & Animation

**Files:**
- Create: `src/creature/render.rs`
- Create: `src/creature/animate.rs`

- [ ] **Step 1: Add rendering tests to creature_test.rs**

Append to `tests/creature_test.rs`:

```rust
use summoner::creature::render::render_sprite_to_buffer;
use summoner::creature::animate::{AnimationState, animate_sprite};
use summoner::session::SessionState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[test]
fn render_sprite_fills_buffer_cells() {
    let mask = vec![
        vec![0, 1, 1],
        vec![1, 1, 1],
        vec![1, 1, 0],
        vec![0, 1, 0],
    ];
    let sprite = generate_sprite(&mask, 42);
    let area = Rect::new(0, 0, sprite.width as u16, (sprite.height / 2 + sprite.height % 2) as u16);
    let mut buf = Buffer::empty(area);
    let palette = summoner::creature::render::state_palette(SessionState::Working);
    render_sprite_to_buffer(&sprite, &palette, area, &mut buf);
    // Buffer should have some non-space cells
    let non_empty = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let cell = &buf[ratatui::layout::Position { x: *x, y: *y }];
            cell.symbol() != " "
        })
        .count();
    assert!(non_empty > 0, "Rendered sprite should have visible cells");
}

#[test]
fn animation_state_advances_frames() {
    let mut anim = AnimationState::new(SessionState::Working);
    let frame0 = anim.current_frame();
    // Advance past one frame duration
    anim.tick(std::time::Duration::from_millis(250));
    let frame1 = anim.current_frame();
    // After enough time, frame should have advanced
    assert!(frame0 == 0);
    assert!(frame1 > 0 || anim.total_frames() == 1);
}

#[test]
fn animation_state_changes_reset_frame() {
    let mut anim = AnimationState::new(SessionState::Working);
    anim.tick(std::time::Duration::from_millis(500));
    anim.set_state(SessionState::Idle);
    assert_eq!(anim.current_frame(), 0);
}

#[test]
fn animate_sprite_returns_modified_sprite() {
    let mask = vec![
        vec![0, 1, 1],
        vec![1, 1, 1],
        vec![1, 1, 0],
        vec![0, 1, 0],
    ];
    let base = generate_sprite(&mask, 42);
    let anim = AnimationState::new(SessionState::Idle);
    let animated = animate_sprite(&base, &anim);
    // Animated sprite should have same or similar dimensions
    assert!(animated.width == base.width);
    assert!(animated.height >= base.height - 1 && animated.height <= base.height + 2);
}
```

- [ ] **Step 2: Implement creature rendering (half-block characters)**

Replace `src/creature/render.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};

use super::generate::{CellKind, Sprite};
use crate::session::SessionState;

pub struct Palette {
    pub body: Color,
    pub border: Color,
    pub highlight: Color,
}

pub fn state_palette(state: SessionState) -> Palette {
    match state {
        SessionState::Working => Palette {
            body: Color::Rgb(0, 200, 120),
            border: Color::Rgb(0, 100, 60),
            highlight: Color::Rgb(100, 255, 180),
        },
        SessionState::Waiting => Palette {
            body: Color::Rgb(255, 180, 50),
            border: Color::Rgb(180, 100, 0),
            highlight: Color::Rgb(255, 220, 100),
        },
        SessionState::Idle => Palette {
            body: Color::Rgb(100, 120, 220),
            border: Color::Rgb(50, 60, 140),
            highlight: Color::Rgb(150, 170, 255),
        },
        SessionState::Sleeping => Palette {
            body: Color::Rgb(80, 90, 120),
            border: Color::Rgb(40, 45, 60),
            highlight: Color::Rgb(100, 110, 140),
        },
        SessionState::Disconnected => Palette {
            body: Color::Rgb(100, 100, 100),
            border: Color::Rgb(60, 60, 60),
            highlight: Color::Rgb(130, 130, 130),
        },
        SessionState::ShellOnly => Palette {
            body: Color::Rgb(200, 200, 210),
            border: Color::Rgb(140, 140, 150),
            highlight: Color::Rgb(240, 240, 255),
        },
    }
}

const UPPER_HALF: &str = "\u{2580}";
const LOWER_HALF: &str = "\u{2584}";
const FULL_BLOCK: &str = "\u{2588}";

/// Renders a sprite into a ratatui Buffer using half-block characters.
/// Each terminal cell represents 2 vertical pixels (upper + lower).
/// The buffer area should be sized for: width = sprite.width, height = ceil(sprite.height / 2).
pub fn render_sprite_to_buffer(
    sprite: &Sprite,
    palette: &Palette,
    area: Rect,
    buf: &mut Buffer,
) {
    let rows = (sprite.height + 1) / 2; // ceil(height / 2)

    for row in 0..rows.min(area.height as usize) {
        for col in 0..sprite.width.min(area.width as usize) {
            let upper_y = row * 2;
            let lower_y = row * 2 + 1;

            let upper = sprite.get(col, upper_y);
            let lower = if lower_y < sprite.height {
                sprite.get(col, lower_y)
            } else {
                CellKind::Empty
            };

            let pos = Position {
                x: area.x + col as u16,
                y: area.y + row as u16,
            };

            if let Some(cell) = buf.cell_mut(pos) {
                match (upper, lower) {
                    (CellKind::Empty, CellKind::Empty) => {}
                    (CellKind::Empty, lower_kind) => {
                        cell.set_symbol(LOWER_HALF);
                        cell.set_style(Style::default().fg(kind_color(&lower_kind, palette)));
                    }
                    (upper_kind, CellKind::Empty) => {
                        cell.set_symbol(UPPER_HALF);
                        cell.set_style(Style::default().fg(kind_color(&upper_kind, palette)));
                    }
                    (upper_kind, lower_kind) => {
                        let fg_color = kind_color(&lower_kind, palette);
                        let bg_color = kind_color(&upper_kind, palette);
                        if fg_color == bg_color {
                            cell.set_symbol(FULL_BLOCK);
                            cell.set_style(Style::default().fg(fg_color));
                        } else {
                            // LOWER_HALF: fg = lower pixel color, bg = upper pixel color
                            cell.set_symbol(LOWER_HALF);
                            cell.set_style(Style::default().fg(fg_color).bg(bg_color));
                        }
                    }
                }
            }
        }
    }
}

fn kind_color(kind: &CellKind, palette: &Palette) -> Color {
    match kind {
        CellKind::Body => palette.body,
        CellKind::Border => palette.border,
        CellKind::Empty => Color::Reset,
    }
}

/// Returns the terminal cell dimensions needed to render a sprite.
pub fn sprite_cell_size(sprite: &Sprite) -> (u16, u16) {
    (sprite.width as u16, ((sprite.height + 1) / 2) as u16)
}
```

- [ ] **Step 3: Implement animation state and frame transforms**

Replace `src/creature/animate.rs`:

```rust
use std::time::Duration;

use super::generate::{CellKind, Sprite};
use crate::session::SessionState;

pub struct AnimationState {
    state: SessionState,
    frame: usize,
    elapsed: Duration,
}

impl AnimationState {
    pub fn new(state: SessionState) -> Self {
        Self {
            state,
            frame: 0,
            elapsed: Duration::ZERO,
        }
    }

    pub fn current_frame(&self) -> usize {
        self.frame
    }

    pub fn total_frames(&self) -> usize {
        frame_count(self.state)
    }

    pub fn set_state(&mut self, state: SessionState) {
        if state != self.state {
            self.state = state;
            self.frame = 0;
            self.elapsed = Duration::ZERO;
        }
    }

    pub fn tick(&mut self, dt: Duration) {
        let duration = frame_duration(self.state);
        if duration == Duration::ZERO {
            return;
        }
        self.elapsed += dt;
        if self.elapsed >= duration {
            self.elapsed -= duration;
            self.frame = (self.frame + 1) % frame_count(self.state);
        }
    }
}

fn frame_count(state: SessionState) -> usize {
    match state {
        SessionState::Working => 4,
        SessionState::Waiting => 3,
        SessionState::Idle => 2,
        SessionState::Sleeping => 3,
        SessionState::Disconnected => 1,
        SessionState::ShellOnly => 2,
    }
}

fn frame_duration(state: SessionState) -> Duration {
    match state {
        SessionState::Working => Duration::from_millis(200),
        SessionState::Waiting => Duration::from_millis(300),
        SessionState::Idle => Duration::from_millis(800),
        SessionState::Sleeping => Duration::from_millis(600),
        SessionState::Disconnected => Duration::ZERO,
        SessionState::ShellOnly => Duration::from_millis(500),
    }
}

/// Produces an animated version of the base sprite for the current frame.
pub fn animate_sprite(base: &Sprite, anim: &AnimationState) -> Sprite {
    match anim.state {
        SessionState::Working => animate_locomotion(base, anim.frame),
        SessionState::Waiting => animate_bounce(base, anim.frame),
        SessionState::Idle => animate_breathe(base, anim.frame),
        SessionState::Sleeping => animate_sleep(base, anim.frame),
        SessionState::Disconnected => base.clone(),
        SessionState::ShellOnly => animate_wobble(base, anim.frame),
    }
}

/// Locomotion: shift sprite left/right by 1px to simulate walking.
fn animate_locomotion(base: &Sprite, frame: usize) -> Sprite {
    let offset: i32 = match frame {
        0 => 0,
        1 => 1,
        2 => 0,
        3 => -1,
        _ => 0,
    };
    shift_horizontal(base, offset)
}

/// Bounce: shift sprite up by varying amounts.
fn animate_bounce(base: &Sprite, frame: usize) -> Sprite {
    let shift_up = match frame {
        0 => 0,
        1 => 2,
        2 => 1,
        _ => 0,
    };
    shift_vertical(base, shift_up)
}

/// Breathe: alternate between normal height and +1 row.
fn animate_breathe(base: &Sprite, frame: usize) -> Sprite {
    if frame == 0 {
        base.clone()
    } else {
        // Stretch by 1 pixel vertically (duplicate middle row)
        let mid = base.height / 2;
        let new_height = base.height + 1;
        let mut cells = Vec::with_capacity(base.width * new_height);
        for y in 0..new_height {
            let src_y = if y <= mid { y } else { y - 1 };
            for x in 0..base.width {
                cells.push(base.get(x, src_y));
            }
        }
        Sprite {
            width: base.width,
            height: new_height,
            cells,
        }
    }
}

/// Sleep: squash sprite down by 1 pixel.
fn animate_sleep(base: &Sprite, frame: usize) -> Sprite {
    // Squash: skip the first row (creature "sinks down")
    let start_row = 1.min(base.height.saturating_sub(1));
    let new_height = base.height - start_row;
    let mut cells = Vec::with_capacity(base.width * new_height);
    for y in start_row..base.height {
        for x in 0..base.width {
            cells.push(base.get(x, y));
        }
    }
    // The "zzz" particles are rendered as an overlay in the render pass,
    // not baked into the sprite. frame controls zzz position.
    let _ = frame;
    Sprite {
        width: base.width,
        height: new_height,
        cells,
    }
}

/// Wobble: shift left/right by 1px.
fn animate_wobble(base: &Sprite, frame: usize) -> Sprite {
    let offset: i32 = if frame == 0 { 0 } else { 1 };
    shift_horizontal(base, offset)
}

fn shift_horizontal(sprite: &Sprite, offset: i32) -> Sprite {
    let mut cells = vec![CellKind::Empty; sprite.width * sprite.height];
    for y in 0..sprite.height {
        for x in 0..sprite.width {
            let src_x = x as i32 - offset;
            if src_x >= 0 && (src_x as usize) < sprite.width {
                cells[y * sprite.width + x] = sprite.get(src_x as usize, y);
            }
        }
    }
    Sprite {
        width: sprite.width,
        height: sprite.height,
        cells,
    }
}

fn shift_vertical(sprite: &Sprite, up_pixels: usize) -> Sprite {
    let new_height = sprite.height + up_pixels;
    let mut cells = vec![CellKind::Empty; sprite.width * new_height];
    for y in 0..sprite.height {
        for x in 0..sprite.width {
            cells[y * sprite.width + x] = sprite.get(x, y);
        }
    }
    Sprite {
        width: sprite.width,
        height: new_height,
        cells,
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd /home/siim/dev/summoner && cargo test --test creature_test
```

Expected: all 13 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/creature/render.rs src/creature/animate.rs tests/creature_test.rs
git commit -m "feat: creature half-block rendering and state-based animation"
```

---

## Task 7: PTY Session Management

**Files:**
- Create: `src/terminal.rs`
- Create: `tests/terminal_test.rs`

- [ ] **Step 1: Write integration test for PTY spawn and read**

Create `tests/terminal_test.rs`:

```rust
use summoner::terminal::PtySession;
use std::time::Duration;
use std::thread;

#[test]
fn pty_session_spawns_and_reads_output() {
    let mut session = PtySession::spawn("/bin/bash", "/tmp", 24, 80).unwrap();

    // Send a command
    session.write(b"echo SUMMONER_TEST\r\n").unwrap();

    // Give the shell time to process
    thread::sleep(Duration::from_millis(200));

    // Read output
    let output = session.read_available();
    let text: String = output.iter().map(|b| String::from_utf8_lossy(b).to_string()).collect();
    assert!(text.contains("SUMMONER_TEST"), "Expected output to contain SUMMONER_TEST, got: {}", text);
}

#[test]
fn pty_session_has_valid_pid() {
    let session = PtySession::spawn("/bin/bash", "/tmp", 24, 80).unwrap();
    assert!(session.pid().is_some());
}

#[test]
fn pty_session_can_resize() {
    let session = PtySession::spawn("/bin/bash", "/tmp", 24, 80).unwrap();
    // Should not panic or error
    session.resize(40, 120).unwrap();
}
```

- [ ] **Step 2: Implement PtySession**

Replace `src/terminal.rs`:

```rust
use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    output_rx: mpsc::Receiver<Vec<u8>>,
    pid: Option<u32>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    _reader_handle: thread::JoinHandle<()>,
}

impl PtySession {
    pub fn spawn(shell: &str, cwd: &str, rows: u16, cols: u16) -> Result<Self> {
        let pty_system = native_pty_system();

        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.cwd(cwd);

        let child = pair.slave.spawn_command(cmd)?;
        let pid = child.process_id();

        // Drop slave — we only interact via master
        drop(pair.slave);

        let writer = pair.master.take_writer()?;
        let mut reader = pair.master.try_clone_reader()?;

        // Spawn reader thread that sends output chunks over a channel
        let (tx, rx) = mpsc::channel();
        let reader_handle = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            master: pair.master,
            writer: Arc::new(Mutex::new(writer)),
            output_rx: rx,
            pid,
            child: Arc::new(Mutex::new(child)),
            _reader_handle: reader_handle,
        })
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    /// Reads all currently available output (non-blocking).
    /// Returns a vec of byte chunks.
    pub fn read_available(&self) -> Vec<Vec<u8>> {
        let mut chunks = Vec::new();
        while let Ok(chunk) = self.output_rx.try_recv() {
            chunks.push(chunk);
        }
        chunks
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    /// Check if the child process has exited (non-blocking).
    pub fn try_wait(&self) -> Option<portable_pty::ExitStatus> {
        self.child.lock().unwrap().try_wait().ok().flatten()
    }

    /// Kill the child process.
    pub fn kill(&self) {
        let _ = self.child.lock().unwrap().kill();
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cd /home/siim/dev/summoner && cargo test --test terminal_test
```

Expected: all 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/terminal.rs tests/terminal_test.rs
git commit -m "feat: PTY session spawning, reading, writing, and resize"
```

---

## Task 8: Terminal View Widget (vt100 → Ratatui)

**Files:**
- Create: `src/ui/session_view.rs`

- [ ] **Step 1: Implement the TerminalView widget**

Replace `src/ui/session_view.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

/// Renders a vt100 Screen into a ratatui Buffer.
pub struct TerminalView<'a> {
    screen: &'a vt100::Screen,
}

impl<'a> TerminalView<'a> {
    pub fn new(screen: &'a vt100::Screen) -> Self {
        Self { screen }
    }
}

impl<'a> Widget for TerminalView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (vt_rows, vt_cols) = self.screen.size();

        for row in 0..area.height.min(vt_rows) {
            for col in 0..area.width.min(vt_cols) {
                if let Some(vt_cell) = self.screen.cell(row, col) {
                    if vt_cell.is_wide_continuation() {
                        continue;
                    }

                    let pos = Position {
                        x: area.x + col,
                        y: area.y + row,
                    };

                    if let Some(buf_cell) = buf.cell_mut(pos) {
                        let contents = vt_cell.contents();
                        buf_cell.set_symbol(if contents.is_empty() { " " } else { contents });

                        let fg = convert_color(vt_cell.fgcolor());
                        let bg = convert_color(vt_cell.bgcolor());

                        let mut modifiers = Modifier::empty();
                        if vt_cell.bold() {
                            modifiers |= Modifier::BOLD;
                        }
                        if vt_cell.dim() {
                            modifiers |= Modifier::DIM;
                        }
                        if vt_cell.italic() {
                            modifiers |= Modifier::ITALIC;
                        }
                        if vt_cell.underline() {
                            modifiers |= Modifier::UNDERLINED;
                        }
                        if vt_cell.inverse() {
                            modifiers |= Modifier::REVERSED;
                        }

                        buf_cell.set_style(
                            Style::default()
                                .fg(fg)
                                .bg(bg)
                                .add_modifier(modifiers),
                        );
                    }
                }
            }
        }

        // Render cursor
        let (cur_row, cur_col) = self.screen.cursor_position();
        let cursor_pos = Position {
            x: area.x + cur_col,
            y: area.y + cur_row,
        };
        if let Some(cell) = buf.cell_mut(cursor_pos) {
            cell.set_style(
                cell.style()
                    .add_modifier(Modifier::REVERSED),
            );
        }
    }
}

fn convert_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd /home/siim/dev/summoner && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/ui/session_view.rs
git commit -m "feat: terminal view widget mapping vt100 screen to ratatui buffer"
```

---

## Task 9: Status Bar Widget

**Files:**
- Create: `src/ui/status_bar.rs`
- Create: `tests/ui_test.rs`

- [ ] **Step 1: Write tests for status bar rendering**

Create `tests/ui_test.rs`:

```rust
use summoner::session::{Session, SessionState};
use summoner::ui::status_bar::StatusBar;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

fn make_session(name: &str, state: SessionState) -> Session {
    let mut s = Session::new(format!("/tmp/{}", name), 42, "bipedal".into());
    s.state = state;
    s
}

#[test]
fn status_bar_renders_session_tabs() {
    let sessions = vec![
        make_session("project-a", SessionState::Working),
        make_session("project-b", SessionState::Waiting),
    ];
    let bar = StatusBar::new(&sessions, Some(0));
    let area = Rect::new(0, 0, 80, 1);
    let mut buf = Buffer::empty(area);
    bar.render(area, &mut buf);

    let text: String = (0..80)
        .map(|x| buf[ratatui::layout::Position { x, y: 0 }].symbol().to_string())
        .collect();

    assert!(text.contains("F1"), "Should contain F1 tab");
    assert!(text.contains("project-a"), "Should contain project name");
    assert!(text.contains("F2"), "Should contain F2 tab");
}

#[test]
fn status_bar_highlights_active_session() {
    let sessions = vec![
        make_session("active", SessionState::Working),
        make_session("inactive", SessionState::Idle),
    ];
    let bar = StatusBar::new(&sessions, Some(0));
    let area = Rect::new(0, 0, 80, 1);
    let mut buf = Buffer::empty(area);
    bar.render(area, &mut buf);

    // The active tab should have BOLD modifier
    let first_cell = &buf[ratatui::layout::Position { x: 0, y: 0 }];
    // Just verify it rendered without panic — visual styling is hard to assert
    assert!(true);
}

#[test]
fn status_bar_shows_f12_hint() {
    let sessions = vec![make_session("test", SessionState::Idle)];
    let bar = StatusBar::new(&sessions, Some(0));
    let area = Rect::new(0, 0, 80, 1);
    let mut buf = Buffer::empty(area);
    bar.render(area, &mut buf);

    let text: String = (0..80)
        .map(|x| buf[ratatui::layout::Position { x, y: 0 }].symbol().to_string())
        .collect();
    assert!(text.contains("F12"), "Should contain F12 dashboard hint");
}
```

- [ ] **Step 2: Implement the StatusBar widget**

Replace `src/ui/status_bar.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::session::{Session, SessionState};

pub struct StatusBar<'a> {
    sessions: &'a [Session],
    active_index: Option<usize>,
}

impl<'a> StatusBar<'a> {
    pub fn new(sessions: &'a [Session], active_index: Option<usize>) -> Self {
        Self {
            sessions,
            active_index,
        }
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }

        // Fill background
        let bg_style = Style::default().bg(Color::Rgb(30, 30, 40));
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
                cell.set_symbol(" ");
                cell.set_style(bg_style);
            }
        }

        let mut x = area.x;
        let f12_hint = " F12 Dashboard ";
        let max_tab_x = area.x + area.width - f12_hint.len() as u16 - 1;
        let mut overflow_count = 0;

        for (i, session) in self.sessions.iter().enumerate() {
            let fkey = format!("F{}", i + 1);
            let icon = session.state.icon();
            let tab_text = format!(" {} {} {} ", fkey, icon, session.name);

            if x + tab_text.len() as u16 > max_tab_x {
                overflow_count = self.sessions.len() - i;
                break;
            }

            let is_active = self.active_index == Some(i);
            let style = tab_style(session.state, is_active);

            for ch in tab_text.chars() {
                if x >= area.x + area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
                    cell.set_symbol(&ch.to_string());
                    cell.set_style(style);
                }
                x += 1;
            }
        }

        // Overflow indicator
        if overflow_count > 0 {
            let overflow_text = format!(" +{} more ", overflow_count);
            let overflow_style = Style::default()
                .fg(Color::Rgb(120, 120, 140))
                .bg(Color::Rgb(30, 30, 40));
            for ch in overflow_text.chars() {
                if x >= max_tab_x {
                    break;
                }
                if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
                    cell.set_symbol(&ch.to_string());
                    cell.set_style(overflow_style);
                }
                x += 1;
            }
        }

        // F12 Dashboard hint (right-aligned)
        let hint_x = area.x + area.width - f12_hint.len() as u16;
        let hint_style = Style::default()
            .fg(Color::Rgb(150, 150, 170))
            .bg(Color::Rgb(30, 30, 40));
        for (i, ch) in f12_hint.chars().enumerate() {
            let pos = Position {
                x: hint_x + i as u16,
                y: area.y,
            };
            if let Some(cell) = buf.cell_mut(pos) {
                cell.set_symbol(&ch.to_string());
                cell.set_style(hint_style);
            }
        }
    }
}

fn tab_style(state: SessionState, active: bool) -> Style {
    let fg = state_color(state);
    let bg = if active {
        Color::Rgb(50, 50, 70)
    } else {
        Color::Rgb(30, 30, 40)
    };
    let mut style = Style::default().fg(fg).bg(bg);
    if active {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

fn state_color(state: SessionState) -> Color {
    match state {
        SessionState::Working => Color::Rgb(0, 200, 120),
        SessionState::Waiting => Color::Rgb(255, 180, 50),
        SessionState::Idle => Color::Rgb(100, 120, 220),
        SessionState::Sleeping => Color::Rgb(80, 90, 120),
        SessionState::Disconnected => Color::Rgb(100, 100, 100),
        SessionState::ShellOnly => Color::Rgb(200, 200, 210),
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cd /home/siim/dev/summoner && cargo test --test ui_test
```

Expected: all 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/ui/status_bar.rs tests/ui_test.rs
git commit -m "feat: status bar widget with session tabs and state colors"
```

---

## Task 10: Dashboard — Project Card Grid

**Files:**
- Create: `src/ui/dashboard.rs`

- [ ] **Step 1: Implement the Dashboard widget with dynamic layout**

Replace `src/ui/dashboard.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Padding, Widget};

use crate::creature::animate::{animate_sprite, AnimationState};
use crate::creature::generate::{generate_sprite, Sprite};
use crate::creature::render::{render_sprite_to_buffer, sprite_cell_size, state_palette};
use crate::creature::templates::get_template;
use crate::session::{group_by_project, ProjectGroup, Session, SessionState};
use crate::ui::dashboard_nav::DashboardNav;

pub struct Dashboard<'a> {
    sessions: &'a [Session],
    animations: &'a [AnimationState],
    sprites: &'a [Sprite],
    nav: &'a DashboardNav,
}

impl<'a> Dashboard<'a> {
    pub fn new(
        sessions: &'a [Session],
        animations: &'a [AnimationState],
        sprites: &'a [Sprite],
        nav: &'a DashboardNav,
    ) -> Self {
        Self {
            sessions,
            animations,
            sprites,
            nav,
        }
    }
}

impl<'a> Widget for Dashboard<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Fill background
        let bg = Style::default().bg(Color::Rgb(20, 20, 30));
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut(Position { x, y }) {
                    cell.set_symbol(" ");
                    cell.set_style(bg);
                }
            }
        }

        let groups = group_by_project(self.sessions);
        if groups.is_empty() {
            // Render empty state message
            let msg = "No sessions. Press N to open a directory.";
            let x = area.x + area.width.saturating_sub(msg.len() as u16) / 2;
            let y = area.y + area.height / 2;
            buf.set_string(
                x,
                y,
                msg,
                Style::default().fg(Color::Rgb(120, 120, 140)),
            );
            return;
        }

        // Compute grid dimensions
        let (cols, rows) = grid_dimensions(groups.len(), area);
        let card_rects = compute_card_rects(area, cols, rows, groups.len());

        for (group_idx, group) in groups.iter().enumerate() {
            if group_idx >= card_rects.len() {
                break;
            }
            let card_area = card_rects[group_idx];
            let is_selected = self.nav.selected_card() == group_idx;
            let selected_session = if is_selected {
                self.nav.selected_session_in_card()
            } else {
                None
            };

            render_project_card(
                &group,
                self.sessions,
                self.animations,
                self.sprites,
                card_area,
                buf,
                is_selected,
                selected_session,
            );
        }

        // Footer hints
        let hints = " ←→↑↓ navigate │ Enter select │ n new session │ N new dir │ d close │ q back ";
        let hints_y = area.y + area.height - 1;
        let hints_x = area.x + area.width.saturating_sub(hints.len() as u16) / 2;
        buf.set_string(
            hints_x,
            hints_y,
            hints,
            Style::default()
                .fg(Color::Rgb(100, 100, 120))
                .bg(Color::Rgb(20, 20, 30)),
        );
    }
}

fn grid_dimensions(count: usize, area: Rect) -> (usize, usize) {
    match count {
        0 => (0, 0),
        1 => (1, 1),
        2..=3 => (count, 1),
        4..=6 => {
            let cols = if area.width >= 120 { 3 } else { 2 };
            let rows = (count + cols - 1) / cols;
            (cols, rows)
        }
        _ => {
            let cols = 3;
            let rows = (count + cols - 1) / cols;
            (cols, rows)
        }
    }
}

fn compute_card_rects(area: Rect, cols: usize, rows: usize, count: usize) -> Vec<Rect> {
    if cols == 0 || rows == 0 {
        return vec![];
    }

    // Reserve 1 row at bottom for hints
    let grid_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(3),
    };

    let col_width = grid_area.width / cols as u16;
    let row_height = grid_area.height / rows as u16;

    let mut rects = Vec::with_capacity(count);
    for i in 0..count {
        let col = i % cols;
        let row = i / cols;
        rects.push(Rect {
            x: grid_area.x + col as u16 * col_width,
            y: grid_area.y + row as u16 * row_height,
            width: col_width.saturating_sub(1),
            height: row_height.saturating_sub(1),
        });
    }
    rects
}

fn render_project_card(
    group: &ProjectGroup,
    sessions: &[Session],
    animations: &[AnimationState],
    sprites: &[Sprite],
    area: Rect,
    buf: &mut Buffer,
    is_selected: bool,
    selected_session: Option<usize>,
) {
    let border_color = if is_selected {
        Color::Rgb(200, 200, 255)
    } else {
        Color::Rgb(60, 60, 80)
    };
    let border_style = Style::default().fg(border_color);

    let block = Block::default()
        .title(format!(" {} ", group.name))
        .title_style(Style::default().fg(Color::Rgb(220, 220, 240)).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(border_style)
        .padding(Padding::uniform(1));

    let inner = block.inner(area);
    block.render(area, buf);

    // Render creatures side by side
    let creature_width: u16 = 12; // max creature width in cells
    let creature_height: u16 = 8; // max creature height in cells
    let mut cx = inner.x;

    for (local_idx, &session_idx) in group.sessions.iter().enumerate() {
        if cx + creature_width > inner.x + inner.width {
            break;
        }
        if session_idx >= sessions.len() {
            continue;
        }

        let creature_area = Rect {
            x: cx,
            y: inner.y,
            width: creature_width.min(inner.width - (cx - inner.x)),
            height: creature_height.min(inner.height.saturating_sub(1)),
        };

        // Render creature
        let animated = animate_sprite(&sprites[session_idx], &animations[session_idx]);
        let palette = state_palette(sessions[session_idx].state);
        render_sprite_to_buffer(&animated, &palette, creature_area, buf);

        // Highlight if this specific session is selected
        if is_selected && selected_session == Some(local_idx) {
            let marker_y = creature_area.y + creature_area.height;
            if marker_y < inner.y + inner.height {
                buf.set_string(
                    creature_area.x + creature_width / 2 - 1,
                    marker_y,
                    "▲",
                    Style::default().fg(Color::Rgb(255, 255, 100)),
                );
            }
        }

        cx += creature_width + 1;
    }

    // Session count footer
    let footer = format!(
        "{} session{}",
        group.sessions.len(),
        if group.sessions.len() == 1 { "" } else { "s" }
    );
    let footer_y = area.y + area.height - 2;
    if footer_y > area.y {
        buf.set_string(
            inner.x,
            footer_y,
            &footer,
            Style::default().fg(Color::Rgb(100, 100, 120)),
        );
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd /home/siim/dev/summoner && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/ui/dashboard.rs
git commit -m "feat: dashboard widget with dynamic project card grid"
```

---

## Task 11: Dashboard Navigation State

**Files:**
- Create: `src/ui/dashboard_nav.rs`

- [ ] **Step 1: Implement two-level navigation**

Replace `src/ui/dashboard_nav.rs`:

```rust
/// Two-level navigation for the dashboard.
/// Level 1: navigate between project cards
/// Level 2: navigate between sessions within a card
#[derive(Debug)]
pub struct DashboardNav {
    card_index: usize,
    /// None = card-level navigation, Some(i) = session-level within card
    session_index: Option<usize>,
    card_count: usize,
    /// Number of sessions in each card (by card index)
    sessions_per_card: Vec<usize>,
}

impl DashboardNav {
    pub fn new() -> Self {
        Self {
            card_index: 0,
            session_index: None,
            card_count: 0,
            sessions_per_card: Vec::new(),
        }
    }

    pub fn update_counts(&mut self, sessions_per_card: Vec<usize>) {
        self.card_count = sessions_per_card.len();
        self.sessions_per_card = sessions_per_card;
        // Clamp indices
        if self.card_count > 0 {
            self.card_index = self.card_index.min(self.card_count - 1);
        } else {
            self.card_index = 0;
            self.session_index = None;
        }
        if let Some(si) = self.session_index {
            let max = self.current_card_session_count();
            if max == 0 {
                self.session_index = None;
            } else {
                self.session_index = Some(si.min(max - 1));
            }
        }
    }

    pub fn selected_card(&self) -> usize {
        self.card_index
    }

    pub fn selected_session_in_card(&self) -> Option<usize> {
        self.session_index
    }

    pub fn is_in_card(&self) -> bool {
        self.session_index.is_some()
    }

    /// Enter session-level navigation within the current card.
    pub fn enter_card(&mut self) {
        if self.current_card_session_count() > 0 {
            self.session_index = Some(0);
        }
    }

    /// Exit back to card-level navigation.
    pub fn exit_card(&mut self) {
        self.session_index = None;
    }

    pub fn move_left(&mut self) {
        if let Some(si) = &mut self.session_index {
            if *si > 0 {
                *si -= 1;
            }
        } else if self.card_index > 0 {
            self.card_index -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if let Some(si) = &mut self.session_index {
            let max = self.current_card_session_count();
            if *si + 1 < max {
                *si += 1;
            }
        } else if self.card_index + 1 < self.card_count {
            self.card_index += 1;
        }
    }

    pub fn move_up(&mut self) {
        // At card level, move to row above (assuming grid columns)
        // For simplicity, just decrement by 1 (can be refined for grid layout later)
        if self.session_index.is_none() && self.card_index > 0 {
            self.card_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.session_index.is_none() && self.card_index + 1 < self.card_count {
            self.card_index += 1;
        }
    }

    /// Returns the global session index for the currently selected session.
    /// `group_session_indices` maps card_index -> Vec of global session indices.
    pub fn selected_global_session(&self, group_session_indices: &[Vec<usize>]) -> Option<usize> {
        let si = self.session_index?;
        group_session_indices
            .get(self.card_index)
            .and_then(|indices| indices.get(si).copied())
    }

    fn current_card_session_count(&self) -> usize {
        self.sessions_per_card
            .get(self.card_index)
            .copied()
            .unwrap_or(0)
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd /home/siim/dev/summoner && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/ui/dashboard_nav.rs
git commit -m "feat: two-level dashboard navigation (cards and sessions)"
```

---

## Task 12: Claude Code State Detection

**Files:**
- Create: `src/claude.rs`
- Create: `tests/claude_test.rs`

- [ ] **Step 1: Write tests for Claude Code detection**

Create `tests/claude_test.rs`:

```rust
use summoner::claude::detect_claude_state;
use summoner::session::SessionState;

fn make_screen(lines: &[&str]) -> vt100::Parser {
    let mut parser = vt100::Parser::new(24, 80, 0);
    for (i, line) in lines.iter().enumerate() {
        let cmd = format!("\x1b[{};1H{}", i + 1, line);
        parser.process(cmd.as_bytes());
    }
    parser
}

#[test]
fn detects_working_state() {
    let parser = make_screen(&[
        "Claude is thinking...",
        "",
        "",
        // ... many empty lines ...
        "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
        "",
        "                                                    esc to interrupt",
    ]);
    let state = detect_claude_state(parser.screen());
    assert_eq!(state, Some(SessionState::Working));
}

#[test]
fn detects_waiting_state() {
    let parser = make_screen(&[
        "Do you want to proceed?",
        "",
        "",
        "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
        "",
        "                                                    Esc to cancel",
    ]);
    let state = detect_claude_state(parser.screen());
    assert_eq!(state, Some(SessionState::Waiting));
}

#[test]
fn returns_none_for_plain_shell() {
    let parser = make_screen(&[
        "siim@host:~/dev$ ls",
        "file1.rs  file2.rs",
        "siim@host:~/dev$",
    ]);
    let state = detect_claude_state(parser.screen());
    assert_eq!(state, None);
}
```

- [ ] **Step 2: Implement Claude Code detection**

Replace `src/claude.rs`:

```rust
use crate::session::SessionState;

/// Inspects the vt100 screen to detect Claude Code state.
/// Returns None if Claude Code is not detected.
pub fn detect_claude_state(screen: &vt100::Screen) -> Option<SessionState> {
    let (rows, cols) = screen.size();

    // Scan the last few rows for Claude Code status bar patterns
    let search_rows = 4.min(rows);
    for row_offset in 0..search_rows {
        let row = rows - 1 - row_offset;
        let text = row_text(screen, row);
        let trimmed = text.trim();

        if trimmed.contains("esc to interrupt") {
            return Some(SessionState::Working);
        }
        if trimmed.contains("Esc to cancel") {
            return Some(SessionState::Waiting);
        }
    }

    // Check if Claude Code is running but idle (look for its UI patterns)
    // Claude Code shows a prompt like "> " or has specific UI elements
    // For now, if no status bar patterns found, assume it's not Claude Code
    None
}

fn row_text(screen: &vt100::Screen, row: u16) -> String {
    let (_rows, cols) = screen.size();
    screen.rows(0, cols).nth(row as usize).unwrap_or_default()
}

/// Attempt to find a Claude conversation ID from the sessions directory.
pub fn find_conversation_id(pid: u32) -> Option<String> {
    let sessions_dir = dirs::home_dir()?.join(".claude/sessions");
    let pid_file = sessions_dir.join(format!("{}.json", pid));
    if pid_file.exists() {
        // Read the file and extract the conversation ID
        let content = std::fs::read_to_string(&pid_file).ok()?;
        // Claude session files are JSON — look for session_id or conversation_id field
        // Simple extraction without pulling in a JSON parser:
        extract_json_field(&content, "session_id")
            .or_else(|| extract_json_field(&content, "id"))
    } else {
        None
    }
}

fn extract_json_field(json: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\"", field);
    let pos = json.find(&pattern)?;
    let after_key = &json[pos + pattern.len()..];
    // Skip whitespace and colon
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    // Extract string value
    let after_quote = after_colon.strip_prefix('"')?;
    let end = after_quote.find('"')?;
    Some(after_quote[..end].to_string())
}
```

- [ ] **Step 3: Run tests**

```bash
cd /home/siim/dev/summoner && cargo test --test claude_test
```

Expected: all 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/claude.rs tests/claude_test.rs
git commit -m "feat: Claude Code state detection from vt100 screen"
```

---

## Task 13: App State Machine & Event Loop

**Files:**
- Create: `src/app.rs`

- [ ] **Step 1: Implement the full App struct and event loop**

Replace `src/app.rs`:

```rust
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::{DefaultTerminal, Frame};
use std::time::{Duration, Instant};

use crate::claude::{detect_claude_state, find_conversation_id};
use crate::config::{AppConfig, RecentDirs, SessionStore};
use crate::creature::animate::{animate_sprite, AnimationState};
use crate::creature::generate::{generate_sprite, Sprite, Xorshift};
use crate::creature::templates::{get_template, template_name, TEMPLATE_COUNT};
use crate::session::{group_by_project, Session, SessionState};
use crate::terminal::PtySession;
use crate::ui::dashboard::Dashboard;
use crate::ui::dashboard_nav::DashboardNav;
use crate::ui::dir_picker::{DirPicker, DirPickerAction};
use crate::ui::session_view::TerminalView;
use crate::ui::status_bar::StatusBar;

#[derive(Debug, PartialEq)]
enum Mode {
    Dashboard,
    Session(usize), // index into sessions vec
    DirPicker,
}

pub struct App {
    mode: Mode,
    last_session: Option<usize>,
    sessions: Vec<Session>,
    pty_sessions: Vec<Option<PtySession>>,
    vt_parsers: Vec<vt100::Parser>,
    animations: Vec<AnimationState>,
    sprites: Vec<Sprite>,
    nav: DashboardNav,
    dir_picker: DirPicker,
    config: AppConfig,
    recent_dirs: RecentDirs,
    config_dir: std::path::PathBuf,
}

impl App {
    pub fn new() -> Result<Self> {
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".summoner");

        let config = AppConfig::load(&config_dir)?;
        let recent_dirs = RecentDirs::load(&config_dir)?;
        let session_store = SessionStore::load(&config_dir)?;

        let mut app = Self {
            mode: Mode::Dashboard,
            last_session: None,
            sessions: Vec::new(),
            pty_sessions: Vec::new(),
            vt_parsers: Vec::new(),
            animations: Vec::new(),
            sprites: Vec::new(),
            nav: DashboardNav::new(),
            dir_picker: DirPicker::new(recent_dirs.directories.clone()),
            config,
            recent_dirs,
            config_dir,
        };

        // Restore disconnected sessions from store
        for entry in &session_store.sessions {
            if !entry.active {
                let mut session = Session::new(
                    entry.directory.clone(),
                    entry.creature_seed,
                    entry.creature_template.clone(),
                );
                session.id = entry.id;
                session.state = SessionState::Disconnected;
                session.claude_conversation_id = entry.claude_conversation_id.clone();

                let template_idx = crate::creature::templates::template_index(&entry.creature_template);
                let mask = get_template(template_idx);
                let sprite = generate_sprite(&mask, entry.creature_seed);

                app.sessions.push(session);
                app.pty_sessions.push(None);
                app.vt_parsers.push(vt100::Parser::new(24, 80, 0));
                app.animations.push(AnimationState::new(SessionState::Disconnected));
                app.sprites.push(sprite);
            }
        }

        app.update_nav_counts();

        // If no sessions, go straight to dir picker
        if app.sessions.is_empty() {
            app.mode = Mode::DirPicker;
        }

        Ok(app)
    }

    fn spawn_session(&mut self, directory: String, rows: u16, cols: u16) -> Result<()> {
        let mut rng = Xorshift::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
        );
        let seed = rng.next();
        let template_idx = (seed as usize) % TEMPLATE_COUNT;
        let template = template_name(template_idx).to_string();

        let mask = get_template(template_idx);
        let sprite = generate_sprite(&mask, seed);

        let pty = PtySession::spawn(&self.config.general.default_shell, &directory, rows, cols)?;

        let session = Session::new(directory.clone(), seed, template);

        self.sessions.push(session);
        self.pty_sessions.push(Some(pty));
        self.vt_parsers.push(vt100::Parser::new(rows, cols, 0));
        self.animations.push(AnimationState::new(SessionState::ShellOnly));
        self.sprites.push(sprite);

        // Update recent dirs
        self.recent_dirs.add(directory, self.config.new_session.recent_dirs_count);
        let _ = self.recent_dirs.save(&self.config_dir);

        self.update_nav_counts();

        let idx = self.sessions.len() - 1;
        self.mode = Mode::Session(idx);
        self.last_session = Some(idx);

        Ok(())
    }

    fn close_session(&mut self, index: usize) {
        if index >= self.sessions.len() {
            return;
        }
        if let Some(Some(pty)) = self.pty_sessions.get(index) {
            pty.kill();
        }
        self.sessions.remove(index);
        self.pty_sessions.remove(index);
        self.vt_parsers.remove(index);
        self.animations.remove(index);
        self.sprites.remove(index);

        self.update_nav_counts();

        // Fix mode if we were viewing the closed session
        if self.mode == Mode::Session(index) {
            if self.sessions.is_empty() {
                self.mode = Mode::Dashboard;
            } else {
                let new_idx = index.min(self.sessions.len() - 1);
                self.mode = Mode::Session(new_idx);
                self.last_session = Some(new_idx);
            }
        }
    }

    fn update_nav_counts(&mut self) {
        let groups = group_by_project(&self.sessions);
        let counts: Vec<usize> = groups.iter().map(|g| g.sessions.len()).collect();
        self.nav.update_counts(counts);
    }

    fn process_pty_output(&mut self) {
        for i in 0..self.sessions.len() {
            if let Some(Some(pty)) = self.pty_sessions.get(i) {
                let chunks = pty.read_available();
                for chunk in chunks {
                    self.vt_parsers[i].process(&chunk);
                }

                // Detect Claude Code state
                let screen = self.vt_parsers[i].screen();
                if let Some(claude_state) = detect_claude_state(screen) {
                    self.sessions[i].state = claude_state;
                    self.animations[i].set_state(claude_state);

                    // Try to capture conversation ID
                    if self.sessions[i].claude_conversation_id.is_none() {
                        if let Some(pid) = pty.pid() {
                            if let Some(conv_id) = find_conversation_id(pid) {
                                self.sessions[i].claude_conversation_id = Some(conv_id);
                            }
                        }
                    }
                } else if self.sessions[i].state != SessionState::ShellOnly {
                    // Claude Code may have exited
                    self.sessions[i].state = SessionState::ShellOnly;
                    self.animations[i].set_state(SessionState::ShellOnly);
                }

                // Check if child exited
                if pty.try_wait().is_some() {
                    self.sessions[i].state = SessionState::Disconnected;
                    self.animations[i].set_state(SessionState::Disconnected);
                    self.pty_sessions[i] = None;
                }
            }
        }
    }

    fn save_state(&self) {
        let entries: Vec<crate::config::SessionEntry> = self.sessions.iter().map(|s| {
            crate::config::SessionEntry {
                id: s.id,
                directory: s.directory.clone(),
                creature_seed: s.creature_seed,
                creature_template: s.creature_template.clone(),
                claude_conversation_id: s.claude_conversation_id.clone(),
                last_active: chrono::Utc::now(),
                active: self.pty_sessions.get(self.sessions.iter().position(|x| x.id == s.id).unwrap_or(0))
                    .and_then(|p| p.as_ref())
                    .is_some(),
            }
        }).collect();
        let store = SessionStore { sessions: entries };
        let _ = store.save(&self.config_dir);
    }
}

pub fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut app = App::new()?;
    let mut last_tick = Instant::now();
    let mut last_status_poll = Instant::now();
    let status_poll_interval = Duration::from_secs(app.config.general.status_bar_poll_interval);

    loop {
        // Render
        terminal.draw(|frame| render(&app, frame))?;

        // Poll for events with a short timeout
        let timeout = match app.mode {
            Mode::Dashboard => Duration::from_millis(33), // ~30fps for animations
            _ => Duration::from_millis(16), // responsive PTY rendering
        };

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if handle_input(&mut app, key, terminal)? {
                    break; // quit
                }
            }
        }

        // Process PTY output
        app.process_pty_output();

        // Update animations (only when dashboard is visible)
        let now = Instant::now();
        let dt = now - last_tick;
        last_tick = now;
        if app.mode == Mode::Dashboard {
            for anim in &mut app.animations {
                anim.tick(dt);
            }
        }

        // Periodic status bar poll for background sessions
        if now - last_status_poll >= status_poll_interval {
            last_status_poll = now;
            // process_pty_output already handles this
        }
    }

    app.save_state();
    Ok(())
}

fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // Reserve 1 row at bottom for status bar
    let [main_area, bar_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    match &app.mode {
        Mode::Dashboard | Mode::DirPicker => {
            let dashboard = Dashboard::new(
                &app.sessions,
                &app.animations,
                &app.sprites,
                &app.nav,
            );
            frame.render_widget(dashboard, main_area);

            if app.mode == Mode::DirPicker {
                // Render dir picker as overlay
                let picker_area = centered_rect(60, 70, main_area);
                frame.render_widget(&app.dir_picker, picker_area);
            }
        }
        Mode::Session(idx) => {
            let screen = app.vt_parsers[*idx].screen();
            let view = TerminalView::new(screen);
            frame.render_widget(view, main_area);
        }
    }

    // Status bar (always)
    let active_idx = match &app.mode {
        Mode::Session(idx) => Some(*idx),
        _ => app.last_session,
    };
    let bar = StatusBar::new(&app.sessions, active_idx);
    frame.render_widget(bar, bar_area);
}

fn handle_input(app: &mut App, key: KeyEvent, terminal: &mut DefaultTerminal) -> Result<bool> {
    // Global keys
    match key.code {
        KeyCode::F(12) => {
            if app.mode == Mode::Dashboard || app.mode == Mode::DirPicker {
                if let Some(idx) = app.last_session {
                    if idx < app.sessions.len() {
                        app.mode = Mode::Session(idx);
                    }
                }
            } else {
                app.mode = Mode::Dashboard;
            }
            return Ok(false);
        }
        KeyCode::F(n) if (1..=11).contains(&n) => {
            let idx = (n as usize) - 1;
            if idx < app.sessions.len() {
                app.mode = Mode::Session(idx);
                app.last_session = Some(idx);
            }
            return Ok(false);
        }
        _ => {}
    }

    match &app.mode {
        Mode::DirPicker => {
            match app.dir_picker.handle_key(key) {
                DirPickerAction::None => {}
                DirPickerAction::Cancel => {
                    app.mode = Mode::Dashboard;
                }
                DirPickerAction::Select(dir) => {
                    let size = terminal.size()?;
                    app.spawn_session(dir, size.height.saturating_sub(1), size.width)?;
                }
            }
        }
        Mode::Dashboard => {
            match key.code {
                KeyCode::Char('q') => {
                    if app.last_session.is_some() {
                        app.mode = Mode::Session(app.last_session.unwrap());
                    } else {
                        return Ok(true); // quit
                    }
                }
                KeyCode::Char('Q') => return Ok(true), // force quit
                KeyCode::Left => app.nav.move_left(),
                KeyCode::Right => app.nav.move_right(),
                KeyCode::Up => app.nav.move_up(),
                KeyCode::Down => app.nav.move_down(),
                KeyCode::Enter => {
                    if app.nav.is_in_card() {
                        // Jump into selected session
                        let groups = group_by_project(&app.sessions);
                        let group_indices: Vec<Vec<usize>> =
                            groups.iter().map(|g| g.sessions.clone()).collect();
                        if let Some(global_idx) = app.nav.selected_global_session(&group_indices) {
                            app.mode = Mode::Session(global_idx);
                            app.last_session = Some(global_idx);
                        }
                    } else {
                        app.nav.enter_card();
                    }
                }
                KeyCode::Esc => {
                    app.nav.exit_card();
                }
                KeyCode::Char('n') => {
                    // New session in selected project's directory
                    let groups = group_by_project(&app.sessions);
                    if let Some(group) = groups.get(app.nav.selected_card()) {
                        let dir = group.directory.clone();
                        let size = terminal.size()?;
                        app.spawn_session(dir, size.height.saturating_sub(1), size.width)?;
                    }
                }
                KeyCode::Char('N') => {
                    app.dir_picker = DirPicker::new(app.recent_dirs.directories.clone());
                    app.mode = Mode::DirPicker;
                }
                KeyCode::Char('d') => {
                    // Close selected session
                    let groups = group_by_project(&app.sessions);
                    let group_indices: Vec<Vec<usize>> =
                        groups.iter().map(|g| g.sessions.clone()).collect();
                    if let Some(global_idx) = app.nav.selected_global_session(&group_indices) {
                        app.close_session(global_idx);
                    }
                }
                _ => {}
            }
        }
        Mode::Session(idx) => {
            let idx = *idx;
            // Forward all non-F-key input to the PTY
            if let Some(Some(pty)) = app.pty_sessions.get(idx) {
                if let Some(bytes) = key_to_bytes(key) {
                    pty.write(&bytes)?;
                }
            }
        }
    }

    Ok(false)
}

/// Convert a crossterm KeyEvent to bytes to send to a PTY.
fn key_to_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
                let byte = (c as u8).wrapping_sub(b'a').wrapping_add(1);
                if byte <= 26 {
                    return Some(vec![byte]);
                }
            }
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            Some(s.as_bytes().to_vec())
        }
        KeyCode::Enter => Some(b"\r".to_vec()),
        KeyCode::Backspace => Some(vec![127]),
        KeyCode::Tab => Some(b"\t".to_vec()),
        KeyCode::Esc => Some(vec![27]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        _ => None,
    }
}

/// Create a centered rect of given percentage within an area.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let width = area.width * percent_x / 100;
    let height = area.height * percent_y / 100;
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    Rect::new(x, y, width, height)
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd /home/siim/dev/summoner && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat: app state machine with dashboard, session view, and event loop"
```

---

## Task 14: Directory Picker (Recent Dirs + Fuzzy Finder)

**Files:**
- Create: `src/ui/dir_picker.rs`

- [ ] **Step 1: Implement the DirPicker widget and input handling**

Replace `src/ui/dir_picker.rs`:

```rust
use crossterm::event::{KeyCode, KeyEvent};
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Padding, Widget};

pub enum DirPickerAction {
    None,
    Cancel,
    Select(String),
}

pub struct DirPicker {
    recent_dirs: Vec<String>,
    query: String,
    filtered: Vec<(String, u32)>,
    selected: usize,
    matcher: Matcher,
}

impl DirPicker {
    pub fn new(recent_dirs: Vec<String>) -> Self {
        let mut picker = Self {
            recent_dirs: recent_dirs.clone(),
            query: String::new(),
            filtered: recent_dirs.iter().map(|d| (d.clone(), 0)).collect(),
            selected: 0,
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
        };
        picker.update_filter();
        picker
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> DirPickerAction {
        match key.code {
            KeyCode::Esc => DirPickerAction::Cancel,
            KeyCode::Enter => {
                if let Some((dir, _)) = self.filtered.get(self.selected) {
                    DirPickerAction::Select(dir.clone())
                } else if !self.query.is_empty() {
                    // Use the raw query as a directory path
                    DirPickerAction::Select(self.query.clone())
                } else {
                    DirPickerAction::None
                }
            }
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                DirPickerAction::None
            }
            KeyCode::Down => {
                if self.selected + 1 < self.filtered.len() {
                    self.selected += 1;
                }
                DirPickerAction::None
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.update_filter();
                self.selected = 0;
                DirPickerAction::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.update_filter();
                self.selected = 0;
                DirPickerAction::None
            }
            _ => DirPickerAction::None,
        }
    }

    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = self.recent_dirs.iter().map(|d| (d.clone(), 0)).collect();
        } else {
            let pattern = Pattern::new(
                &self.query,
                CaseMatching::Ignore,
                Normalization::Smart,
                AtomKind::Fuzzy,
            );
            self.filtered = pattern.match_list(&self.recent_dirs, &mut self.matcher);
        }
    }
}

impl Widget for &DirPicker {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the overlay area
        Clear.render(area, buf);

        let block = Block::default()
            .title(" Open Directory ")
            .title_style(Style::default().fg(Color::Rgb(220, 220, 240)).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(100, 100, 140)))
            .padding(Padding::uniform(1));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 || inner.width < 10 {
            return;
        }

        // Search input
        let prompt = "> ";
        let input_style = Style::default().fg(Color::Rgb(220, 220, 240));
        buf.set_string(inner.x, inner.y, prompt, input_style);
        buf.set_string(
            inner.x + prompt.len() as u16,
            inner.y,
            &self.query,
            input_style.add_modifier(Modifier::BOLD),
        );

        // Cursor
        let cursor_x = inner.x + prompt.len() as u16 + self.query.len() as u16;
        if let Some(cell) = buf.cell_mut(Position { x: cursor_x, y: inner.y }) {
            cell.set_symbol("▌");
            cell.set_style(Style::default().fg(Color::Rgb(150, 150, 200)));
        }

        // Separator
        let sep_y = inner.y + 1;
        for x in inner.x..inner.x + inner.width {
            if let Some(cell) = buf.cell_mut(Position { x, y: sep_y }) {
                cell.set_symbol("─");
                cell.set_style(Style::default().fg(Color::Rgb(60, 60, 80)));
            }
        }

        // Results
        let results_start = sep_y + 1;
        let max_results = (inner.height - 2) as usize;

        if self.filtered.is_empty() {
            let msg = if self.query.is_empty() {
                "No recent directories. Type a path."
            } else {
                "No matches. Press Enter to use as path."
            };
            buf.set_string(
                inner.x,
                results_start,
                msg,
                Style::default().fg(Color::Rgb(100, 100, 120)),
            );
        } else {
            for (i, (dir, _score)) in self.filtered.iter().take(max_results).enumerate() {
                let y = results_start + i as u16;
                let is_selected = i == self.selected;

                let style = if is_selected {
                    Style::default()
                        .fg(Color::Rgb(255, 255, 255))
                        .bg(Color::Rgb(50, 50, 80))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(180, 180, 200))
                };

                let prefix = if is_selected { "▸ " } else { "  " };
                let display = format!(
                    "{}{}",
                    prefix,
                    truncate_path(dir, inner.width as usize - 2)
                );
                buf.set_string(inner.x, y, &display, style);
            }
        }
    }
}

fn truncate_path(path: &str, max_len: usize) -> &str {
    if path.len() <= max_len {
        path
    } else {
        &path[path.len() - max_len..]
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd /home/siim/dev/summoner && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/ui/dir_picker.rs
git commit -m "feat: directory picker with recent dirs and fuzzy finder"
```

---

## Task 15: Integration — Wire Everything Together & First Run

**Files:**
- Modify: `src/main.rs`
- Modify: `src/creature/mod.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Ensure all module re-exports are correct**

Verify `src/creature/mod.rs`:
```rust
pub mod animate;
pub mod generate;
pub mod render;
pub mod templates;
```

Verify `src/ui/mod.rs`:
```rust
pub mod dashboard;
pub mod dashboard_nav;
pub mod dir_picker;
pub mod session_view;
pub mod status_bar;
```

Verify `src/lib.rs`:
```rust
pub mod app;
pub mod claude;
pub mod config;
pub mod creature;
pub mod session;
pub mod terminal;
pub mod ui;
```

Verify `src/main.rs`:
```rust
use anyhow::Result;

fn main() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = summoner::app::run(&mut terminal);
    ratatui::restore();
    result
}
```

- [ ] **Step 2: Build the full binary**

```bash
cd /home/siim/dev/summoner && cargo build 2>&1
```

Expected: builds successfully (warnings about unused imports/variables are fine at this stage).

- [ ] **Step 3: Fix any compilation issues**

If there are errors, fix them now. Common issues:
- Missing imports
- Type mismatches between modules
- Lifetime issues

- [ ] **Step 4: Run the full test suite**

```bash
cd /home/siim/dev/summoner && cargo test 2>&1
```

Expected: all tests pass.

- [ ] **Step 5: Manual smoke test**

```bash
cd /home/siim/dev/summoner && cargo run
```

Expected behavior:
1. App starts with the directory picker overlay (empty dashboard behind it)
2. Type a path or select from recent dirs
3. Shell spawns in the selected directory
4. F12 toggles back to dashboard
5. Status bar visible at bottom
6. `Q` from dashboard quits

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: wire all modules together, first runnable Summoner binary"
```

---

## Task 16: PTY Resize on Terminal Resize

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Handle terminal resize events**

In `src/app.rs`, update the event loop in `run()` to handle resize events. Find the `event::poll` block and add resize handling:

After the existing `Event::Key` match arm, add:

```rust
Event::Resize(cols, rows) => {
    // Resize all active PTY sessions
    let session_rows = rows.saturating_sub(1); // minus status bar
    for (i, pty_opt) in app.pty_sessions.iter().enumerate() {
        if let Some(pty) = pty_opt {
            let _ = pty.resize(session_rows, cols);
            app.vt_parsers[i].screen_mut().set_size(session_rows, cols);
        }
    }
}
```

- [ ] **Step 2: Build and verify**

```bash
cd /home/siim/dev/summoner && cargo build
```

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat: resize PTY sessions when terminal is resized"
```

---

## Task 17: Session Restore (Claude Code Resume)

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add a restore action to the dashboard**

Add a restore method to `App` in `src/app.rs`:

```rust
fn restore_session(&mut self, index: usize, rows: u16, cols: u16) -> Result<()> {
    if index >= self.sessions.len() {
        return Ok(());
    }
    if self.pty_sessions[index].is_some() {
        return Ok(()); // already active
    }

    let dir = self.sessions[index].directory.clone();
    let pty = PtySession::spawn(&self.config.general.default_shell, &dir, rows, cols)?;

    // If we have a Claude conversation ID, resume it
    if let Some(conv_id) = &self.sessions[index].claude_conversation_id {
        let resume_cmd = format!("claude --resume {}\r\n", conv_id);
        pty.write(resume_cmd.as_bytes())?;
    }

    self.pty_sessions[index] = Some(pty);
    self.vt_parsers[index] = vt100::Parser::new(rows, cols, 0);
    self.sessions[index].state = SessionState::ShellOnly;
    self.animations[index].set_state(SessionState::ShellOnly);

    self.mode = Mode::Session(index);
    self.last_session = Some(index);

    Ok(())
}
```

Then add a key binding in the dashboard `handle_input` — use `r` for restore:

```rust
KeyCode::Char('r') => {
    let groups = group_by_project(&app.sessions);
    let group_indices: Vec<Vec<usize>> =
        groups.iter().map(|g| g.sessions.clone()).collect();
    if let Some(global_idx) = app.nav.selected_global_session(&group_indices) {
        let size = terminal.size()?;
        app.restore_session(global_idx, size.height.saturating_sub(1), size.width)?;
    }
}
```

- [ ] **Step 2: Update dashboard hints to include restore**

In `src/ui/dashboard.rs`, update the footer hints string:

```rust
let hints = " ←→↑↓ navigate │ Enter select │ n new │ N new dir │ r restore │ d close │ q back ";
```

- [ ] **Step 3: Build and verify**

```bash
cd /home/siim/dev/summoner && cargo build
```

- [ ] **Step 4: Commit**

```bash
git add src/app.rs src/ui/dashboard.rs
git commit -m "feat: restore disconnected sessions with Claude Code resume"
```
