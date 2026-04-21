// Dashboard grid — grouped by project, with selection boxes

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::creature::outline::RasterResult;
use crate::creature::render::{
    render_sprite_shaded_clipped, render_sprite_to_buffer_clipped, state_palette, terminal_icon_sprite,
};
use crate::session::{group_by_project, Session, SessionState, SessionStats, GlobalStats};
use crate::git::GitDiffCache;
use crate::stats;
use crate::ui::dashboard_nav::DashboardNav;

const CREATURE_WIDTH: u16 = 18;
const CREATURE_HEIGHT: u16 = 24;

pub fn card_metrics() -> (u16, u16) {
    let creature_render_h = CREATURE_HEIGHT.div_ceil(2);
    let card_inner_height = 1 + 1 + creature_render_h + 1 + 1 + 1;
    let card_height = card_inner_height + 2;
    let row_stride = card_height + 1;
    (card_height, row_stride)
}

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

pub struct Dashboard<'a> {
    sessions: &'a [Session],
    session_stats: &'a [SessionStats],
    global_stats: &'a GlobalStats,
    rasters: &'a [RasterResult],
    nav: &'a mut DashboardNav,
    git_cache: &'a mut GitDiffCache,
    reordering: bool,
}

impl<'a> Dashboard<'a> {
    pub fn new(
        sessions: &'a [Session],
        session_stats: &'a [SessionStats],
        global_stats: &'a GlobalStats,
        rasters: &'a [RasterResult],
        nav: &'a mut DashboardNav,
        git_cache: &'a mut GitDiffCache,
        reordering: bool,
    ) -> Self {
        Self { sessions, session_stats, global_stats, rasters, nav, git_cache, reordering }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        fill_background(area, buf);

        if area.height < 3 { return; }

        // Usage bar at top
        let usage_y = area.y;
        draw_usage_bar(self.global_stats, Rect { x: area.x, y: usage_y, width: area.width, height: 1 }, buf);

        // Hint row at bottom
        let hint_y = area.y + area.height.saturating_sub(1);
        let hint_text = if self.reordering {
            " \u{2190}\u{2192}\u{2191}\u{2193} move \u{2502} Enter/Esc done \u{2502} r cancel "
        } else {
            " \u{2190}\u{2192}\u{2191}\u{2193} navigate \u{2502} Enter open \u{2502} n session / N dir \u{2502} r reorder / R reroll \u{2502} x close / X dir \u{2502} Ctrl+Q quit "
        };
        let hint_style = Style::default()
            .fg(Color::Rgb(120, 120, 140))
            .bg(Color::Rgb(20, 20, 30));
        draw_text(area.x, hint_y, hint_text, hint_style, area, buf);

        // Content area (between top usage bar and bottom hints)
        let content_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(2),
        };

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

        let (card_height, row_stride) = card_metrics();
        let creature_render_h = CREATURE_HEIGHT.div_ceil(2);

        let total_rows = layout.row_groups.len();
        let sc = scroll_layout(content_area.height, row_stride, total_rows, self.nav.scroll_row());

        self.nav.set_visible_full_rows(sc.full_rows);
        self.nav.ensure_selection_visible();
        let scroll_row = self.nav.scroll_row();

        if sc.full_rows == 0 {
            self.render_flat_rows(
                &groups, &group_sizes, &layout, &basenames, &needs_full_path,
                content_area, card_height, row_stride, creature_render_h,
                buf, 0, usize::MAX,
            );
            return;
        }

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

        let full_band_y = content_area.y + sc.top_peek;

        let first_row = if sc.top_peek > 0 && scroll_row > 0 { scroll_row - 1 } else { scroll_row };
        let last_row = {
            let mut end = scroll_row + sc.full_rows;
            if sc.bottom_peek > 0 && end < total_rows { end += 1; }
            end
        };

