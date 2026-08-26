/// In-app text selection for PTY session views.
///
/// Positions use vt100 stream rows: the row drawn at viewport row `v` is
/// `screen.scrolled_lines() - screen.scrollback() + v`. Because
/// `scrolled_lines` counts every row that has ever scrolled off the top,
/// a given piece of content keeps the same stream row forever — the
/// selection stays glued to its text both while the user scrolls and while
/// new output streams in underneath it.
use ratatui::layout::Rect;

/// How a selection turns screen cells into text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    /// Join soft-wrapped lines, strip Claude Code gutters, dedent.
    Smart,
    /// Copy the cells verbatim.
    Raw,
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub anchor: (u64, u16),
    pub moving: (u64, u16),
    pub dragged: bool,
    pub mode: SelectionMode,
}

impl Selection {
    pub fn new(stream_row: u64, col: u16, mode: SelectionMode) -> Self {
        Self {
            anchor: (stream_row, col),
            moving: (stream_row, col),
            dragged: false,
            mode,
        }
    }

    /// Returns (start, end) in reading order.
    pub fn normalised(&self) -> ((u64, u16), (u64, u16)) {
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
}

/// Convert mouse terminal coordinates to a stream-row position.
pub fn mouse_to_stream(
    mouse_row: u16,
    mouse_col: u16,
    area: Rect,
    screen: &vt100::Screen,
) -> (u64, u16) {
    let vrow = mouse_row
        .saturating_sub(area.y)
        .min(area.height.saturating_sub(1));
    let vcol = mouse_col
        .saturating_sub(area.x)
        .min(area.width.saturating_sub(1));
    (viewport_top(screen) + u64::from(vrow), vcol)
}

/// Stream row of the topmost row currently drawn in the viewport.
pub fn viewport_top(screen: &vt100::Screen) -> u64 {
    screen.scrolled_lines() - screen.scrollback() as u64
}

/// Select the word under (stream_row, col) by scanning for word boundaries.
///
/// A "word" is a contiguous run of non-whitespace, non-punctuation
/// characters, except that `_` counts as a word character so `snake_case`
/// and `SCREAMING_SNAKE_CASE` identifiers select as one word.
pub fn select_word(screen: &vt100::Screen, stream_row: u64, col: u16) -> Selection {
    let (_, cols) = screen.size();
    let point = Selection {
        anchor: (stream_row, col),
        moving: (stream_row, col),
        dragged: true,
        mode: SelectionMode::Raw,
    };

    if screen.stream_row_wrapped(stream_row).is_none() {
        return point;
    }

    let line: Vec<char> = (0..cols)
        .map(|c| {
            screen
                .stream_cell(stream_row, c)
                .map_or(' ', |cell| cell.contents().chars().next().unwrap_or(' '))
        })
        .collect();

    let col_idx = (col as usize).min(line.len().saturating_sub(1));
    if line.is_empty() || !is_word_char(line[col_idx]) {
        return point;
    }

    let mut start = col_idx;
    while start > 0 && is_word_char(line[start - 1]) {
        start -= 1;
    }
    let mut end = col_idx;
    while end + 1 < line.len() && is_word_char(line[end + 1]) {
        end += 1;
    }

    Selection {
        anchor: (stream_row, start as u16),
        moving: (stream_row, end as u16),
        dragged: true,
        mode: SelectionMode::Raw,
    }
}

fn is_word_char(ch: char) -> bool {
    ch == '_' || (!ch.is_ascii_whitespace() && !ch.is_ascii_punctuation())
}

/// Select the entire line at `stream_row`.
pub fn select_line(stream_row: u64, cols: u16) -> Selection {
    Selection {
        anchor: (stream_row, 0),
        moving: (stream_row, cols.saturating_sub(1)),
        dragged: true,
        mode: SelectionMode::Smart,
    }
}

/// Copy text to the system clipboard via the OSC 52 escape sequence.
///
/// Terminated with ST rather than BEL, and flushed immediately: this is the
/// only escape Summoner writes to the host terminal outside Ratatui's backend,
/// and a half-written OSC leaves the host swallowing everything drawn after it
/// as string data.
pub fn copy_to_clipboard(text: &str) {
    use base64::Engine;
    if text.is_empty() {
        return;
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let osc = format!("\x1b]52;c;{}\x1b\\", encoded);
    let mut out = std::io::stdout();
    let _ = std::io::Write::write_all(&mut out, osc.as_bytes());
    let _ = std::io::Write::flush(&mut out);
}
