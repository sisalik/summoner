// Dashboard grid — grouped by project, with selection boxes

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::creature::outline::RasterResult;
use crate::creature::render::{render_sprite_shaded, render_sprite_to_buffer, state_palette, terminal_icon_sprite};
use crate::session::{group_by_project, Session, SessionState, SessionStats, GlobalStats};
use crate::git::GitDiffCache;
use crate::stats;
use crate::ui::dashboard_nav::DashboardNav;

const CREATURE_WIDTH: u16 = 18;
const CREATURE_HEIGHT: u16 = 24;

pub struct Dashboard<'a> {
    sessions: &'a [Session],
    session_stats: &'a [SessionStats],
    global_stats: &'a GlobalStats,
    rasters: &'a [RasterResult],
    nav: &'a DashboardNav,
    git_cache: &'a mut GitDiffCache,
}

impl<'a> Dashboard<'a> {
    pub fn new(
        sessions: &'a [Session],
        session_stats: &'a [SessionStats],
        global_stats: &'a GlobalStats,
        rasters: &'a [RasterResult],
        nav: &'a DashboardNav,
        git_cache: &'a mut GitDiffCache,
    ) -> Self {
        Self { sessions, session_stats, global_stats, rasters, nav, git_cache }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        fill_background(area, buf);

        if area.height < 3 { return; }

        // Usage bar at top
        let usage_y = area.y;
        draw_usage_bar(self.global_stats, Rect { x: area.x, y: usage_y, width: area.width, height: 1 }, buf);

        // Hint row at bottom
        let hint_y = area.y + area.height.saturating_sub(1);
        let hint_text = " \u{2190}\u{2192}\u{2191}\u{2193} navigate \u{2502} Enter open \u{2502} n/N new session/dir \u{2502} x/X close session/project ";
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

        // Group sessions by project
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

        let max_creatures = groups.iter().map(|g| g.sessions.len()).max().unwrap_or(1);
        let (cols, _rows) = grid_layout(groups.len(), content_area.width, max_creatures);
        if cols == 0 { return; }

        // Determine which group names need disambiguation (same basename, different path)
        let basenames: Vec<&str> = groups.iter().map(|g| {
            std::path::Path::new(&g.directory)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
        }).collect();
        let needs_full_path: Vec<bool> = basenames.iter().enumerate().map(|(i, name)| {
            basenames.iter().enumerate().any(|(j, other)| i != j && name == other)
        }).collect();

        // Creature render height in terminal rows = CREATURE_HEIGHT / 2 (half-block)
        let creature_render_h = CREATURE_HEIGHT.div_ceil(2);

        // Card dimensions: each group gets its own card slot
        let total_margin_x = (cols as u16) + 1;
        let card_width = content_area.width.saturating_sub(total_margin_x) / cols as u16;

        // card inner height: padding(1) + health_bar(1) + creature(creature_render_h) + lvl_xp(1) + state(1) + padding(1)
        let card_inner_height = 1 + 1 + creature_render_h + 1 + 1 + 1;
        let card_height = card_inner_height + 2; // + border top/bottom
        let row_stride = card_height + 1;

        let selected = self.nav.selected();

        // Build flat position mapping: position 0, 1, 2... across all groups
        let mut flat_pos = 0usize;

        for (group_idx, group) in groups.iter().enumerate() {
            let col = group_idx % cols;
            let row = group_idx / cols;

            let card_x = content_area.x + 1 + col as u16 * (card_width + 1);
            let card_y = content_area.y + 1 + row as u16 * row_stride;

            if card_y + card_height > content_area.y + content_area.height {
                break;
            }

            let card_area = Rect {
                x: card_x,
                y: card_y,
                width: card_width,
                height: card_height,
            };

            let border_color = Color::Rgb(60, 60, 80);

            let display_name = if needs_full_path[group_idx] {
                crate::app::display_path(&group.directory)
            } else {
                basenames[group_idx].to_string()
            };
            let title_style = Style::default().fg(Color::Rgb(140, 140, 160));

            // Draw card border (no title — we'll draw it manually)
            let block = ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .padding(ratatui::widgets::Padding::uniform(1));

            let inner = block.inner(card_area);
            block.render(card_area, buf);

            // Draw title with git diff stats
            let (git_adds, git_dels) = self.git_cache.get(&group.directory);
            let diff_str = crate::git::format_diff_compact(git_adds, git_dels);

            let title_x = card_area.x + 2;
            let title_y = card_area.y;
            let name_text = format!(" {} ", display_name);
            draw_text(title_x, title_y, &name_text, title_style, card_area, buf);

            if !diff_str.is_empty() {
                let diff_x = title_x + name_text.len() as u16;
                // Color-code: green for +additions, red for -deletions
                let mut x = diff_x;
                let mut color = Color::Rgb(129, 199, 132); // start green
                for ch in diff_str.chars() {
                    if ch == '-' && x > diff_x {
                        color = Color::Rgb(229, 115, 115); // switch to red
                    }
                    draw_text(x, title_y, &ch.to_string(), Style::default().fg(color), card_area, buf);
                    x += 1;
                }
                draw_text(x, title_y, " ", Style::default().fg(border_color), card_area, buf);
            }

            // Render all creatures in this group side by side
            for (local_idx, &sess_idx) in group.sessions.iter().enumerate() {
                let session = &self.sessions[sess_idx];

                let creature_spacing = CREATURE_WIDTH + 2;
                let cx = inner.x + local_idx as u16 * creature_spacing;
                let inner_right = inner.x + inner.width;
                let cw = CREATURE_WIDTH.min(inner_right.saturating_sub(cx));

                if cw == 0 || cx >= inner_right {
                    flat_pos += 1;
                    continue;
                }

                let creature_col = Rect {
                    x: cx,
                    y: card_area.y,
                    width: cw,
                    height: card_area.height,
                };

                let is_active_claude = session.state != SessionState::Disconnected
                    && session.state != SessionState::ShellOnly
                    && (session.claude_conversation_id.is_some()
                        || session.state == SessionState::Working
                        || session.state == SessionState::Waiting
                        || session.state == SessionState::Idle);

                // Layout from top of inner: health_bar(1), creature(creature_render_h), lvl_xp(1), state(1)
                let health_y = inner.y;
                let lvl_xp_y = inner.y + 1;
                let creature_y = inner.y + 2;
                let state_y = creature_y + creature_render_h;

                // --- Health bar at top (only for active Claude sessions) ---
                // Uses upper-half blocks ▀ for a thin bar
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
                    // "Damage" bar: green = remaining, red = used
                    let damaged = (bar_total as u64 * pct as u64 / 100).min(bar_total as u64) as usize;
                    let healthy = bar_total.saturating_sub(damaged);

                    // Draw healthy portion (green, lower-half block ▄)
                    let healthy_str: String = "\u{2580}".repeat(healthy);
                    draw_text(cx, health_y, &healthy_str, Style::default().fg(Color::Rgb(129, 199, 132)), creature_col, buf);
                    // Draw damaged portion (red)
                    let damaged_str: String = "\u{2580}".repeat(damaged);
                    draw_text(cx + healthy as u16, health_y, &damaged_str, Style::default().fg(Color::Rgb(229, 115, 115)), creature_col, buf);
                    // Draw percentage
                    let pct_color = if pct >= 90 { Color::Rgb(229, 115, 115) } else { Color::Rgb(136, 136, 136) };
                    draw_text(cx + bar_total as u16, health_y, &pct_str, Style::default().fg(pct_color), creature_col, buf);
                }

                // --- Creature sprite ---
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
                        render_sprite_shaded(&raster.sprite, &raster.capsule_ids, &palette, creature_area, buf);
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
                    render_sprite_to_buffer(&icon, &palette, centered_area, buf);
                }

                // --- Lv/XP line (only for active Claude) ---
                if is_active_claude && lvl_xp_y < card_area.y + card_area.height.saturating_sub(2) {
                    let default_stats = SessionStats::new();
                    let stat = self.session_stats.get(sess_idx).unwrap_or(&default_stats);
                    let level = stats::level_from_tokens(stat.total_tokens);
                    let xp = stats::format_xp(stat.total_tokens);

                    // Color-code level
                    let lvl_color = match level {
                        1 => Color::Rgb(140, 140, 160),   // gray
                        2 => Color::Rgb(129, 199, 132),   // green
                        3 => Color::Rgb(100, 181, 246),   // blue
                        4 => Color::Rgb(149, 117, 205),   // purple
                        5 => Color::Rgb(255, 213, 79),    // gold
                        6 => Color::Rgb(255, 152, 0),     // orange
                        7 => Color::Rgb(244, 67, 54),     // red
                        8 => Color::Rgb(233, 30, 99),     // pink
                        9 => Color::Rgb(0, 230, 230),     // cyan
                        _ => Color::Rgb(255, 255, 100),   // bright yellow (10+)
                    };

                    let lvl_text = format!("Lv.{}", level);
                    draw_text(cx, lvl_xp_y, &lvl_text, Style::default().fg(lvl_color), creature_col, buf);

                    // Right-align XP
                    let xp_len = xp.chars().count() as u16;
                    let xp_x = cx + cw.saturating_sub(xp_len);
                    draw_text(xp_x, lvl_xp_y, &xp, Style::default().fg(Color::Rgb(255, 213, 79)), creature_col, buf);
                }

                // --- State line (always shown, bottom row) ---
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
                        draw_text(cx, state_y, &activity, state_style, creature_col, buf);
                    } else {
                        let label = format!("{}  {}", session.state.icon(), session.state.label());
                        let label_style = Style::default().fg(session.state.color());
                        draw_text(cx, state_y, &label, label_style, creature_col, buf);
                    }
                }

                // --- Selection box (starts above health bar row) ---
                if flat_pos == selected {
                    let box_x = cx.saturating_sub(1).max(card_area.x + 1);
                    let box_y = health_y.saturating_sub(1).max(card_area.y + 1);
                    let box_right = (cx + cw + 1).min(card_area.x + card_area.width - 1);
                    let box_bottom = (state_y + 2).min(card_area.y + card_area.height - 1);
                    let box_w = box_right.saturating_sub(box_x);
                    let box_h = box_bottom.saturating_sub(box_y);

                    if box_w >= 3 && box_h >= 3 {
                        let box_area = Rect {
                            x: box_x,
                            y: box_y,
                            width: box_w,
                            height: box_h,
                        };
                        draw_selection_box(box_area, buf, Color::Rgb(150, 150, 255));
                    }
                }

                flat_pos += 1;
            }
        }
    }
}