        for row_idx in first_row..last_row.min(total_rows) {
            let card_y = if row_idx < scroll_row {
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

                let flat_pos_start: usize = group_sizes.iter().take(group_idx).sum();

                self.render_card_into(
                    group_idx,
                    card_area,
                    content_area,
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

        blit_clipped(&scratch, scratch_area, buf, content_area);
    }
}

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

pub struct CardLayout {
    pub row: usize,
    pub x: u16,
    pub width: u16,
}

pub struct FlowLayout {
    pub cards: Vec<CardLayout>,
    pub row_groups: Vec<Vec<usize>>,
}

/// Compute natural card width for a group with `session_count` sessions.
fn natural_card_width(session_count: usize) -> u16 {
    let creatures_w = session_count.max(1) as u16 * (CREATURE_WIDTH + 2);
    (creatures_w + 4).max(28) // border(2) + padding(2), floor at 28
}

/// Flow layout: each card gets its natural width, packed left-to-right with row wrapping.
pub fn flow_layout(group_sizes: &[usize], available_width: u16) -> FlowLayout {
    let mut cards = Vec::with_capacity(group_sizes.len());
    let mut row_groups: Vec<Vec<usize>> = Vec::new();
    let mut current_x: u16 = 1; // left margin
    let mut current_row: usize = 0;

    for (i, &size) in group_sizes.iter().enumerate() {
        let card_w = natural_card_width(size);
        let needed = card_w + 1; // card + right gap

        if current_x > 1 && current_x + needed > available_width {
            current_row += 1;
            current_x = 1;
        }

        cards.push(CardLayout { row: current_row, x: current_x, width: card_w });

        if current_row >= row_groups.len() {
            row_groups.push(Vec::new());
        }
        row_groups[current_row].push(i);

        current_x += card_w + 1;
    }

    FlowLayout { cards, row_groups }
}

/// Hit-test: given a mouse position, return the flat session index clicked (if any).
pub fn session_at_position(
    group_sizes: &[usize],
    content_area: Rect,
    mouse_row: u16,
    mouse_col: u16,
) -> Option<usize> {
    if group_sizes.is_empty() { return None; }

    let layout = flow_layout(group_sizes, content_area.width);
    let creature_render_h = CREATURE_HEIGHT.div_ceil(2);
    let card_inner_height = 1 + 1 + creature_render_h + 1 + 1 + 1;
    let card_height = card_inner_height + 2;
    let row_stride = card_height + 1;

    let mut flat_pos = 0usize;
    for (group_idx, &size) in group_sizes.iter().enumerate() {
        let card_pos = &layout.cards[group_idx];
        let card_x = content_area.x + card_pos.x;
        let card_y = content_area.y + 1 + card_pos.row as u16 * row_stride;
        let card_width = card_pos.width;

        if mouse_row < card_y || mouse_row >= card_y + card_height
            || mouse_col < card_x || mouse_col >= card_x + card_width
        {
            flat_pos += size;
            continue;
        }

        // Inside this card — determine which creature column
        let inner_x = card_x + 2; // border(1) + padding(1)
        let creature_spacing = CREATURE_WIDTH + 2;
        for local_idx in 0..size {
            let cx = inner_x + local_idx as u16 * creature_spacing;
            if mouse_col >= cx && mouse_col < cx + CREATURE_WIDTH {
                return Some(flat_pos + local_idx);
            }
        }
        // Clicked inside card but not on a creature column — select nearest
        return Some(flat_pos);
    }
    None
}

fn fill_background(area: Rect, buf: &mut Buffer) {
    let bg = Style::default().bg(Color::Rgb(20, 20, 30));
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut(Position { x, y }) {
                cell.set_symbol(" ");
                cell.set_style(bg);
            }
        }
    }
}

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

/// Draw a box around an area using box-drawing characters
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

fn draw_usage_bar(global_stats: &GlobalStats, area: Rect, buf: &mut Buffer) {
    let bg_style = Style::default().bg(Color::Rgb(25, 25, 35));
    for x in area.x..area.x + area.width {
        if let Some(cell) = buf.cell_mut(Position { x, y: area.y }) {
            cell.set_symbol(" ");
            cell.set_style(bg_style);
        }
    }

    let bg = Color::Rgb(25, 25, 35);
    let sep = " \u{2502} ";
    let sep_style = Style::default().fg(Color::Rgb(85, 85, 85)).bg(bg);

    let mut x = area.x + 1;

    // 📨 N msgs — emoji is 2 cells wide, draw separately then text
    draw_text(x, area.y, "\u{1f4e8}", Style::default().fg(Color::Rgb(120, 120, 140)).bg(bg), area, buf);
    x += 2; // emoji width
    let msgs = format!(" {} msgs", global_stats.daily_messages);
    draw_text(x, area.y, &msgs, Style::default().fg(Color::Rgb(120, 120, 140)).bg(bg), area, buf);
    x += msgs.len() as u16;

    draw_text(x, area.y, sep, sep_style, area, buf);
    x += sep.len() as u16;

    // ✨ Nk tokens
    draw_text(x, area.y, "\u{2728}", Style::default().fg(Color::Rgb(255, 213, 79)).bg(bg), area, buf);
    x += 2;
    let tok = format!(" {}", format_tokens_compact(global_stats.daily_tokens));
    draw_text(x, area.y, &tok, Style::default().fg(Color::Rgb(255, 213, 79)).bg(bg), area, buf);
    x += tok.len() as u16;

    draw_text(x, area.y, sep, sep_style, area, buf);
    x += sep.len() as u16;

    // ⏳ N% resets Xh Ym
    draw_text(x, area.y, "\u{23f3}", Style::default().fg(Color::Rgb(79, 195, 247)).bg(bg), area, buf);
    x += 2;
    let five_hr = match global_stats.five_hour_pct {
        Some(pct) => {
            let reset = format_reset_countdown(global_stats.five_hour_resets_at);
            format!(" {}% {}", pct, reset)
        }
        None => " ---".to_string(),
    };
    let five_color = pct_color(global_stats.five_hour_pct, Color::Rgb(79, 195, 247));
    draw_text(x, area.y, &five_hr, Style::default().fg(five_color).bg(bg), area, buf);
    x += five_hr.len() as u16;

    draw_text(x, area.y, sep, sep_style, area, buf);
    x += sep.len() as u16;

    // 📅 N% resets Day
    draw_text(x, area.y, "\u{1f4c5}", Style::default().fg(Color::Rgb(129, 199, 132)).bg(bg), area, buf);
    x += 2;
    let seven_day = match global_stats.seven_day_pct {
        Some(pct) => {
            let reset = format_reset_countdown(global_stats.seven_day_resets_at);
            format!(" {}% {}", pct, reset)
        }
        None => " ---".to_string(),
    };
    let seven_color = pct_color(global_stats.seven_day_pct, Color::Rgb(129, 199, 132));
    draw_text(x, area.y, &seven_day, Style::default().fg(seven_color).bg(bg), area, buf);
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
        match chrono::DateTime::from_timestamp(ts, 0) {
            Some(d) => format!("resets {}", d.format("%a")),
            None => format!("resets {}h", hours),
        }
    } else if hours > 0 {
        format!("resets {}h {}m", hours, minutes)
    } else {
        format!("resets {}m", minutes)
    }
}

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
