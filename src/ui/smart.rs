//! Claude Code-aware "smart" selection.
//!
//! A raw selection is a rectangle of grid cells. What the user usually wants
//! is the *text*: soft-wrapped lines joined back together, Claude Code's
//! gutter chrome (`⏺ `, `│ `, `⎿ `, `> `) and box borders dropped, and the
//! block dedented. Both modes are expressed as a [`SpanSet`] — the exact
//! cells that will be copied — so the selection highlight can show what the
//! clipboard is going to get.

use std::borrow::Cow;

use super::selection::{Selection, SelectionMode};

/// Gutter markers stripped from the start of a logical line.
const GUTTER_MARKERS: [char; 4] = ['⏺', '│', '⎿', '>'];

/// Characters that make a line pure box-drawing chrome.
const BOX_CHARS: [char; 19] = [
    '─', '│', '╭', '╮', '╰', '╯', '├', '┤', '┬', '┴', '┼', '┌', '┐', '└', '┘', '═', '║', '━',
    '┃',
];

/// One physical row's worth of a selection: the cells actually copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineSpan {
    pub stream_row: u64,
    pub col_start: u16,
    /// Inclusive.
    pub col_end: u16,
}

/// The cells a selection copies, plus the text they produce.
#[derive(Debug, Clone, Default)]
pub struct SpanSet {
    /// Sorted by `stream_row`, at most one span per row.
    pub spans: Vec<LineSpan>,
    text: String,
}

impl SpanSet {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Whether the cell at (stream_row, col) is part of the copied text.
    pub fn contains(&self, stream_row: u64, col: u16) -> bool {
        self.spans
            .binary_search_by_key(&stream_row, |s| s.stream_row)
            .is_ok_and(|i| {
                let span = &self.spans[i];
                col >= span.col_start && col <= span.col_end
            })
    }
}

/// A physical row as seen by the span algorithm.
///
/// `cells` has one entry per column; `None` marks a wide-character
/// continuation column, which carries no glyph of its own. Cell text is
/// borrowed from the screen where possible — this runs every frame, so the
/// obvious `String` per cell is worth avoiding.
#[derive(Debug, Clone)]
pub struct RowIn<'a> {
    pub cells: Vec<Option<Cow<'a, str>>>,
    pub wrapped: bool,
}

impl RowIn<'static> {
    /// Build a row from plain text, one column per character.
    pub fn from_text(text: &str, wrapped: bool, cols: u16) -> Self {
        let mut cells: Vec<Option<Cow<'static, str>>> = text
            .chars()
            .map(|c| Some(Cow::Owned(c.to_string())))
            .collect();
        cells.resize(usize::from(cols), Some(Cow::Borrowed(" ")));
        Self { cells, wrapped }
    }
}

/// A span in terms of the row's index within the input slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelSpan {
    pub row: usize,
    pub col_start: u16,
    pub col_end: u16,
}

#[derive(Debug, Clone, Default)]
pub struct Computed {
    pub spans: Vec<RelSpan>,
    pub text: String,
}

/// Compute the copied cells for a selection against a live screen.
pub fn compute_spans(screen: &vt100::Screen, sel: &Selection) -> SpanSet {
    let ((sr, sc), (er, ec)) = sel.normalised();
    let (_, cols) = screen.size();

    let mut stream_rows = Vec::new();
    let mut rows = Vec::new();
    for stream_row in sr..=er {
        let Some(wrapped) = screen.stream_row_wrapped(stream_row) else {
            continue;
        };
        let mut cells: Vec<Option<Cow<'_, str>>> = screen
            .stream_row_cells(stream_row)
            .into_iter()
            .flatten()
            .take(usize::from(cols))
            .map(|cell| {
                if cell.is_wide_continuation() {
                    None
                } else if cell.has_contents() {
                    Some(Cow::Borrowed(cell.contents()))
                } else {
                    Some(Cow::Borrowed(" "))
                }
            })
            .collect();
        cells.resize(usize::from(cols), Some(Cow::Borrowed(" ")));
        stream_rows.push(stream_row);
        rows.push(RowIn { cells, wrapped });
    }

    if rows.is_empty() {
        return SpanSet::default();
    }

    // Clip only if the row the column bound belongs to is actually present.
    let first_col = if stream_rows[0] == sr { sc } else { 0 };
    let last_col = if *stream_rows.last().unwrap() == er {
        ec
    } else {
        cols.saturating_sub(1)
    };

    let computed = spans_core(&rows, first_col, last_col, sel.mode);
    SpanSet {
        spans: computed
            .spans
            .into_iter()
            .map(|s| LineSpan {
                stream_row: stream_rows[s.row],
                col_start: s.col_start,
                col_end: s.col_end,
            })
            .collect(),
        text: computed.text,
    }
}

/// Pure core of the span computation.
///
/// `first_col` clips the first row, `last_col` (inclusive) the last.
pub fn spans_core(
    rows: &[RowIn],
    first_col: u16,
    last_col: u16,
    mode: SelectionMode,
) -> Computed {
    if rows.is_empty() {
        return Computed::default();
    }
    match mode {
        SelectionMode::Raw => raw_spans(rows, first_col, last_col),
        SelectionMode::Smart => smart_spans(rows, first_col, last_col),
    }
}

/// A cell taken from the clipped selection, tagged with where it came from.
struct Taken<'a> {
    row: usize,
    col: u16,
    text: &'a str,
}

