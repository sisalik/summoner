# Dashboard Scroll Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scroll the dashboard viewport when project-card rows overflow the available height, with partially-clipped "peek" cards above/below signalling more content, and mouse-wheel + keyboard-driven scroll.

**Architecture:** `DashboardNav` gains scroll state (`scroll_row`, cached `visible_full_rows`). `Dashboard::render` computes a responsive peek budget from the leftover vertical space, then renders the scroll-adjacent rows clipped to `content_area`. All drawing helpers (`draw_text`, `draw_selection_box`, sprite renderers) take a clip rect so peek cards never overwrite the usage bar or hint row. Mouse wheel calls `scroll_up`/`scroll_down`; keyboard navigation calls `ensure_selection_visible`.

**Tech Stack:** Rust, Ratatui, crossterm, existing `vt100`/`portable-pty` stack.

Design spec: `docs/superpowers/specs/2026-04-15-dashboard-scroll-design.md`.

---

### Task 1: Scroll state on DashboardNav

**Files:**
- Modify: `src/ui/dashboard_nav.rs`
- Test: `src/ui/dashboard_nav.rs` (in-file `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing tests**

Append at the bottom of `src/ui/dashboard_nav.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn nav_with(rows: Vec<Vec<usize>>, sizes: Vec<usize>) -> DashboardNav {
        let mut nav = DashboardNav::new();
        nav.update_layout_with_rows(&sizes, rows);
        nav
    }

    #[test]
    fn scroll_up_down_clamps_to_max() {
        // 4 rows of 1 group each, 2 visible -> max_scroll_row = 2
        let mut nav = nav_with(
            vec![vec![0], vec![1], vec![2], vec![3]],
            vec![1, 1, 1, 1],
        );
        nav.set_visible_full_rows(2);

        assert_eq!(nav.scroll_row(), 0);
        nav.scroll_up();
        assert_eq!(nav.scroll_row(), 0, "cannot scroll above 0");

        nav.scroll_down();
        assert_eq!(nav.scroll_row(), 1);
        nav.scroll_down();
        assert_eq!(nav.scroll_row(), 2);
        nav.scroll_down();
        assert_eq!(nav.scroll_row(), 2, "cannot scroll past max");
    }

    #[test]
    fn ensure_selection_visible_pulls_viewport_down() {
        let mut nav = nav_with(
            vec![vec![0], vec![1], vec![2], vec![3]],
            vec![1, 1, 1, 1],
        );
        nav.set_visible_full_rows(2);

        // Select row 3, expect scroll_row -> 2 so selection is last visible
        nav.set_selected(3);
        nav.ensure_selection_visible();
        assert_eq!(nav.scroll_row(), 2);
    }

    #[test]
    fn ensure_selection_visible_pulls_viewport_up() {
        let mut nav = nav_with(
            vec![vec![0], vec![1], vec![2], vec![3]],
            vec![1, 1, 1, 1],
        );
        nav.set_visible_full_rows(2);
        nav.set_selected(3);
        nav.ensure_selection_visible();
        assert_eq!(nav.scroll_row(), 2);

        nav.set_selected(0);
        nav.ensure_selection_visible();
        assert_eq!(nav.scroll_row(), 0, "selection above band -> scroll_row becomes selection row");
    }

    #[test]
    fn ensure_selection_visible_noop_when_in_band() {
        let mut nav = nav_with(
            vec![vec![0], vec![1], vec![2], vec![3]],
            vec![1, 1, 1, 1],
        );
        nav.set_visible_full_rows(2);
        nav.scroll_down();
        assert_eq!(nav.scroll_row(), 1);

        nav.set_selected(2); // row 2 is inside [1, 2]
        nav.ensure_selection_visible();
        assert_eq!(nav.scroll_row(), 1, "no change when selection already visible");
    }

    #[test]
    fn layout_update_clamps_scroll_row() {
        let mut nav = nav_with(
            vec![vec![0], vec![1], vec![2], vec![3]],
            vec![1, 1, 1, 1],
        );
        nav.set_visible_full_rows(2);
        nav.scroll_down();
        nav.scroll_down();
        assert_eq!(nav.scroll_row(), 2);

        // Layout shrinks to 2 rows total
        nav.update_layout_with_rows(&[1, 1], vec![vec![0], vec![1]]);
        assert!(nav.scroll_row() <= nav.max_scroll_row(),
                "scroll_row must be clamped on layout change");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib dashboard_nav::tests`
Expected: compile errors or FAIL — `scroll_row`, `scroll_up`, `scroll_down`, `ensure_selection_visible`, `set_visible_full_rows`, `max_scroll_row` undefined.

- [ ] **Step 3: Implement scroll state + methods**

Replace the contents of `src/ui/dashboard_nav.rs` (keeping the existing module structure intact) with:

```rust
// Navigation state — flat navigation across all sessions, with row-aware up/down

#[derive(Debug)]
pub struct DashboardNav {
    selected: usize,
    total: usize,
    /// group_ranges[i] = (start_session_idx, count)
    group_ranges: Vec<(usize, usize)>,
    /// row_groups[row] = [group_idx, ...] — which groups are on each visual row
    row_groups: Vec<Vec<usize>>,
    /// Index of the topmost row rendered in the full band.
    scroll_row: usize,
    /// Cached count of fully-rendered rows from the most recent render.
    /// Input handlers use this to drive `ensure_selection_visible`.
    visible_full_rows: usize,
}

impl Default for DashboardNav {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardNav {
    pub fn new() -> Self {
        Self {
            selected: 0,
            total: 0,
            group_ranges: Vec::new(),
            row_groups: Vec::new(),
            scroll_row: 0,
            visible_full_rows: 0,
        }
    }

    pub fn update_layout_with_rows(&mut self, group_sizes: &[usize], row_groups: Vec<Vec<usize>>) {
        self.row_groups = row_groups;
        self.total = group_sizes.iter().sum();
        let mut start = 0;
        self.group_ranges = group_sizes
            .iter()
            .map(|&size| {
                let range = (start, size);
                start += size;
                range
            })
            .collect();
        if self.total > 0 {
            self.selected = self.selected.min(self.total - 1);
        } else {
            self.selected = 0;
        }
        // Clamp scroll_row against new layout.
        let max = self.max_scroll_row();
        if self.scroll_row > max {
            self.scroll_row = max;
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, pos: usize) {
        if self.total > 0 {
            self.selected = pos.min(self.total - 1);
        }
    }

    /// Convert flat position to actual session index using the given order mapping.
    pub fn selected_session(&self, session_order: &[usize]) -> Option<usize> {
        session_order.get(self.selected).copied()
    }

    pub fn row_groups(&self) -> &[Vec<usize>] {
        &self.row_groups
    }

    pub fn scroll_row(&self) -> usize {
        self.scroll_row
    }

    pub fn visible_full_rows(&self) -> usize {
        self.visible_full_rows
    }

    pub fn set_visible_full_rows(&mut self, n: usize) {
        self.visible_full_rows = n;
        let max = self.max_scroll_row();
        if self.scroll_row > max {
            self.scroll_row = max;
        }
    }

    pub fn max_scroll_row(&self) -> usize {
        self.row_groups.len().saturating_sub(self.visible_full_rows)
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_row > 0 {
            self.scroll_row -= 1;
        }
    }

    pub fn scroll_down(&mut self) {
        let max = self.max_scroll_row();
        if self.scroll_row < max {
            self.scroll_row += 1;
        }
    }

    /// Move viewport so the row containing the selection sits inside
    /// `[scroll_row, scroll_row + visible_full_rows - 1]`.
    pub fn ensure_selection_visible(&mut self) {
        if self.visible_full_rows == 0 { return; }
        let Some(g) = self.current_group() else { return };
        let Some(row) = self.group_row(g) else { return };

        if row < self.scroll_row {
            self.scroll_row = row;
        } else if row >= self.scroll_row + self.visible_full_rows {
            self.scroll_row = row + 1 - self.visible_full_rows;
        }
        let max = self.max_scroll_row();
        if self.scroll_row > max {
            self.scroll_row = max;
        }
    }

    fn current_group(&self) -> Option<usize> {
        self.group_ranges
            .iter()
            .position(|&(start, size)| self.selected >= start && self.selected < start + size)
    }

    fn position_in_group(&self) -> usize {
        if let Some(g) = self.current_group() {
            self.selected - self.group_ranges[g].0
        } else {
            0
        }
    }

    fn group_row(&self, group_idx: usize) -> Option<usize> {
        self.row_groups.iter().position(|row| row.contains(&group_idx))
    }

    fn group_col_in_row(&self, group_idx: usize, row: usize) -> Option<usize> {
        self.row_groups.get(row).and_then(|r| r.iter().position(|&g| g == group_idx))
    }

    pub fn move_left(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
        self.ensure_selection_visible();
    }

    pub fn move_right(&mut self) {
        if self.selected + 1 < self.total {
            self.selected += 1;
        }
        self.ensure_selection_visible();
    }

    pub fn move_up(&mut self) {
        let Some(g) = self.current_group() else { return };
        let Some(row) = self.group_row(g) else { return };
        if row == 0 { return; }

        let col = self.group_col_in_row(g, row).unwrap_or(0);
        let target_row = &self.row_groups[row - 1];
        let target_group = target_row[col.min(target_row.len() - 1)];
        let pos = self.position_in_group();
        let (start, size) = self.group_ranges[target_group];
        self.selected = start + pos.min(size - 1);
        self.ensure_selection_visible();
    }

    pub fn move_down(&mut self) {
        let Some(g) = self.current_group() else { return };
        let Some(row) = self.group_row(g) else { return };
        if row + 1 >= self.row_groups.len() { return; }

        let col = self.group_col_in_row(g, row).unwrap_or(0);
        let target_row = &self.row_groups[row + 1];
        let target_group = target_row[col.min(target_row.len() - 1)];
        let pos = self.position_in_group();
        let (start, size) = self.group_ranges[target_group];
        self.selected = start + pos.min(size - 1);
        self.ensure_selection_visible();
    }
}
```

Note `move_*` methods now end with `ensure_selection_visible()`. `set_selected` deliberately does NOT — callers that need scroll-follow (keyboard nav, reorder) will call it explicitly after; mouse click uses a different path.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib dashboard_nav::tests`
Expected: all five tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/dashboard_nav.rs
git commit -m "feat(dashboard): add scroll state and selection-follow to DashboardNav"
```

---

### Task 2: Clip-aware sprite renderers

**Files:**
- Modify: `src/creature/render.rs`
- Test: `tests/creature_test.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/creature_test.rs`:

```rust
#[test]
fn render_sprite_to_buffer_clipped_respects_clip_rect() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::{Position, Rect};
    use summoner::creature::generate::{CellKind, Sprite};
    use summoner::creature::render::{render_sprite_to_buffer_clipped, state_palette};
    use summoner::session::SessionState;

    // 4x4 sprite of solid body (all same color)
    let sprite = Sprite {
        width: 4,
        height: 4,
        cells: vec![CellKind::Body; 16],
    };
    let palette = state_palette(SessionState::Working);

    // Buffer 8x4; draw sprite at y=-2 (so top half clipped) with clip y>=0
    let area = Rect::new(0, 0, 8, 4);
    let mut buf = Buffer::empty(area);

    // Render at area starting above the buffer top.
    let draw_area = Rect { x: 0, y: 0, width: 4, height: 2 };
    let clip = Rect { x: 0, y: 1, width: 8, height: 3 };
    render_sprite_to_buffer_clipped(&sprite, &palette, draw_area, clip, &mut buf);

    // Row 0 must be empty (outside clip.y)
    for x in 0..4 {
        assert_eq!(buf[Position { x, y: 0 }].symbol(), " ",
                   "row 0 (outside clip) must not be written");
    }
    // Row 1 must have sprite content (inside clip)
    let any_nonblank = (0..4).any(|x| buf[Position { x, y: 1 }].symbol() != " ");
    assert!(any_nonblank, "row 1 (inside clip) must have sprite content");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test creature_test render_sprite_to_buffer_clipped_respects_clip_rect`
Expected: FAIL — `render_sprite_to_buffer_clipped` undefined.

- [ ] **Step 3: Add clipped variants**

Append to `src/creature/render.rs` (above `sprite_cell_size`):

```rust
fn within_clip(pos: Position, clip: Rect) -> bool {
    pos.x >= clip.x
        && pos.x < clip.x.saturating_add(clip.width)
        && pos.y >= clip.y
        && pos.y < clip.y.saturating_add(clip.height)
}

pub fn render_sprite_to_buffer_clipped(
    sprite: &Sprite,
    palette: &Palette,
    area: Rect,
    clip: Rect,
    buf: &mut Buffer,
) {
    let rows = sprite.height.div_ceil(2);

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
            if !within_clip(pos, clip) { continue; }

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
                            cell.set_symbol(LOWER_HALF);
                            cell.set_style(Style::default().fg(fg_color).bg(bg_color));
                        }
                    }
                }
            }
        }
    }
}

