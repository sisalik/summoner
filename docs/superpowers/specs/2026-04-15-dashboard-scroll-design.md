# Dashboard scrolling with peek indicators

## Problem

When the number of project cards exceeds what fits vertically in the dashboard, later rows are hidden entirely (`src/ui/dashboard.rs:118-120` breaks out of the render loop). There's no indication that more sessions exist, and selection can move to hidden cards with no viewport follow-through.

## Goal

- Scroll the dashboard viewport when there are more rows than can fit.
- Always show a partially-clipped card above and/or below the fully-rendered band as a permanent "more exists" signal.
- Support both keyboard (selection-driven) and mouse wheel scrolling.
- Responsive to terminal resize.

## Design

### Scroll state

Extend `DashboardNav` (`src/ui/dashboard_nav.rs`) with:

- `scroll_row: usize` — index of the topmost row in the fully-rendered band.
- `visible_full_rows: usize` — cached during render so input handlers can call `ensure_selection_visible()` without re-measuring.

New methods:

- `scroll_up(&mut self)` / `scroll_down(&mut self)` — adjust `scroll_row` by one, clamped to `[0, max_scroll_row]` where `max_scroll_row = total_rows.saturating_sub(visible_full_rows)`. Do not affect `selected`.
- `ensure_selection_visible(&mut self)` — using `visible_full_rows`, if the row containing `selected` is below the visible band, advance `scroll_row` so the selected row becomes the last fully-visible row; if above, make it the first.
- `scroll_row(&self) -> usize` getter.
- `set_visible_full_rows(&mut self, n: usize)` — called by the renderer each frame.

`ensure_selection_visible` is called after every `move_up` / `move_down` / `move_left` / `move_right` / `set_selected` from `app.rs`.

### Peek sizing (responsive)

Given `content_area.height = H` and `row_stride = card_height + 1`, each frame computes:

```
natural_full_rows = H / row_stride
leftover          = H % row_stride

if total_rows <= natural_full_rows:
    full_rows   = natural_full_rows
    peek_budget = 0                              // no overflow; no peek
else if leftover >= 2:
    full_rows   = natural_full_rows
    peek_budget = leftover                       // use awkward leftover
else if natural_full_rows >= 2:
    full_rows   = natural_full_rows - 1
    peek_budget = leftover + row_stride          // sacrifice one full row
else:
    full_rows   = natural_full_rows              // == 1; can't sacrifice
    peek_budget = leftover                       // peek from leftover only (may be 0 or 1)
```

Allocation between top/bottom is driven by which direction has hidden rows:

```
has_above = scroll_row > 0
has_below = scroll_row + full_rows < total_rows

top_peek    = if has_above && has_below { peek_budget / 2 }
              else if has_above         { peek_budget }
              else                      { 0 }
bottom_peek = if has_below              { peek_budget - top_peek }
              else                      { 0 }
```

Fallback: if `natural_full_rows == 0` (terminal too short), skip peek logic entirely and use the existing "render what fits, break" path.

### Clip-aware rendering

Peek cards render outside the current `content_area`, so drawing must clip to a bounding rect rather than relying on `Buffer::cell_mut` bounds (which would overwrite the usage bar at `area.y` or the hint row at the bottom).

Changes:

- Add a `clip: Rect` parameter to `draw_text` in `src/ui/dashboard.rs`. Skip cells whose `y` is outside `[clip.y, clip.y + clip.height)` or `x` outside `[clip.x, clip.x + clip.width)`. Update all callers in `dashboard.rs`.
- Update `draw_selection_box` to take `clip: Rect` and clip edge/corner writes.
- Add clip-aware variants in `src/creature/render.rs`:
  - `render_sprite_shaded_clipped(sprite, capsule_ids, palette, area, clip, buf)`
  - `render_sprite_to_buffer_clipped(sprite, palette, area, clip, buf)`

  These wrap the existing functions (or take an extra arg) and skip cell writes outside `clip`.

### Render loop restructure

Rewrite the iteration inside `Dashboard::render`:

1. Compute `row_stride`, `natural_full_rows`, `full_rows`, `peek_budget`, `top_peek`, `bottom_peek` as above.
2. Determine the `full_band` rect:
   ```
   full_band_y      = content_area.y + top_peek
   full_band_height = full_rows * row_stride
   ```
3. Iterate rows to render:
   - If `has_above` and `top_peek > 0`: render row `scroll_row - 1` at `y = content_area.y - (row_stride - top_peek)` (negative offset into the peek band), clipped to `content_area`.
   - Rows `scroll_row ..= scroll_row + full_rows - 1`: render at `y = full_band_y + (r - scroll_row) * row_stride`, clipped to `content_area`.
   - If `has_below` and `bottom_peek > 0`: render row `scroll_row + full_rows` at `y = full_band_y + full_rows * row_stride`, clipped to `content_area`.
4. Selection box is drawn only when the selected row is inside the fully-rendered band (guaranteed after `ensure_selection_visible`).

Factor the per-card rendering into a helper like `render_card(...)` so the peek rows and band rows share code.

### Hit-testing

Update `session_at_position` (`src/ui/dashboard.rs`) to take `scroll_row`, `full_rows`, `top_peek`, `bottom_peek` (or a small `ScrollLayout` struct). Map `mouse_row` to a row index using the same offsets the renderer uses, then dispatch to the group at that row as today. Peek rows are clickable; clicking a peek card selects a session in it (which will scroll the viewport on the next render via `ensure_selection_visible`).

Callers in `app.rs` pass `scroll_row` from `nav` and recompute the layout.

### Mouse wheel

In `app.rs` in the Dashboard mouse arm (around `src/app.rs:1440`):

```rust
MouseEventKind::ScrollUp   => app.nav.scroll_up(),
MouseEventKind::ScrollDown => app.nav.scroll_down(),
```

Wheel events do not move selection. If the user scrolls the selection out of view and then presses an arrow key, `ensure_selection_visible` snaps the viewport back to selection — matching typical file-explorer behaviour.

### Reordering mode

In reorder mode, selection moves = card moves. Since `ensure_selection_visible` runs after every move, the moving card stays in view. No additional changes needed.

### Edge cases

- **Resize shrinks `full_rows`**: the render path clamps `scroll_row` to `max_scroll_row` each frame after computing `full_rows`, before iterating rows. Then calls `ensure_selection_visible` to pull the selection into the band if it fell out.
- **Sessions removed (total_rows shrinks)**: the same per-frame clamp handles this; `update_layout_with_rows` additionally clamps defensively so `scroll_row` is never stale between renders.
- **`natural_full_rows == 0`**: fall back to current behaviour — render what fits, no peek.
- **Single row, no overflow**: `full_rows = total_rows`, `peek_budget = 0`, no scrolling.

## Files touched

- `src/ui/dashboard_nav.rs` — scroll state + methods, layout-change clamping.
- `src/ui/dashboard.rs` — clip-aware draws, peek layout, `render_card` helper, `session_at_position` scroll-aware.
- `src/creature/render.rs` — clip-aware sprite render variants.
- `src/app.rs` — mouse wheel handler for Dashboard; `ensure_selection_visible` call sites after navigation; `session_at_position` invocation updated.

## Testing

- Unit tests in `dashboard_nav`: scroll clamping, `ensure_selection_visible` across edges, scroll state under layout shrink.
- Unit tests in `dashboard.rs`: peek-sizing math for the three branches (no overflow, awkward leftover, clean multiple), `session_at_position` with peek rows.
- Manual smoke test via `cargo run`: resize terminal, spam sessions across multiple rows, scroll with wheel and arrows, verify peek indicators appear on appropriate sides.
