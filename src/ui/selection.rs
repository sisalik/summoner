/// In-app text selection for PTY session views.
/// Positions use an absolute coordinate system where:
///   abs_row = viewport_row - scrollback_offset
/// This keeps selection stable as the user scrolls — the same content
/// always maps to the same abs_row regardless of current scrollback.

#[derive(Debug, Clone)]
pub struct Selection {
    pub anchor: (isize, u16),
    pub moving: (isize, u16),
}

impl Selection {
    pub fn new(abs_row: isize, col: u16) -> Self {
        Self {
            anchor: (abs_row, col),
            moving: (abs_row, col),
        }
    }

    /// Returns (start, end) in reading order.
    pub fn normalised(&self) -> ((isize, u16), (isize, u16)) {
        if self.anchor.0 < self.moving.0
            || (self.anchor.0 == self.moving.0 && self.anchor.1 <= self.moving.1)
        {
            (self.anchor, self.moving)
        } else {
            (self.moving, self.anchor)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.moving
    }

    /// Whether the cell at (abs_row, col) falls within the selection.
    pub fn contains(&self, abs_row: isize, col: u16) -> bool {
        let ((sr, sc), (er, ec)) = self.normalised();
        if abs_row < sr || abs_row > er {
            return false;
        }
        if sr == er {
            return col >= sc && col <= ec;
        }
        if abs_row == sr {
            return col >= sc;
        }
        if abs_row == er {
            return col <= ec;
        }
        true
    }
}

/// Convert mouse terminal coordinates to absolute buffer position.
pub fn mouse_to_abs(
    mouse_row: u16,
    mouse_col: u16,
    area_y: u16,
    area_x: u16,
    area_height: u16,
    area_width: u16,
    scrollback: usize,
) -> (isize, u16) {
    let vrow = mouse_row.saturating_sub(area_y).min(area_height.saturating_sub(1));
    let vcol = mouse_col.saturating_sub(area_x).min(area_width.saturating_sub(1));
    (vrow as isize - scrollback as isize, vcol)
}

/// Extract selected text from a vt100 screen.
/// Temporarily adjusts scrollback so the selection range is visible,
/// extracts via `contents_between`, then restores the original scrollback.
pub fn extract_text(
    screen: &mut vt100::Screen,
    selection: &Selection,
) -> String {
    let ((sr, sc), (er, ec)) = selection.normalised();
    let (rows, _) = screen.size();
    let original_scrollback = screen.scrollback();

    // Set scrollback so that the start of selection maps to visible row 0:
    //   visible_row = abs_row + scrollback  →  0 = sr + sb  →  sb = -sr
    let needed_sb = if sr < 0 { (-sr) as usize } else { 0 };
    screen.set_scrollback(needed_sb);

    let start_vrow = (sr + needed_sb as isize) as u16;
    let end_vrow = ((er + needed_sb as isize) as u16).min(rows.saturating_sub(1));

    let text = screen.contents_between(start_vrow, sc, end_vrow, ec + 1);
    screen.set_scrollback(original_scrollback);
    text
}

/// Copy text to the system clipboard via the OSC 52 escape sequence.
pub fn copy_to_clipboard(text: &str) {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let osc = format!("\x1b]52;c;{}\x07", encoded);
    let _ = std::io::Write::write_all(&mut std::io::stdout(), osc.as_bytes());
}