pub fn render_sprite_shaded_clipped(
    sprite: &Sprite,
    capsule_ids: &[u8],
    palette: &Palette,
    area: Rect,
    clip: Rect,
    buf: &mut Buffer,
) {
    let rows = sprite.height.div_ceil(2);

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

            let upper_id = capsule_ids.get(upper_y * sprite.width + col).copied().unwrap_or(0);
            let lower_id = if lower_y < sprite.height {
                capsule_ids.get(lower_y * sprite.width + col).copied().unwrap_or(0)
            } else {
                0
            };

            let pos = Position {
                x: area.x + col as u16,
                y: area.y + row as u16,
            };
            if !within_clip(pos, clip) { continue; }

            if let Some(cell) = buf.cell_mut(pos) {
                match (upper, lower) {
                    (CellKind::Empty, CellKind::Empty) => {}
                    (CellKind::Empty, lower_kind) => {
                        cell.set_symbol(LOWER_HALF);
                        cell.set_style(Style::default().fg(shaded_kind_color(&lower_kind, lower_id, palette)));
                    }
                    (upper_kind, CellKind::Empty) => {
                        cell.set_symbol(UPPER_HALF);
                        cell.set_style(Style::default().fg(shaded_kind_color(&upper_kind, upper_id, palette)));
                    }
                    (upper_kind, lower_kind) => {
                        let fg_color = shaded_kind_color(&lower_kind, lower_id, palette);
                        let bg_color = shaded_kind_color(&upper_kind, upper_id, palette);
                        if fg_color == bg_color {
                            cell.set_symbol(FULL_BLOCK);
                            cell.set_style(Style::default().fg(fg_color));
                        } else {
                            cell.set_symbol(LOWER_HALF);
                            cell.set_style(Style::default().fg(fg_color).bg(bg_color));
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test creature_test render_sprite_to_buffer_clipped_respects_clip_rect`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/creature/render.rs tests/creature_test.rs
git commit -m "feat(creature): add clip-aware sprite render variants"
```

---

### Task 3: Peek-layout math helper (pure function)

**Files:**
- Modify: `src/ui/dashboard.rs` (add module-private function + tests)

- [ ] **Step 1: Write the failing tests**

Append inside a new `#[cfg(test)] mod peek_tests { ... }` at the bottom of `src/ui/dashboard.rs`:

```rust
#[cfg(test)]
mod peek_tests {
    use super::*;

    #[test]
    fn no_overflow_no_peek() {
        // H=34, row_stride=17 -> natural=2, total_rows=2 -> no peek
        let s = scroll_layout(34, 17, 2, 0);
        assert_eq!(s.full_rows, 2);
        assert_eq!(s.top_peek, 0);
        assert_eq!(s.bottom_peek, 0);
    }

    #[test]
    fn awkward_leftover_becomes_peek() {
        // H=40, stride=17 -> natural=2, leftover=6. 4 rows total, at top: below hidden.
        let s = scroll_layout(40, 17, 4, 0);
        assert_eq!(s.full_rows, 2);
        assert_eq!(s.top_peek, 0);
        assert_eq!(s.bottom_peek, 6);
    }

    #[test]
    fn awkward_leftover_splits_when_both_sides_hidden() {
        // Same as above, scrolled to middle -> both sides hidden
        let s = scroll_layout(40, 17, 4, 1);
        assert_eq!(s.full_rows, 2);
        assert_eq!(s.top_peek, 3);
        assert_eq!(s.bottom_peek, 3);
    }

    #[test]
    fn clean_multiple_sacrifices_row() {
        // H=34, stride=17, leftover=0, overflow -> sacrifice 1 row, peek_budget=17.
        let s = scroll_layout(34, 17, 4, 0);
        assert_eq!(s.full_rows, 1);
        assert_eq!(s.top_peek, 0);
        assert_eq!(s.bottom_peek, 17);
    }

    #[test]
    fn one_row_fits_no_sacrifice() {
        // H=17, stride=17, leftover=0, natural=1, overflow (total=3): can't sacrifice.
        let s = scroll_layout(17, 17, 3, 0);
        assert_eq!(s.full_rows, 1);
        assert_eq!(s.top_peek, 0);
        assert_eq!(s.bottom_peek, 0);
    }

    #[test]
    fn zero_rows_fallback() {
        // Too short for a full row
        let s = scroll_layout(5, 17, 3, 0);
        assert_eq!(s.full_rows, 0);
        assert_eq!(s.top_peek, 0);
        assert_eq!(s.bottom_peek, 0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib peek_tests`
Expected: FAIL — `scroll_layout` and `ScrollLayout` undefined.

- [ ] **Step 3: Implement peek math**

Add above `CardLayout` near the bottom of `src/ui/dashboard.rs`:

```rust
/// Computed peek/full-row layout for one frame.
#[derive(Debug, Clone, Copy)]
pub struct ScrollLayout {
    pub full_rows: usize,
    pub top_peek: u16,
    pub bottom_peek: u16,
}

/// Compute peek budget split for the current viewport state.
///
/// `content_height` = available vertical cells (rows); `row_stride` = card_height + 1;
/// `total_rows` = number of flow-layout rows; `scroll_row` = topmost full-band row.
pub fn scroll_layout(
    content_height: u16,
    row_stride: u16,
    total_rows: usize,
    scroll_row: usize,
) -> ScrollLayout {
    if row_stride == 0 || content_height == 0 {
        return ScrollLayout { full_rows: 0, top_peek: 0, bottom_peek: 0 };
    }
    let natural_full_rows = (content_height / row_stride) as usize;
    if natural_full_rows == 0 {
        return ScrollLayout { full_rows: 0, top_peek: 0, bottom_peek: 0 };
    }
    let leftover = content_height % row_stride;

    let (full_rows, peek_budget): (usize, u16) = if total_rows <= natural_full_rows {
        (natural_full_rows, 0)
    } else if leftover >= 2 {
        (natural_full_rows, leftover)
    } else if natural_full_rows >= 2 {
        (natural_full_rows - 1, leftover + row_stride)
    } else {
        (natural_full_rows, leftover)
    };

    let has_above = scroll_row > 0;
    let has_below = scroll_row + full_rows < total_rows;
    let top_peek = if has_above && has_below {
        peek_budget / 2
    } else if has_above {
        peek_budget
    } else {
        0
    };
    let bottom_peek = if has_below { peek_budget - top_peek } else { 0 };

    ScrollLayout { full_rows, top_peek, bottom_peek }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib peek_tests`
Expected: all six PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/dashboard.rs
git commit -m "feat(dashboard): add scroll_layout peek-sizing math"
```

---

### Task 4: Clip-aware drawing helpers

**Files:**
- Modify: `src/ui/dashboard.rs`

- [ ] **Step 1: Update `draw_text` and `draw_selection_box` to take a clip rect**

Replace the existing `draw_text` function in `src/ui/dashboard.rs` with:

```rust
fn draw_text(x: u16, y: u16, text: &str, style: Style, clip: Rect, buf: &mut Buffer) {
    if y < clip.y || y >= clip.y.saturating_add(clip.height) { return; }
    for (i, ch) in text.chars().enumerate() {
        let px = x + i as u16;
        if px < clip.x { continue; }
        if px >= clip.x.saturating_add(clip.width) { break; }
        if let Some(cell) = buf.cell_mut(Position { x: px, y }) {
            cell.set_symbol(&ch.to_string());
            cell.set_style(style);
        }
    }
}
```

Replace `draw_selection_box` with:

```rust
fn draw_selection_box(area: Rect, clip: Rect, buf: &mut Buffer, color: Color) {
    let style = Style::default().fg(color);
    let x1 = area.x;
    let y1 = area.y;
    let x2 = area.x + area.width.saturating_sub(1);
    let y2 = area.y + area.height.saturating_sub(1);

    let in_clip = |p: Position| {
        p.x >= clip.x
            && p.x < clip.x.saturating_add(clip.width)
            && p.y >= clip.y
            && p.y < clip.y.saturating_add(clip.height)
    };
    let set = |buf: &mut Buffer, p: Position, sym: &str| {
        if !in_clip(p) { return; }
        if let Some(cell) = buf.cell_mut(p) {
            cell.set_symbol(sym);
            cell.set_style(style);
        }
    };

    set(buf, Position { x: x1, y: y1 }, "\u{250c}");
    set(buf, Position { x: x2, y: y1 }, "\u{2510}");
    set(buf, Position { x: x1, y: y2 }, "\u{2514}");
    set(buf, Position { x: x2, y: y2 }, "\u{2518}");

    for x in (x1 + 1)..x2 {
        set(buf, Position { x, y: y1 }, "\u{2500}");
        set(buf, Position { x, y: y2 }, "\u{2500}");
    }
    for y in (y1 + 1)..y2 {
        set(buf, Position { x: x1, y }, "\u{2502}");
        set(buf, Position { x: x2, y }, "\u{2502}");
    }
}
```

- [ ] **Step 2: Fix all `draw_text` call sites in `Dashboard::render` and `draw_usage_bar`**

Every existing `draw_text(..., area, buf)` call in `src/ui/dashboard.rs` now passes `clip` as the 5th arg. The caller's natural `clip` is the area it was already bounded to. Do a mechanical rename — pass the same `area` or `card_area`/`creature_col` / `content_area` the call already used as its bounds. For `draw_usage_bar`, keep passing `area` (unchanged behaviour).

Also replace the one existing `draw_selection_box(box_area, buf, box_color)` with `draw_selection_box(box_area, card_area, buf, box_color)` (the card area is the natural clip). We'll replace this with `content_area` in Task 5 when peek rendering needs tighter clipping.

- [ ] **Step 3: Run build to verify it compiles**

Run: `cargo build`
Expected: success, no warnings beyond existing.

- [ ] **Step 4: Run existing tests to check no regressions**

Run: `cargo test`
Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/dashboard.rs
git commit -m "refactor(dashboard): thread clip Rect through draw_text and draw_selection_box"
```

---

### Task 5: Add `blit_clipped` helper

**Files:**
- Modify: `src/ui/dashboard.rs`

**Strategy:** Ratatui's `Block` writes its border/padding anywhere inside `card_area` without a clip concept. Rather than reimplement Block manually, render each frame's cards into a **scratch `Buffer`** that covers `content_area` plus a vertical margin for peek rows, then blit only the cells inside `content_area` back into the real buffer. This gives free clipping for Block and any future Ratatui widget.

- [ ] **Step 1: Add the blit helper**

Add near the top of `src/ui/dashboard.rs` (after the `CREATURE_HEIGHT` constant or alongside other helpers):

```rust
/// Copy cells from `src` into `dst` for the overlap between `src_area` and `dst_clip`.
/// Cells in `src` are positioned at `src_area.x + col, src_area.y + row`.
fn blit_clipped(src: &Buffer, src_area: Rect, dst: &mut Buffer, dst_clip: Rect) {
    let x_start = src_area.x.max(dst_clip.x);
    let y_start = src_area.y.max(dst_clip.y);
    let x_end = (src_area.x + src_area.width).min(dst_clip.x + dst_clip.width);
    let y_end = (src_area.y + src_area.height).min(dst_clip.y + dst_clip.height);
    if x_start >= x_end || y_start >= y_end { return; }

    for y in y_start..y_end {
        for x in x_start..x_end {
            if let (Some(src_cell), Some(dst_cell)) = (
                src.cell(Position { x, y }),
                dst.cell_mut(Position { x, y }),
            ) {
                *dst_cell = src_cell.clone();
            }
        }
    }
}
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build`
Expected: success (helper is unused until Task 6 wires it up).

- [ ] **Step 3: Commit**

```bash
git add src/ui/dashboard.rs
git commit -m "refactor(dashboard): add blit_clipped helper for scratch-buffer peek rendering"
```

---

### Task 6: Wire peek rendering into Dashboard::render

**Files:**
- Modify: `src/ui/dashboard.rs`

- [ ] **Step 1: Rewrite the render loop to use scratch buffer + peek rows**

In `src/ui/dashboard.rs`, inside `Dashboard::render`, replace the existing card-iteration block (from `let groups = group_by_project(...)` through the end of the `for (group_idx, group)` loop) with the following. Note: `card_inner_height`, `card_height`, `row_stride` remain as computed today.

```rust
let groups = group_by_project(self.sessions);

if groups.is_empty() {
    let msg = "No sessions. Press N to open a directory.";
    let msg_len = msg.len() as u16;
    let mx = content_area.x + content_area.width.saturating_sub(msg_len) / 2;
    let my = content_area.y + content_area.height / 2;
    let msg_style = Style::default().fg(Color::Rgb(120, 120, 140));
    draw_text(mx, my, msg, msg_style, content_area, buf);
    return;
}

let group_sizes: Vec<usize> = groups.iter().map(|g| g.sessions.len()).collect();
let layout = flow_layout(&group_sizes, content_area.width);
if layout.cards.is_empty() { return; }

let basenames: Vec<&str> = groups.iter().map(|g| {
    std::path::Path::new(&g.directory)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
}).collect();
let needs_full_path: Vec<bool> = basenames.iter().enumerate().map(|(i, name)| {
    basenames.iter().enumerate().any(|(j, other)| i != j && name == other)
}).collect();

let creature_render_h = CREATURE_HEIGHT.div_ceil(2);
let card_inner_height = 1 + 1 + creature_render_h + 1 + 1 + 1;
let card_height = card_inner_height + 2;
let row_stride = card_height + 1;

let total_rows = layout.row_groups.len();
let sc = scroll_layout(content_area.height, row_stride, total_rows, self.nav.scroll_row());

// Publish full_rows for input handlers
self.nav.set_visible_full_rows(sc.full_rows);
// Clamp scroll_row against the (possibly shrunken) max and pull selection into view.
self.nav.ensure_selection_visible();
let scroll_row = self.nav.scroll_row();

// Fallback: terminal too short
if sc.full_rows == 0 {
    // Render what fits, old behaviour (no peek)
    self.render_flat_rows(
        &groups, &group_sizes, &layout, &basenames, &needs_full_path,
        content_area, card_height, row_stride, creature_render_h,
        buf, 0, usize::MAX,
    );
    return;
}

// Scratch buffer covers the content_area plus a margin on top and bottom
// so peek cards that start outside content_area still have a place to render.
let scratch_y = content_area.y.saturating_sub(row_stride);
let scratch_height = content_area.height + 2 * row_stride;
let scratch_area = Rect {
    x: content_area.x,
    y: scratch_y,
    width: content_area.width,
    height: scratch_height,
};
let mut scratch = Buffer::empty(scratch_area);
fill_background(scratch_area, &mut scratch);

// Compute full-band origin within scratch coords (content_area is inside scratch_area).
let full_band_y = content_area.y + sc.top_peek;

// Rows to render: optional above-peek, full band, optional below-peek.
let first_row = if sc.top_peek > 0 && scroll_row > 0 { scroll_row - 1 } else { scroll_row };
let last_row = {
    let mut end = scroll_row + sc.full_rows;
    if sc.bottom_peek > 0 && end < total_rows { end += 1; }
    end
};

for row_idx in first_row..last_row.min(total_rows) {
    // Card y position for this row — above-peek row gets a negative offset.
    let card_y = if row_idx < scroll_row {
        // above-peek: position so only the bottom `top_peek` rows are inside content_area
        full_band_y.saturating_sub(row_stride)
    } else {
        full_band_y + (row_idx - scroll_row) as u16 * row_stride
    };

    for &group_idx in &layout.row_groups[row_idx] {
        let card_pos = &layout.cards[group_idx];
        let card_x = content_area.x + card_pos.x;
        let card_width = card_pos.width;

        let card_area = Rect {
            x: card_x,
            y: card_y,
            width: card_width,
            height: card_height,
        };

        // Flat position start for this group
        let flat_pos_start: usize = group_sizes.iter().take(group_idx).sum();

        self.render_card_into(
            group_idx,
            card_area,
            content_area, // clip = real content_area; peek cells outside are clipped
            &groups,
            &basenames,
            &needs_full_path,
            flat_pos_start,
            self.nav.selected(),
            creature_render_h,
            &mut scratch,
        );
    }
}

// Blit scratch -> buf for the content_area region only.
blit_clipped(&scratch, scratch_area, buf, content_area);
```

- [ ] **Step 2: Add the `render_card_into` and `render_flat_rows` helpers**

Add these methods inside the existing `impl<'a> Dashboard<'a>` block. `render_card_into` is the body of the old per-card loop, extracted:

```rust
impl<'a> Dashboard<'a> {
    #[allow(clippy::too_many_arguments)]
    fn render_card_into(
        &mut self,
        group_idx: usize,
        card_area: Rect,
        clip: Rect,
        groups: &[crate::session::ProjectGroup],
        basenames: &[&str],
        needs_full_path: &[bool],
        flat_pos_start: usize,
        selected: usize,
        creature_render_h: u16,
        buf: &mut Buffer,
    ) {
        let group = &groups[group_idx];
        let border_color = Color::Rgb(60, 60, 80);

        let display_name = if needs_full_path[group_idx] {
            crate::app::display_path(&group.directory)
        } else {
            basenames[group_idx].to_string()
        };
        let title_style = Style::default().fg(Color::Rgb(140, 140, 160));

        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .padding(ratatui::widgets::Padding::uniform(1));
        let inner = block.inner(card_area);
        block.render(card_area, buf);

        let (git_adds, git_dels) = self.git_cache.get(&group.directory);
        let diff_str = crate::git::format_diff_compact(git_adds, git_dels);

        let title_x = card_area.x + 2;
        let title_y = card_area.y;
        let name_text = format!(" {} ", display_name);
        draw_text(title_x, title_y, &name_text, title_style, clip, buf);

        if !diff_str.is_empty() {
            let diff_x = title_x + name_text.len() as u16;
            let mut x = diff_x;
            let mut color = Color::Rgb(129, 199, 132);
            for ch in diff_str.chars() {
                if ch == '-' && x > diff_x {
                    color = Color::Rgb(229, 115, 115);
                }
                draw_text(x, title_y, &ch.to_string(), Style::default().fg(color), clip, buf);
                x += 1;
            }
            draw_text(x, title_y, " ", Style::default().fg(border_color), clip, buf);
        }

        for (local_idx, &sess_idx) in group.sessions.iter().enumerate() {
            let session = &self.sessions[sess_idx];

            let creature_spacing = CREATURE_WIDTH + 2;
            let cx = inner.x + local_idx as u16 * creature_spacing;
            let inner_right = inner.x + inner.width;
            let cw = CREATURE_WIDTH.min(inner_right.saturating_sub(cx));

            if cw == 0 || cx >= inner_right {
                continue;
            }

            let creature_col = Rect {
                x: cx,
                y: card_area.y,
                width: cw,
                height: card_area.height,
            };
            // Intersect with clip for tighter text clipping.
            let col_clip = rect_intersect(creature_col, clip);

            let is_active_claude = session.state != SessionState::Disconnected
                && session.state != SessionState::ShellOnly
                && (session.claude_conversation_id.is_some()
                    || session.state == SessionState::Working
                    || session.state == SessionState::Waiting
                    || session.state == SessionState::Idle);

            let health_y = inner.y;
            let lvl_xp_y = inner.y + 1;
            let creature_y = inner.y + 2;
            let state_y = creature_y + creature_render_h;

            if is_active_claude {
                let default_stats = SessionStats::new();
                let stat = self.session_stats.get(sess_idx).unwrap_or(&default_stats);
                let pct = stat.context_pct.unwrap_or(0);
                let pct_str = if stat.context_pct.is_some() {
                    format!(" {}%", pct)
                } else {
                    " ---%".to_string()
                };
                let bar_total = cw.saturating_sub(pct_str.len() as u16) as usize;
                let damaged = (bar_total as u64 * pct as u64 / 100).min(bar_total as u64) as usize;
                let healthy = bar_total.saturating_sub(damaged);

                let healthy_str: String = "\u{2580}".repeat(healthy);
                draw_text(cx, health_y, &healthy_str, Style::default().fg(Color::Rgb(129, 199, 132)), col_clip, buf);
                let damaged_str: String = "\u{2580}".repeat(damaged);
                draw_text(cx + healthy as u16, health_y, &damaged_str, Style::default().fg(Color::Rgb(229, 115, 115)), col_clip, buf);
                let pct_color = if pct >= 90 { Color::Rgb(229, 115, 115) } else { Color::Rgb(136, 136, 136) };
                draw_text(cx + bar_total as u16, health_y, &pct_str, Style::default().fg(pct_color), col_clip, buf);
            }

            let creature_area = Rect {
                x: cx,
                y: creature_y,
                width: cw,
                height: creature_render_h,
            };

            let is_claude_sprite = session.claude_conversation_id.is_some()
                || session.state == SessionState::Working
                || session.state == SessionState::Waiting
                || session.state == SessionState::Idle;

            if is_claude_sprite {
                if let Some(raster) = self.rasters.get(sess_idx) {
                    let palette = state_palette(session.state);
                    render_sprite_shaded_clipped(&raster.sprite, &raster.capsule_ids, &palette, creature_area, clip, buf);
                }
            } else {
                let icon = terminal_icon_sprite();
                let palette = state_palette(session.state);
                let (icon_w, icon_h) = crate::creature::render::sprite_cell_size(&icon);
                let x_offset = creature_area.width.saturating_sub(icon_w) / 2;
                let y_offset = creature_area.height.saturating_sub(icon_h) / 2;
                let centered_area = Rect {
                    x: creature_area.x + x_offset,
                    y: creature_area.y + y_offset,
                    width: icon_w.min(creature_area.width.saturating_sub(x_offset)),
                    height: creature_area.height.saturating_sub(y_offset),
                };
                render_sprite_to_buffer_clipped(&icon, &palette, centered_area, clip, buf);
            }

            if is_active_claude && lvl_xp_y < card_area.y + card_area.height.saturating_sub(2) {
                let default_stats = SessionStats::new();
                let stat = self.session_stats.get(sess_idx).unwrap_or(&default_stats);
                let level = stats::level_from_tokens(stat.total_tokens);
                let xp = stats::format_xp(stat.total_tokens);
                let lvl_color = match level {
                    1 => Color::Rgb(140, 140, 160),
                    2 => Color::Rgb(129, 199, 132),
                    3 => Color::Rgb(100, 181, 246),
                    4 => Color::Rgb(149, 117, 205),
                    5 => Color::Rgb(255, 213, 79),
                    6 => Color::Rgb(255, 152, 0),
                    7 => Color::Rgb(244, 67, 54),
                    8 => Color::Rgb(233, 30, 99),
                    9 => Color::Rgb(0, 230, 230),
                    _ => Color::Rgb(255, 255, 100),
                };
                let lvl_text = format!("Lv.{}", level);
                draw_text(cx, lvl_xp_y, &lvl_text, Style::default().fg(lvl_color), col_clip, buf);

                let xp_len = xp.chars().count() as u16;
                let xp_x = cx + cw.saturating_sub(xp_len);
                draw_text(xp_x, lvl_xp_y, &xp, Style::default().fg(Color::Rgb(255, 213, 79)), col_clip, buf);
            }

            if state_y < card_area.y + card_area.height.saturating_sub(1) {
                if is_active_claude {
                    let default_stats = SessionStats::new();
                    let stat = self.session_stats.get(sess_idx).unwrap_or(&default_stats);
                    let activity = if let Some(ref tool) = stat.active_tool {
                        stats::tool_display(tool)
                    } else {
                        format!("{}  {}", session.state.icon(), session.state.label())
                    };
                    let state_style = Style::default().fg(session.state.color());
                    draw_text(cx, state_y, &activity, state_style, col_clip, buf);
                } else {
                    let label = format!("{}  {}", session.state.icon(), session.state.label());
                    let label_style = Style::default().fg(session.state.color());
                    draw_text(cx, state_y, &label, label_style, col_clip, buf);
                }
            }

            let flat_pos = flat_pos_start + local_idx;
            if flat_pos == selected {
                let box_x = cx.saturating_sub(1).max(card_area.x + 1);
                let box_y = health_y.saturating_sub(1).max(card_area.y + 1);
                let box_right = (cx + cw + 1).min(card_area.x + card_area.width - 1);
                let box_bottom = (state_y + 2).min(card_area.y + card_area.height - 1);
                let box_w = box_right.saturating_sub(box_x);
                let box_h = box_bottom.saturating_sub(box_y);
                if box_w >= 3 && box_h >= 3 {
                    let box_area = Rect { x: box_x, y: box_y, width: box_w, height: box_h };
                    let box_color = if self.reordering {
                        Color::Rgb(255, 180, 50)
                    } else {
                        Color::Rgb(150, 150, 255)
                    };
                    draw_selection_box(box_area, clip, buf, box_color);
                }
            }
        }
    }

    /// Fallback path when `full_rows == 0` — render rows top-down, skipping rows past bottom.
    #[allow(clippy::too_many_arguments)]
    fn render_flat_rows(
        &mut self,
        groups: &[crate::session::ProjectGroup],
        group_sizes: &[usize],
        layout: &FlowLayout,
        basenames: &[&str],
        needs_full_path: &[bool],
        content_area: Rect,
        card_height: u16,
        row_stride: u16,
        creature_render_h: u16,
        buf: &mut Buffer,
        first_row: usize,
        max_rows: usize,
    ) {
        let selected = self.nav.selected();
        let end = (first_row + max_rows).min(layout.row_groups.len());
        for row_idx in first_row..end {
            let card_y = content_area.y + 1 + (row_idx - first_row) as u16 * row_stride;
            if card_y + card_height > content_area.y + content_area.height {
                break;
            }
            for &group_idx in &layout.row_groups[row_idx] {
                let card_pos = &layout.cards[group_idx];
                let card_x = content_area.x + card_pos.x;
                let card_area = Rect {
                    x: card_x,
                    y: card_y,
                    width: card_pos.width,
                    height: card_height,
                };
                let flat_pos_start: usize = group_sizes.iter().take(group_idx).sum();
                self.render_card_into(
                    group_idx,
                    card_area,
                    content_area,
                    groups,
                    basenames,
                    needs_full_path,
                    flat_pos_start,
                    selected,
                    creature_render_h,
                    buf,
                );
            }
        }
    }
}
```

Also add a small utility near the other helpers:

```rust
fn rect_intersect(a: Rect, b: Rect) -> Rect {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width).min(b.x + b.width);
    let y2 = (a.y + a.height).min(b.y + b.height);
    if x2 <= x1 || y2 <= y1 {
        Rect { x: x1, y: y1, width: 0, height: 0 }
    } else {
        Rect { x: x1, y: y1, width: x2 - x1, height: y2 - y1 }
    }
}
```

`Buffer::cell` is needed for `blit_clipped`. Confirm it exists (ratatui ≥ 0.26) — if not, use `buf.content.get(i)` via Position indexing via `buf[Position{...}]`.

- [ ] **Step 3: Build and run tests**

Run: `cargo build && cargo test`
Expected: compilation succeeds; all existing + peek_tests PASS.

- [ ] **Step 4: Smoke-test manually**

Run: `cargo run --release` and open enough sessions to overflow. Verify:
- Peek cards visible at top/bottom when hidden rows exist on that side
- Selecting past the bottom row scrolls
- Terminal resize adjusts live

- [ ] **Step 5: Commit**

```bash
git add src/ui/dashboard.rs
git commit -m "feat(dashboard): render peek rows with scratch-buffer blit"
```

---

### Task 7: Scroll-aware hit testing

**Files:**
- Modify: `src/ui/dashboard.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Update `session_at_position` to take scroll state**

In `src/ui/dashboard.rs`, replace the `session_at_position` function with:

```rust
/// Hit-test: given a mouse position, return the flat session index clicked (if any).
pub fn session_at_position(
    group_sizes: &[usize],
    content_area: Rect,
    scroll_row: usize,
    card_height: u16,
    row_stride: u16,
    mouse_row: u16,
    mouse_col: u16,
) -> Option<usize> {
    if group_sizes.is_empty() { return None; }

    let layout = flow_layout(group_sizes, content_area.width);
    let total_rows = layout.row_groups.len();
    let sc = scroll_layout(content_area.height, row_stride, total_rows, scroll_row);
    let full_band_y = content_area.y + sc.top_peek;

    // Determine which layout row the mouse is inside (if any).
    // Above-peek row
    let candidate_rows: Vec<(usize, u16)> = {
        let mut v = Vec::new();
        if sc.top_peek > 0 && scroll_row > 0 {
            v.push((scroll_row - 1, full_band_y.saturating_sub(row_stride)));
        }
        for r in 0..sc.full_rows {
            let idx = scroll_row + r;
            if idx >= total_rows { break; }
            v.push((idx, full_band_y + r as u16 * row_stride));
        }
        if sc.bottom_peek > 0 && scroll_row + sc.full_rows < total_rows {
            let idx = scroll_row + sc.full_rows;
            v.push((idx, full_band_y + sc.full_rows as u16 * row_stride));
        }
        v
    };

    for (row_idx, card_y) in candidate_rows {
        if mouse_row < card_y || mouse_row >= card_y + card_height { continue; }
        for &group_idx in &layout.row_groups[row_idx] {
            let card_pos = &layout.cards[group_idx];
            let card_x = content_area.x + card_pos.x;
            let card_width = card_pos.width;
            if mouse_col < card_x || mouse_col >= card_x + card_width { continue; }

            let inner_x = card_x + 2;
            let creature_spacing = CREATURE_WIDTH + 2;
            let flat_pos_start: usize = group_sizes.iter().take(group_idx).sum();
            let size = group_sizes[group_idx];
            for local_idx in 0..size {
                let cx = inner_x + local_idx as u16 * creature_spacing;
                if mouse_col >= cx && mouse_col < cx + CREATURE_WIDTH {
                    return Some(flat_pos_start + local_idx);
                }
            }
            return Some(flat_pos_start);
        }
    }
    None
}
```

- [ ] **Step 2: Update the call site in `src/app.rs`**

Find the call around `src/app.rs:1451`:

```rust
if let Some(pos) = session_at_position(
    &group_sizes, content_area, mouse.row, mouse.column,
) {
```

Replace with:

```rust
let creature_render_h = 12u16; // CREATURE_HEIGHT / 2 = 24 / 2
let card_inner_height = 1 + 1 + creature_render_h + 1 + 1 + 1;
let card_height = card_inner_height + 2;
let row_stride = card_height + 1;
if let Some(pos) = session_at_position(
    &group_sizes,
    content_area,
    app.nav.scroll_row(),
    card_height,
    row_stride,
    mouse.row,
    mouse.column,
) {
```

If `CREATURE_HEIGHT` is not re-exported from `ui::dashboard`, export it by adding `pub const CREATURE_HEIGHT: u16 = 24;` to the module-public section, OR add a helper `pub fn card_metrics() -> (u16, u16)` returning `(card_height, row_stride)` to `src/ui/dashboard.rs` and import/use that instead. Prefer the helper:

In `src/ui/dashboard.rs`, add:

```rust
pub fn card_metrics() -> (u16, u16) {
    let creature_render_h = CREATURE_HEIGHT.div_ceil(2);
    let card_inner_height = 1 + 1 + creature_render_h + 1 + 1 + 1;
    let card_height = card_inner_height + 2;
    let row_stride = card_height + 1;
    (card_height, row_stride)
}
```

And in `src/app.rs`:

```rust
use crate::ui::dashboard::card_metrics;
// ...
let (card_height, row_stride) = card_metrics();
if let Some(pos) = session_at_position(
    &group_sizes,
    content_area,
    app.nav.scroll_row(),
    card_height,
    row_stride,
    mouse.row,
    mouse.column,
) {
```

Also make sure `card_metrics` is used inside `Dashboard::render` in Task 6 for consistency — replace the inline `creature_render_h = ... card_height = ...` computations with a call to `card_metrics()`.

- [ ] **Step 3: Build and run tests**

Run: `cargo build && cargo test`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/ui/dashboard.rs src/app.rs
git commit -m "feat(dashboard): scroll-aware mouse hit testing"
```

---

### Task 8: Mouse wheel + explicit ensure_selection_visible

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Handle wheel events in dashboard mouse arm**

In `src/app.rs` around line 1440, replace:

```rust
else if matches!(app.mode, Mode::Dashboard) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // ... existing body
        }
        _ => {}
    }
}
```

with:

```rust
else if matches!(app.mode, Mode::Dashboard) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.nav.scroll_up();
        }
        MouseEventKind::ScrollDown => {
            app.nav.scroll_down();
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // ... existing body unchanged
        }
        _ => {}
    }
}
```

- [ ] **Step 2: Follow selection after reorder swaps and mouse click**

Reorder methods call `self.nav.set_selected(new_pos)` (lines ~283, ~315 in `src/app.rs`). Selection-follow isn't automatic on `set_selected`. Add `self.nav.ensure_selection_visible();` immediately after each of those two `set_selected` calls.

Similarly in the mouse click handler (around line 1475): after `app.nav.set_selected(pos);` add `app.nav.ensure_selection_visible();`.

- [ ] **Step 3: Build and run tests**

Run: `cargo build && cargo test`
Expected: PASS.

- [ ] **Step 4: Smoke-test manually**

Run: `cargo run --release`. With many sessions:
- Scroll wheel up/down on dashboard — viewport scrolls without changing selection.
- Arrow-down past bottom — viewport scrolls to follow selection.
- Reorder (`r`) across rows — moving card stays visible.
- Click a peek card — selects a session in it, viewport snaps.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat(dashboard): mouse wheel scroll and selection-follow for nav"
```

---

### Task 9: Final validation

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: all PASS.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: Release build**

Run: `cargo build --release`
Expected: success.

- [ ] **Step 4: Manual verification checklist**

- [ ] Overflow detected only when >N rows; no peek when everything fits.
- [ ] Peek card visible at bottom when scrolled to top + overflow below.
- [ ] Peek card visible at top when scrolled to middle/bottom.
- [ ] Both peeks visible when scrolled to middle.
- [ ] Wheel scroll does not change selection.
- [ ] Arrow-down/up snaps viewport to follow selection.
- [ ] Mouse click on peek selects and scrolls.
- [ ] Terminal resize adapts peek budget live.
- [ ] No overwrite of usage bar or hint row during scroll.

- [ ] **Step 5: Commit final polish if needed**

```bash
git status
# If anything dangling:
git add -A && git commit -m "chore: final polish for dashboard scroll"
```
