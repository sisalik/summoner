// Dashboard grid — grouped by project, with selection boxes

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthChar;

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
const SCROLLBAR_WIDTH: u16 = 1;
const BACKGROUND: Color = Color::Rgb(20, 20, 30);

/// Width available to card flow after reserving the scrollbar column.
pub fn layout_width(content_width: u16) -> u16 {
    content_width.saturating_sub(SCROLLBAR_WIDTH)
}

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

pub struct Dashboard<'a> {
    sessions: &'a [Session],
    session_stats: &'a [SessionStats],
    global_stats: &'a GlobalStats,
    rasters: &'a [RasterResult],
    nav: &'a mut DashboardNav,
    git_cache: &'a mut GitDiffCache,
    reordering: bool,
    /// A startup problem to show under the usage bar, if any.
    notice: Option<&'a str>,
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
        Self { sessions, session_stats, global_stats, rasters, nav, git_cache, reordering, notice: None }
    }

    /// Show a startup problem on a row under the usage bar.
    pub fn with_notice(mut self, notice: Option<&'a str>) -> Self {
        self.notice = notice;
        self
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
            .bg(BACKGROUND);
        draw_text(area.x, hint_y, hint_text, hint_style, area, buf);

        // Notice row under the usage bar, only when there is something to say
        let notice_rows = match self.notice {
            Some(notice) if area.height >= 4 => {
                let style = Style::default()
                    .fg(Color::Rgb(230, 180, 80))
                    .bg(BACKGROUND);
                let text = format!(" \u{26a0} {} ", notice);
                draw_text(area.x, area.y + 1, &text, style, area, buf);
                1
            }
            _ => 0,
        };

        // Content area (between top usage bar and bottom hints). Cards flow in
        // `flow_area` (one column narrower) so the last column is reserved for the
        // scrollbar track.
        let content_area = Rect {
            x: area.x,
            y: area.y + 1 + notice_rows,
            width: area.width,
            height: area.height.saturating_sub(2 + notice_rows),
        };
        let flow_area = Rect {
            x: content_area.x,
            y: content_area.y,
            width: layout_width(content_area.width),
            height: content_area.height,
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
        let layout = flow_layout(&group_sizes, flow_area.width);
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
        let scroll_row = self.nav.scroll_row();

        if sc.full_rows == 0 {
            return;
        }

        // Virtual scratch origin: scratch y=`row_stride` maps to absolute y=content_area.y.
        // Gives `row_stride` cells of margin above and below content_area so peek cards
        // (which sit at absolute y that can be less than content_area.y) stay in non-negative
        // scratch coordinates.
        let scratch_height = content_area.height + 2 * row_stride;
        let scratch_area = Rect {
            x: content_area.x,
            y: 0,
            width: content_area.width,
            height: scratch_height,
        };
        let mut scratch = Buffer::filled(scratch_area, background_cell());

        // scratch_y = abs_y - content_area.y + row_stride
        let scratch_clip = Rect {
            x: content_area.x,
            y: row_stride,
            width: content_area.width,
            height: content_area.height,
        };
        let full_band_scratch_y = row_stride + sc.top_peek;

        let first_row = if sc.top_peek > 0 && scroll_row > 0 { scroll_row - 1 } else { scroll_row };
        let last_row = {
            let mut end = scroll_row + sc.full_rows;
            if sc.bottom_peek > 0 && end < total_rows { end += 1; }
            end
        };

        for row_idx in first_row..last_row.min(total_rows) {
            let card_scratch_y = if row_idx < scroll_row {
                full_band_scratch_y.saturating_sub(row_stride)
            } else {
                full_band_scratch_y + (row_idx - scroll_row) as u16 * row_stride
            };

            for &group_idx in &layout.row_groups[row_idx] {
                let card_pos = &layout.cards[group_idx];
                let card_x = content_area.x + card_pos.x;
                let card_width = card_pos.width;

                let card_area = Rect {
                    x: card_x,
                    y: card_scratch_y,
                    width: card_width,
                    height: card_height,
                };

                let flat_pos_start: usize = group_sizes.iter().take(group_idx).sum();

                self.render_card_into(
                    group_idx,
                    card_area,
                    scratch_clip,
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

        // Copy scratch[row_stride..row_stride+content_area.height] rows to buf[content_area.y..].
        let copy_area = content_area.intersection(buf.area);
        let width = copy_area.width as usize;
        for y in copy_area.top()..copy_area.bottom() {
            let src = scratch.index_of(copy_area.x, y - content_area.y + row_stride);
            let dst = buf.index_of(copy_area.x, y);
            buf.content[dst..dst + width].clone_from_slice(&scratch.content[src..src + width]);
        }

        draw_scrollbar(content_area, total_rows, sc.full_rows, scroll_row, buf);
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

        let selected_local = selected
            .checked_sub(flat_pos_start)
            .filter(|&local| local < group.sessions.len());
        let (first, count) = visible_creatures(card_area.width, group.sessions.len(), selected_local);
        // A dashed side border marks each side with creatures scrolled out of
        // view; unlike a label it survives a short window cutting the card off
        let border_style = Style::default().fg(border_color);
        if first > 0 {
            dash_side_border(card_area.x, card_area, clip, border_style, buf);
        }
        if first + count < group.sessions.len() {
            dash_side_border(card_area.right().saturating_sub(1), card_area, clip, border_style, buf);
        }

        // Creatures are laid out at full width and clipped at the card's inner
        // edge, so the one after the last that fits shows partly as a hint
        for (slot, (local_idx, &sess_idx)) in group.sessions.iter().enumerate().skip(first).enumerate() {
            let session = &self.sessions[sess_idx];

            let creature_spacing = CREATURE_WIDTH + 2;
            let cx = inner.x + slot as u16 * creature_spacing;
            let inner_right = inner.x + inner.width;
            // Visible width; equals CREATURE_WIDTH except for the clipped one
            let cw = CREATURE_WIDTH.min(inner_right.saturating_sub(cx));

            if cw == 0 || cx >= inner_right {
                break;
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
                    || session.state == SessionState::Errored
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
                let bar_total = CREATURE_WIDTH.saturating_sub(pct_str.len() as u16) as usize;
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
                width: CREATURE_WIDTH,
                height: creature_render_h,
            };

            let is_claude_sprite = session.claude_conversation_id.is_some()
                || session.state == SessionState::Working
                || session.state == SessionState::Waiting
                || session.state == SessionState::Errored
                || session.state == SessionState::Idle;

            if is_claude_sprite {
                if let Some(raster) = self.rasters.get(sess_idx) {
                    let palette = state_palette(session.state);
                    render_sprite_shaded_clipped(&raster.sprite, &raster.capsule_ids, &palette, creature_area, col_clip, buf);
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
                render_sprite_to_buffer_clipped(&icon, &palette, centered_area, col_clip, buf);
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
                let xp_x = cx + CREATURE_WIDTH.saturating_sub(xp_len);
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

            if local_idx >= first + count {
                let inner_rows = Rect { y: inner.y, height: inner.height, ..creature_col };
                fade_area(rect_intersect(inner_rows, clip), buf);
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
        // Shorter than one card: show the top of the current row's cards
        return ScrollLayout { full_rows: 1, top_peek: 0, bottom_peek: 0 };
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

/// Which of a card's creatures are shown in full, as `(first, count)`. A card
/// too narrow for all of its creatures shows as many as fit, scrolled so the
/// selected creature (`selected_local`, when it is in this card) stays visible;
/// the next one is drawn clipped at the card's edge.
pub fn visible_creatures(card_width: u16, session_count: usize, selected_local: Option<usize>) -> (usize, usize) {
    // Inner width is the card minus border and padding; the last creature
    // needs no gap after it.
    let fit = ((card_width.saturating_sub(4) + 2) / (CREATURE_WIDTH + 2)).max(1) as usize;
    let count = fit.min(session_count);
    let first = selected_local.map_or(0, |sel| (sel + 1).saturating_sub(count));
    (first, count)
}

/// Flow layout: each card gets its natural width, capped to the available
/// width, packed left-to-right with row wrapping.
pub fn flow_layout(group_sizes: &[usize], available_width: u16) -> FlowLayout {
    let mut cards = Vec::with_capacity(group_sizes.len());
    let mut row_groups: Vec<Vec<usize>> = Vec::new();
    let mut current_x: u16 = 1; // left margin
    let mut current_row: usize = 0;

    for (i, &size) in group_sizes.iter().enumerate() {
        // Leave room for the left margin and the right gap
        let card_w = natural_card_width(size).min(available_width.saturating_sub(2));
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
/// `selected` is needed because it decides how a narrow card's creatures are scrolled.
pub fn session_at_position(
    group_sizes: &[usize],
    content_area: Rect,
    scroll_row: usize,
    selected: usize,
    mouse_row: u16,
    mouse_col: u16,
) -> Option<usize> {
    if group_sizes.is_empty() { return None; }

    let (card_height, row_stride) = card_metrics();

    let layout = flow_layout(group_sizes, layout_width(content_area.width));
    let total_rows = layout.row_groups.len();
    let sc = scroll_layout(content_area.height, row_stride, total_rows, scroll_row);
    let full_band_y = content_area.y + sc.top_peek;
    let content_end = content_area.y + content_area.height;

    // (row_idx, visible_y_start, visible_y_end) — half-open intervals in absolute buf coords.
    let candidate_rows: Vec<(usize, u16, u16)> = {
        let mut v = Vec::new();
        if sc.top_peek > 0 && scroll_row > 0 {
            v.push((scroll_row - 1, content_area.y, full_band_y));
        }
        for r in 0..sc.full_rows {
            let idx = scroll_row + r;
            if idx >= total_rows { break; }
            let card_y = full_band_y + r as u16 * row_stride;
            v.push((idx, card_y, (card_y + card_height).min(content_end)));
        }
        if sc.bottom_peek > 0 && scroll_row + sc.full_rows < total_rows {
            let idx = scroll_row + sc.full_rows;
            let card_y = full_band_y + sc.full_rows as u16 * row_stride;
            v.push((idx, card_y, content_end));
        }
        v
    };

    for (row_idx, y_start, y_end) in candidate_rows {
        if mouse_row < y_start || mouse_row >= y_end { continue; }
        for &group_idx in &layout.row_groups[row_idx] {
            let card_pos = &layout.cards[group_idx];
            let card_x = content_area.x + card_pos.x;
            let card_width = card_pos.width;
            if mouse_col < card_x || mouse_col >= card_x + card_width { continue; }

            let inner_x = card_x + 2;
            let creature_spacing = CREATURE_WIDTH + 2;
            let flat_pos_start: usize = group_sizes.iter().take(group_idx).sum();
            let size = group_sizes[group_idx];
            let selected_local = selected.checked_sub(flat_pos_start).filter(|&local| local < size);
            let (first, _) = visible_creatures(card_width, size, selected_local);
            // Includes the creature clipped at the card's inner edge
            let inner_right = (card_x + card_width).saturating_sub(2);
            for slot in 0..size - first {
                let cx = inner_x + slot as u16 * creature_spacing;
                if cx >= inner_right { break; }
                if mouse_col >= cx && mouse_col < (cx + CREATURE_WIDTH).min(inner_right) {
                    return Some(flat_pos_start + first + slot);
                }
            }
            return Some(flat_pos_start);
        }
    }
    None
}

/// Draw a 1-column scrollbar on the right edge of `area`. No-op if the content
/// fits entirely (`total_rows <= visible_rows`) or the track has zero height.
fn draw_scrollbar(
    area: Rect,
    total_rows: usize,
    visible_rows: usize,
    scroll_row: usize,
    buf: &mut Buffer,
) {
    if area.height == 0 || area.width == 0 { return; }
    if visible_rows == 0 || total_rows <= visible_rows { return; }

    let track_x = area.x + area.width - 1;
    let track_h = area.height;
    let bg = BACKGROUND;
    let track_style = Style::default().fg(Color::Rgb(35, 35, 48)).bg(bg);
    let thumb_style = Style::default().fg(Color::Rgb(70, 70, 90)).bg(bg);

    for y in area.y..area.y + track_h {
        if let Some(cell) = buf.cell_mut(Position { x: track_x, y }) {
            cell.set_symbol("\u{2502}");
            cell.set_style(track_style);
        }
    }

    let track_h_usize = track_h as usize;
    let thumb_h = ((track_h_usize * visible_rows) / total_rows).max(1).min(track_h_usize) as u16;
    let max_scroll = total_rows - visible_rows;
    let thumb_off = if max_scroll == 0 {
        0u16
    } else {
        (((track_h_usize - thumb_h as usize) * scroll_row) / max_scroll) as u16
    };

    for y in (area.y + thumb_off)..(area.y + thumb_off + thumb_h) {
        if let Some(cell) = buf.cell_mut(Position { x: track_x, y }) {
            cell.set_symbol("\u{2588}");
            cell.set_style(thumb_style);
        }
    }
}

/// Redraw the side border at column `x` of `card` dashed, between its corners.
fn dash_side_border(x: u16, card: Rect, clip: Rect, style: Style, buf: &mut Buffer) {
    for y in card.y + 1..card.bottom().saturating_sub(1) {
        draw_text(x, y, "\u{254e}", style, clip, buf);
    }
}

/// Blend every colour in `area` halfway into the background, so a creature
/// cut off at a card's edge reads as offstage.
fn fade_area(area: Rect, buf: &mut Buffer) {
    let fade = |color: Color| match (color, BACKGROUND) {
        (Color::Rgb(r, g, b), Color::Rgb(br, bg, bb)) => {
            let mix = |c: u8, base: u8| ((c as u16 + base as u16) / 2) as u8;
            Color::Rgb(mix(r, br), mix(g, bg), mix(b, bb))
        }
        (other, _) => other,
    };
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            cell.fg = fade(cell.fg);
            cell.bg = fade(cell.bg);
        }
    }
}

fn background_cell() -> Cell {
    let mut cell = Cell::default();
    cell.set_bg(BACKGROUND);
    cell
}

/// Blank `area` to the dashboard background. Fills whole row slices, since
/// this touches every cell of every dashboard frame.
fn fill_background(area: Rect, buf: &mut Buffer) {
    let area = area.intersection(buf.area);
    let blank = background_cell();
    for y in area.top()..area.bottom() {
        let start = buf.index_of(area.x, y);
        buf.content[start..start + area.width as usize].fill(blank.clone());
    }
}

fn draw_text(x: u16, y: u16, text: &str, style: Style, clip: Rect, buf: &mut Buffer) {
    if y < clip.y || y >= clip.y.saturating_add(clip.height) { return; }
    for (i, ch) in text.chars().enumerate() {
        let px = x + i as u16;
        if px < clip.x { continue; }
        // A wide glyph in the last column would spill one column past the clip
        let width = UnicodeWidthChar::width(ch).unwrap_or(1).max(1) as u16;
        if px + width > clip.x.saturating_add(clip.width) { break; }
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
    fn shorter_than_a_card_shows_one_clipped_row() {
        let s = scroll_layout(5, 17, 3, 0);
        assert_eq!(s.full_rows, 1);
        assert_eq!(s.top_peek, 0);
        assert_eq!(s.bottom_peek, 0);
    }

    #[test]
    fn narrow_card_is_capped_to_the_available_width() {
        // Three creatures want 64 columns
        let layout = flow_layout(&[3], 43);
        assert_eq!(layout.cards[0].width, 41);
        assert_eq!(layout.row_groups, vec![vec![0]]);
    }

    #[test]
    fn wide_card_keeps_its_natural_width() {
        let layout = flow_layout(&[3], 120);
        assert_eq!(layout.cards[0].width, natural_card_width(3));
    }

    #[test]
    fn natural_width_card_shows_every_creature() {
        assert_eq!(visible_creatures(natural_card_width(3), 3, None), (0, 3));
        assert_eq!(visible_creatures(natural_card_width(3), 3, Some(2)), (0, 3));
    }

    #[test]
    fn narrow_card_scrolls_to_the_selected_creature() {
        // 41 columns fit one creature
        assert_eq!(visible_creatures(41, 3, None), (0, 1));
        assert_eq!(visible_creatures(41, 3, Some(0)), (0, 1));
        assert_eq!(visible_creatures(41, 3, Some(2)), (2, 1));
        // 44 columns fit two
        assert_eq!(visible_creatures(44, 3, Some(2)), (1, 2));
        assert_eq!(visible_creatures(44, 3, Some(1)), (0, 2));
    }

    #[test]
    fn clicking_a_clipped_creature_selects_it() {
        // At 44 columns a three-creature card is 41 wide: creature 0 is whole
        // and creature 1 starts at column 23, clipped at the inner edge (40)
        let content = Rect::new(0, 1, 44, 22);
        assert_eq!(session_at_position(&[3], content, 0, 0, 10, 30), Some(1));
        // The padding past the inner edge belongs to the card, not a creature
        assert_eq!(session_at_position(&[3], content, 0, 0, 10, 40), Some(0));
    }
}

#[cfg(test)]
mod narrow_card_tests {
    use super::*;
    use crate::creature::{locomotion::LocomotionState, outline::rasterize_skeleton, skeleton::Skeleton};

    /// Three shell sessions in one project, drawn at `width` x 24 with
    /// creature `selected` selected.
    fn render_three_shells(width: u16, selected: usize) -> Buffer {
        let sessions: Vec<Session> = (0..3).map(|i| {
            let mut s = Session::new("/tmp/narrow-card".into(), i, "bipedal".into());
            s.state = SessionState::ShellOnly;
            s
        }).collect();
        let stats: Vec<SessionStats> = sessions.iter().map(|_| SessionStats::new()).collect();
        let sprites: Vec<_> = (0..3).map(|i| {
            rasterize_skeleton(LocomotionState::new(Skeleton::instantiate(i, 42), SessionState::ShellOnly).skeleton())
        }).collect();
        let mut nav = DashboardNav::new();
        nav.update_layout_with_rows(&[3], flow_layout(&[3], layout_width(width)).row_groups);
        nav.set_selected(selected);
        let mut git = GitDiffCache::new(3600);
        let area = Rect::new(0, 0, width, 24);
        let mut buf = Buffer::empty(area);
        Dashboard::new(&sessions, &stats, &GlobalStats::new(), &sprites, &mut nav, &mut git, false)
            .render(area, &mut buf);
        buf
    }

    fn fg_colours(buf: &Buffer, xs: std::ops::Range<u16>) -> Vec<Color> {
        xs.flat_map(|x| (2..19).map(move |y| (x, y))).map(|p| buf[p].fg).collect()
    }

    // At 60 columns the card is 57 wide (columns 1 to 57): creatures start at
    // columns 3 and 23, and the third starts at 43, cut off at the inner edge.
    #[test]
    fn hidden_creatures_dash_their_side_of_the_border() {
        let first = render_three_shells(60, 0);
        assert_eq!(first[(1, 10)].symbol(), "\u{2502}");
        assert_eq!(first[(57, 10)].symbol(), "\u{254e}");

        let last = render_three_shells(60, 2);
        assert_eq!(last[(1, 10)].symbol(), "\u{254e}");
        assert_eq!(last[(57, 10)].symbol(), "\u{2502}");
    }

    #[test]
    fn cut_off_creature_is_faded() {
        let buf = render_three_shells(60, 0);
        let label = SessionState::ShellOnly.color();
        assert!(fg_colours(&buf, 3..21).contains(&label));
        let clipped = fg_colours(&buf, 43..56);
        assert!(!clipped.contains(&label));
        assert!(clipped.contains(&Color::Rgb(110, 110, 120)));
    }

    #[test]
    fn wide_card_has_solid_borders_and_no_fading() {
        let buf = render_three_shells(80, 0);
        assert_eq!(buf[(1, 10)].symbol(), "\u{2502}");
        assert_eq!(buf[(64, 10)].symbol(), "\u{2502}");
        assert!(fg_colours(&buf, 43..61).contains(&SessionState::ShellOnly.color()));
    }
}