fn clipped_cells<'a>(rows: &'a [RowIn], first_col: u16, last_col: u16) -> Vec<Vec<Taken<'a>>> {
    let last_row = rows.len() - 1;
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let start = if i == 0 { usize::from(first_col) } else { 0 };
            let end = if i == last_row {
                usize::from(last_col).min(row.cells.len().saturating_sub(1))
            } else {
                row.cells.len().saturating_sub(1)
            };
            let mut taken = Vec::new();
            if start > end {
                return taken;
            }
            for (offset, cell) in row.cells[start..=end].iter().enumerate() {
                if let Some(text) = cell {
                    taken.push(Taken {
                        row: i,
                        col: (start + offset) as u16,
                        text,
                    });
                }
            }
            taken
        })
        .collect()
}

fn raw_spans(rows: &[RowIn], first_col: u16, last_col: u16) -> Computed {
    let per_row = clipped_cells(rows, first_col, last_col);
    let mut spans = Vec::new();
    let mut text = String::new();

    for (i, cells) in per_row.iter().enumerate() {
        if let (Some(first), Some(last)) = (cells.first(), cells.last()) {
            spans.push(RelSpan {
                row: i,
                col_start: first.col,
                col_end: last.col,
            });
        }
        let line: String = cells.iter().map(|c| c.text).collect();
        text.push_str(line.trim_end());
        if i + 1 < rows.len() && !rows[i].wrapped {
            text.push('\n');
        }
    }

    Computed { spans, text }
}

fn smart_spans(rows: &[RowIn], first_col: u16, last_col: u16) -> Computed {
    let per_row = clipped_cells(rows, first_col, last_col);

    // Group physical rows into logical lines along soft wraps.
    let mut logical: Vec<Vec<Taken>> = Vec::new();
    let mut current: Vec<Taken> = Vec::new();
    for (i, cells) in per_row.into_iter().enumerate() {
        current.extend(cells);
        let joins_next = i + 1 < rows.len() && rows[i].wrapped;
        if !joins_next {
            logical.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        logical.push(current);
    }

    // Per line: drop box chrome, strip gutters, measure the indent left over.
    struct Line<'a> {
        cells: Vec<Taken<'a>>,
        start: usize,
        indent: usize,
        blank: bool,
    }
    let mut lines = Vec::new();
    for cells in logical {
        if is_box_border(&cells) {
            continue;
        }
        let start = gutter_end(&cells);
        let indent = cells[start..]
            .iter()
            .take_while(|c| c.text == " ")
            .count();
        let blank = start + indent >= cells.len();
        lines.push(Line {
            cells,
            start,
            indent,
            blank,
        });
    }

    // Dedent per gutter depth. Lines behind different amounts of chrome sit
    // in different contexts — a `│ ` box body indented under its border must
    // not have its padding preserved just because an un-guttered line above
    // it starts at column zero — while code indentation *within* one context
    // survives, because it is the same group's common indent that is removed.
    // Blank lines keep their place but must not drag the indent down.
    let mut dedents: Vec<(usize, usize)> = Vec::new();
    for line in lines.iter().filter(|l| !l.blank) {
        match dedents.iter_mut().find(|(start, _)| *start == line.start) {
            Some((_, indent)) => *indent = (*indent).min(line.indent),
            None => dedents.push((line.start, line.indent)),
        }
    }

    let mut spans: Vec<RelSpan> = Vec::new();
    let mut text = String::new();
    let line_count = lines.len();
    for (i, line) in lines.iter().enumerate() {
        let dedent = dedents
            .iter()
            .find(|(start, _)| *start == line.start)
            .map_or(0, |&(_, indent)| indent);
        let start = (line.start + dedent).min(line.cells.len());
        let kept = trim_trailing_blanks(&line.cells[start..]);

        for cell in kept {
            match spans.last_mut() {
                Some(span) if span.row == cell.row => span.col_end = cell.col,
                _ => spans.push(RelSpan {
                    row: cell.row,
                    col_start: cell.col,
                    col_end: cell.col,
                }),
            }
        }

        text.extend(kept.iter().map(|c| c.text));
        if i + 1 < line_count {
            text.push('\n');
        }
    }

    Computed { spans, text }
}

fn trim_trailing_blanks<'a, 'b>(cells: &'a [Taken<'b>]) -> &'a [Taken<'b>] {
    let end = cells
        .iter()
        .rposition(|c| c.text != " ")
        .map_or(0, |i| i + 1);
    &cells[..end]
}

/// Index of the first cell after any leading gutter markers.
fn gutter_end(cells: &[Taken]) -> usize {
    let mut idx = 0;
    // Two passes handle nesting like "│ > ".
    for _ in 0..2 {
        let marker = idx + cells[idx..].iter().take_while(|c| c.text == " ").count();
        let Some(cell) = cells.get(marker) else {
            break;
        };
        let is_marker = cell
            .text
            .chars()
            .next()
            .is_some_and(|ch| GUTTER_MARKERS.contains(&ch))
            && cell.text.chars().count() == 1;
        // Require a separating space (or end of line) so real content that
        // merely starts with a marker character is left alone.
        let separated = cells
            .get(marker + 1)
            .is_none_or(|next| next.text == " ");
        if !is_marker || !separated {
            break;
        }
        idx = (marker + 2).min(cells.len());
    }
    idx
}

fn is_box_border(cells: &[Taken]) -> bool {
    let mut saw_box = false;
    for cell in cells {
        for ch in cell.text.chars() {
            if ch == ' ' {
                continue;
            }
            if BOX_CHARS.contains(&ch) {
                saw_box = true;
            } else {
                return false;
            }
        }
    }
    saw_box
}