/// Compute grid layout: (cols, rows) based on group count and available width.
/// `max_creatures_per_group` is the largest number of sessions in any single group.
pub fn grid_layout(count: usize, available_width: u16, max_creatures_per_group: usize) -> (usize, usize) {
    // Minimum card width: enough to fit the widest group's creatures side by side
    // Each creature needs CREATURE_WIDTH + 2 gap, plus card border(2) + padding(2)
    let creatures_w = max_creatures_per_group.max(1) as u16 * (CREATURE_WIDTH + 2);
    let min_card_width = (creatures_w + 4).max(28); // border(2) + padding(2), floor at 28

    // Max columns that fit: (width - 1 outer margin) / (card + 1 gap)
    let max_cols = ((available_width.saturating_sub(1)) / (min_card_width + 1)).max(1) as usize;

    let desired = match count {
        0 => return (0, 0),
        1 => 1,
        2 | 3 => count,
        4..=6 => 3,
        _ => 3,
    };
    let cols = desired.min(max_cols);
    let rows = count.div_ceil(cols);
    (cols, rows)
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

fn draw_text(x: u16, y: u16, text: &str, style: Style, area: Rect, buf: &mut Buffer) {
    for (i, ch) in text.chars().enumerate() {
        let px = x + i as u16;
        if px >= area.x + area.width { break; }
        if let Some(cell) = buf.cell_mut(Position { x: px, y }) {
            cell.set_symbol(&ch.to_string());
            cell.set_style(style);
        }
    }
}

/// Draw a box around an area using box-drawing characters
fn draw_selection_box(area: Rect, buf: &mut Buffer, color: Color) {
    let style = Style::default().fg(color);
    let x1 = area.x;
    let y1 = area.y;
    let x2 = area.x + area.width.saturating_sub(1);
    let y2 = area.y + area.height.saturating_sub(1);

    // Corners
    if let Some(cell) = buf.cell_mut(Position { x: x1, y: y1 }) {
        cell.set_symbol("\u{250c}"); // ┌
        cell.set_style(style);
    }
    if let Some(cell) = buf.cell_mut(Position { x: x2, y: y1 }) {
        cell.set_symbol("\u{2510}"); // ┐
        cell.set_style(style);
    }
    if let Some(cell) = buf.cell_mut(Position { x: x1, y: y2 }) {
        cell.set_symbol("\u{2514}"); // └
        cell.set_style(style);
    }
    if let Some(cell) = buf.cell_mut(Position { x: x2, y: y2 }) {
        cell.set_symbol("\u{2518}"); // ┘
        cell.set_style(style);
    }

    // Top and bottom edges
    for x in (x1 + 1)..x2 {
        if let Some(cell) = buf.cell_mut(Position { x, y: y1 }) {
            cell.set_symbol("\u{2500}"); // ─
            cell.set_style(style);
        }
        if let Some(cell) = buf.cell_mut(Position { x, y: y2 }) {
            cell.set_symbol("\u{2500}"); // ─
            cell.set_style(style);
        }
    }

    // Left and right edges
    for y in (y1 + 1)..y2 {
        if let Some(cell) = buf.cell_mut(Position { x: x1, y }) {
            cell.set_symbol("\u{2502}"); // │
            cell.set_style(style);
        }
        if let Some(cell) = buf.cell_mut(Position { x: x2, y }) {
            cell.set_symbol("\u{2502}"); // │
            cell.set_style(style);
        }
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
