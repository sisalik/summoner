//! Claude Code-aware "smart" selection.
//!
//! A raw selection is a rectangle of grid cells. What the user usually wants
//! is the *text*: soft-wrapped lines joined back together, Claude Code's
//! gutter chrome (`⏺ `, `⎿ `, `> `, quote bars like `│ ` and `▎ `) and box
//! borders dropped, and the block dedented. Claude Code prints markdown, so
//! smart mode goes one step further and puts the markdown back — see
//! [`super::markdown`] and [`super::table`]. Both modes are expressed as a
//! [`SpanSet`] — the cells the copy is made from — so the selection highlight
//! can show what the clipboard is going to get.

use std::borrow::Cow;

use super::links;
use super::markdown;
use super::selection::{Selection, SelectionMode};
use super::table::{self, Table};

/// Gutter markers that introduce a block of their own: the line they mark
/// starts something new, so the line above it ended deliberately.
const ITEM_MARKERS: [char; 3] = ['⏺', '⎿', '>'];

/// Gutter markers that only draw a container around text — box sides and the
/// quote bars Claude Code puts down the left of quoted/pasted blocks. Every
/// line of the block carries one, so they say nothing about where it breaks.
const BAR_MARKERS: [char; 6] = ['│', '┃', '▏', '▎', '▍', '▌'];

/// Characters that make a line pure box-drawing chrome.
pub(crate) const BOX_CHARS: [char; 19] = [
    '─', '│', '╭', '╮', '╰', '╯', '├', '┤', '┬', '┴', '┼', '┌', '┐', '└', '┘', '═', '║', '━',
    '┃',
];

/// The rendered style of one cell, as far as markdown reconstruction cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellStyle {
    pub fg: vt100::Color,
    pub bg: vt100::Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    /// OSC 8 link id, 0 for none; resolved through [`RowIn::links`].
    pub link: u16,
}

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

#[derive(Debug, Clone)]
pub struct CellIn<'a> {
    pub text: Cow<'a, str>,
    pub style: CellStyle,
}

impl CellIn<'static> {
    fn blank() -> Self {
        Self { text: Cow::Borrowed(" "), style: CellStyle::default() }
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
    pub cells: Vec<Option<CellIn<'a>>>,
    pub wrapped: bool,
    /// OSC 8 targets used on this row. Empty for all but a handful of rows.
    pub links: Vec<(u16, Cow<'a, str>)>,
}

impl RowIn<'static> {
    /// Build an unstyled row from plain text, one column per character.
    pub fn from_text(text: &str, wrapped: bool, cols: u16) -> Self {
        Self::from_styled(
            text.chars().map(|c| (c.to_string(), CellStyle::default())).collect(),
            wrapped,
            cols,
        )
    }

    /// Build a row from per-cell text and style.
    pub fn from_styled(cells: Vec<(String, CellStyle)>, wrapped: bool, cols: u16) -> Self {
        let mut cells: Vec<Option<CellIn<'static>>> = cells
            .into_iter()
            .map(|(text, style)| Some(CellIn { text: Cow::Owned(text), style }))
            .collect();
        cells.resize(usize::from(cols), Some(CellIn::blank()));
        Self { cells, wrapped, links: Vec::new() }
    }

    /// Stamp an OSC 8 id and target over an inclusive column range.
    pub fn with_link(mut self, cols: std::ops::RangeInclusive<u16>, url: &str) -> Self {
        let id = self.links.len() as u16 + 1;
        for col in cols {
            if let Some(Some(cell)) = self.cells.get_mut(usize::from(col)) {
                cell.style.link = id;
            }
        }
        self.links.push((id, Cow::Owned(url.to_string())));
        self
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

/// The one place the highlight/clipboard relationship is expressed: a cell
/// extends both, a marker is punctuation smart mode added and extends only the
/// text.
#[derive(Default)]
pub(crate) struct Sink {
    spans: Vec<RelSpan>,
    text: String,
}

impl Sink {
    pub(crate) fn cell(&mut self, cell: &Taken) {
        self.mark(cell);
        self.text.push_str(cell.text);
    }

    /// A cell copied as something else — an escaped `|`, a `]` inside a label.
    pub(crate) fn cell_as(&mut self, cell: &Taken, text: &str) {
        self.mark(cell);
        self.text.push_str(text);
    }

    pub(crate) fn marker(&mut self, text: &str) {
        self.text.push_str(text);
    }

    fn ends_with_blank_line(&self) -> bool {
        self.text.is_empty() || self.text.ends_with("\n\n")
    }

    fn mark(&mut self, cell: &Taken) {
        // Cells usually arrive in reading order; a table's rejoined cell is the
        // exception, which is why the fallback exists at all.
        if let Some(last) = self.spans.last_mut()
            && last.row == cell.row
        {
            last.col_start = last.col_start.min(cell.col);
            last.col_end = last.col_end.max(cell.col);
            return;
        }
        match self.spans.iter_mut().find(|span| span.row == cell.row) {
            Some(span) => {
                span.col_start = span.col_start.min(cell.col);
                span.col_end = span.col_end.max(cell.col);
            }
            None => self.spans.push(RelSpan {
                row: cell.row,
                col_start: cell.col,
                col_end: cell.col,
            }),
        }
    }

    fn into_computed(mut self) -> Computed {
        self.spans.sort_by_key(|span| span.row);
        Computed { spans: self.spans, text: self.text }
    }
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
        let mut links: Vec<(u16, Cow<'_, str>)> = Vec::new();
        let mut cells: Vec<Option<CellIn<'_>>> = screen
            .stream_row_cells(stream_row)
            .into_iter()
            .flatten()
            .take(usize::from(cols))
            .map(|cell| {
                if cell.is_wide_continuation() {
                    return None;
                }
                let link = cell.link_id();
                if link != 0
                    && !links.iter().any(|(id, _)| *id == link)
                    && let Some(url) =
                        screen.hyperlink(link).filter(|u| links::is_openable(u))
                {
                    links.push((link, Cow::Borrowed(url)));
                }
                Some(CellIn {
                    text: if cell.has_contents() {
                        Cow::Borrowed(cell.contents())
                    } else {
                        Cow::Borrowed(" ")
                    },
                    style: CellStyle {
                        fg: cell.fgcolor(),
                        bg: cell.bgcolor(),
                        bold: cell.bold(),
                        dim: cell.dim(),
                        italic: cell.italic(),
                        underline: cell.underline(),
                        link,
                    },
                })
            })
            .collect();
        cells.resize(usize::from(cols), Some(CellIn::blank()));
        stream_rows.push(stream_row);
        rows.push(RowIn { cells, wrapped, links });
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
pub(crate) struct Taken<'a> {
    pub row: usize,
    pub col: u16,
    pub text: &'a str,
    pub style: CellStyle,
    pub url: Option<&'a str>,
}

fn clipped_cells<'a>(rows: &'a [RowIn], first_col: u16, last_col: u16) -> Vec<Vec<Taken<'a>>> {
    let last_row = rows.len() - 1;
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let mut taken = Vec::new();
            take_row(row, i, bounds(row, i, last_row, first_col, last_col), &mut taken);
            taken
        })
        .collect()
}

fn bounds(
    row: &RowIn,
    i: usize,
    last_row: usize,
    first_col: u16,
    last_col: u16,
) -> (usize, usize) {
    let start = if i == 0 { usize::from(first_col) } else { 0 };
    let end = if i == last_row {
        usize::from(last_col).min(row.cells.len().saturating_sub(1))
    } else {
        row.cells.len().saturating_sub(1)
    };
    (start, end)
}

/// Append one row's cells to `taken`, so a soft-wrapped logical line is built
/// in place rather than assembled and then copied.
fn take_row<'a>(
    row: &'a RowIn,
    i: usize,
    (start, end): (usize, usize),
    taken: &mut Vec<Taken<'a>>,
) {
    if start > end {
        return;
    }
    for (offset, cell) in row.cells[start..=end].iter().enumerate() {
        if let Some(cell) = cell {
            taken.push(Taken {
                row: i,
                col: (start + offset) as u16,
                text: cell.text.as_ref(),
                style: cell.style,
                url: url_for(row, cell.style.link),
            });
        }
    }
}

fn url_for<'a>(row: &'a RowIn, link: u16) -> Option<&'a str> {
    (link != 0)
        .then(|| row.links.iter().find(|(id, _)| *id == link))
        .flatten()
        .map(|(_, url)| url.as_ref())
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

/// One logical line, with everything the emit pass needs to know about it.
struct Line<'a> {
    cells: Vec<Taken<'a>>,
    start: usize,
    indent: usize,
    blank: bool,
    /// Had a gutter marker of its own, so it begins a new block.
    marked: bool,
    /// Belongs to a table or a fenced block: no chrome stripping, no dedent,
    /// no reflow.
    region: bool,
    /// A lone box border outside any table.
    dropped: bool,
}

fn smart_spans(rows: &[RowIn], first_col: u16, last_col: u16) -> Computed {
    // Analyse whole rows, not the clipped selection: gutters and indents have
    // to be measured against the real line, or starting a drag at the first
    // visible character would make that line look un-indented and cancel the
    // block's dedent. Clipping happens at the end, when cells are emitted.
    // Physical rows are grouped into logical lines along soft wraps as they
    // are read.
    let mut logical: Vec<Vec<Taken>> = Vec::new();
    let mut current: Vec<Taken> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        take_row(row, i, (0, row.cells.len().saturating_sub(1)), &mut current);
        let joins_next = i + 1 < rows.len() && rows[i].wrapped;
        if !joins_next {
            logical.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        logical.push(current);
    }
    if logical.is_empty() {
        return Computed::default();
    }

    let last_row = rows.len() - 1;
    let guards = markdown::guards(&logical);

    // Regions are claimed before any chrome is removed: dropping box borders
    // and stripping the left frame is exactly what destroys a table.
    let mut claimed = vec![false; logical.len()];
    let mut tables = table::detect(&logical);
    tables.retain(|table| {
        if clips_table(table, &logical, first_col, last_col, last_row) {
            return false;
        }
        claimed[table.lines.clone()].fill(true);
        true
    });
    let blocks = markdown::detect_code_blocks(&logical, &guards, &claimed);
    for block in &blocks {
        claimed[block.clone()].fill(true);
    }

    let mut lines: Vec<Line> = Vec::with_capacity(logical.len());
    for (i, cells) in logical.into_iter().enumerate() {
        if claimed[i] {
            lines.push(Line {
                cells,
                start: 0,
                indent: 0,
                blank: false,
                marked: true,
                region: true,
                dropped: false,
            });
            continue;
        }
        if is_box_border(&cells) {
            lines.push(Line {
                cells,
                start: 0,
                indent: 0,
                blank: true,
                marked: false,
                region: false,
                dropped: true,
            });
            continue;
        }
        let (start, marked) = gutter_end(&cells);
        let indent = cells[start..].iter().take_while(|c| c.text == " ").count();
        let blank = start + indent >= cells.len();
        lines.push(Line {
            cells,
            start,
            indent,
            blank,
            marked,
            region: false,
            dropped: false,
        });
    }

    // Dedent per gutter depth. Lines behind different amounts of chrome sit
    // in different contexts — a `│ ` box body indented under its border must
    // not have its padding preserved just because an un-guttered line above
    // it starts at column zero — while code indentation *within* one context
    // survives, because it is the same group's common indent that is removed.
    // Blank lines keep their place but must not drag the indent down.
    let mut dedents: Vec<(usize, usize)> = Vec::new();
    for line in lines.iter().filter(|l| !l.blank && !l.region && !l.dropped) {
        match dedents.iter_mut().find(|(start, _)| *start == line.start) {
            Some((_, indent)) => *indent = (*indent).min(line.indent),
            None => dedents.push((line.start, line.indent)),
        }
    }

    // Each line's cells after chrome removal, before the selection is applied.
    let kept: Vec<&[Taken]> = lines
        .iter()
        .map(|line| {
            if line.region {
                return trim_trailing_blanks(&line.cells);
            }
            let dedent = dedents
                .iter()
                .find(|(start, _)| *start == line.start)
                .map_or(0, |&(_, indent)| indent);
            let start = (line.start + dedent).min(line.cells.len());
            trim_trailing_blanks(&line.cells[start..])
        })
        .collect();

    let cols = rows[0].cells.len();

    // The width text was wrapped at, estimated from the widest line in the
    // block. A table's frame is wider than the prose around it and would
    // wreck the estimate, so regions do not contribute.
    let wrap_width = kept
        .iter()
        .enumerate()
        .filter(|(i, _)| !lines[*i].region && !lines[*i].dropped)
        .filter_map(|(_, cells)| cells.last())
        .map(|cell| usize::from(cell.col) + 1)
        .max()
        .unwrap_or(0);

    // Which lines are continuations of the line above rather than new ones.
    // Text the producing program wrapped arrives as separate lines but is one
    // paragraph; rejoin it rather than pasting mid-sentence breaks.
    let mut joins = vec![false; kept.len()];
    let mut previous: Option<usize> = None;
    for i in 0..kept.len() {
        if lines[i].region {
            previous = None;
            continue;
        }
        if lines[i].dropped {
            continue;
        }
        if let Some(p) = previous {
            joins[i] =
                !lines[i].marked && wraps_onto_next(kept[p], kept[i], wrap_width, cols);
        }
        previous = Some(i);
    }

    // Now apply the selection: drop anything outside it. A continuation is
    // being reflowed onto the line above, so its own leading indent goes away
    // — including in the highlight.
    let clipped: Vec<Vec<&Taken>> = kept
        .iter()
        .enumerate()
        .map(|(i, cells)| {
            let cells: &[Taken] = if joins[i] {
                let lead = cells.iter().take_while(|c| c.text == " ").count();
                &cells[lead..]
            } else {
                cells
            };
            cells
                .iter()
                .filter(|cell| {
                    let after_start = cell.row > 0 || cell.col >= first_col;
                    let before_end = cell.row < last_row || cell.col <= last_col;
                    after_start && before_end
                })
                .collect()
        })
        .collect();

    let mut sink = Sink::default();
    let mut pending: Option<&'static str> = None;
    let mut after_region = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].dropped {
            i += 1;
            continue;
        }
        if let Some(table) = tables.iter().find(|t| t.lines.start == i) {
            blank_line(&mut sink, &mut pending);
            table::emit(table, &clipped, &guards, &mut sink);
            after_region = true;
            i = table.lines.end;
            continue;
        }
        if let Some(block) = blocks.iter().find(|b| b.start == i) {
            blank_line(&mut sink, &mut pending);
            markdown::emit_fence(&clipped[block.clone()], &mut sink);
            after_region = true;
            i = block.end;
            continue;
        }

        let cells = &clipped[i];
        // A blank line has nothing to clip and still counts as a line break.
        // A line with content that the selection misses entirely contributes
        // nothing at all — not even a blank line.
        if cells.is_empty() && !kept[i].is_empty() {
            i += 1;
            continue;
        }
        if after_region {
            // The separator is written when the next line with content
            // arrives, so blank source lines after a region are already spent.
            if cells.is_empty() {
                i += 1;
                continue;
            }
            blank_line(&mut sink, &mut pending);
            after_region = false;
        }
        if let Some(sep) = pending.take() {
            sink.marker(sep);
        }
        if markdown::is_heading(kept[i]) {
            sink.marker("## ");
            markdown::emit_plain(cells, false, &mut sink);
        } else {
            markdown::emit_line(cells, &guards, false, &mut sink);
        }
        let next = (i + 1..lines.len()).find(|&j| !lines[j].dropped);
        pending = Some(if next.is_some_and(|j| joins[j]) { " " } else { "\n" });
        i += 1;
    }

    sink.into_computed()
}

/// Put a blank line between a region and its surroundings: a GFM table needs
/// one, and half a paragraph is never joined onto a table either way. Topping
/// up rather than always writing one keeps a blank line that was already there
/// from doubling.
fn blank_line(sink: &mut Sink, pending: &mut Option<&'static str>) {
    if let Some(sep) = pending.take() {
        sink.marker(sep);
    }
    while !sink.ends_with_blank_line() {
        sink.marker("\n");
    }
}

/// Whether the selection cuts through a table's frame sideways.
///
/// Half a table is not a table: truncated columns generate a large family of
/// edge cases for a gesture nobody makes on purpose, so the region falls back
/// to plain lines instead.
fn clips_table(
    table: &Table,
    logical: &[Vec<Taken>],
    first_col: u16,
    last_col: u16,
    last_row: usize,
) -> bool {
    let touches = |row: usize| {
        logical[table.lines.clone()]
            .iter()
            .flatten()
            .any(|cell| cell.row == row)
    };
    (touches(0) && first_col > table.frame.0) || (touches(last_row) && last_col < table.frame.1)
}

/// Whether `line` looks like it was wrapped onto `next` rather than ended.
///
/// The test is the wrap itself: if `next`'s first word would still have fit
/// on the end of `line`, the break was deliberate and is kept.
fn wraps_onto_next(
    line: &[Taken],
    next: &[Taken],
    wrap_width: usize,
    cols: usize,
) -> bool {
    // `wrap_width` is estimated from the widest line in the selection, so it
    // only means something when the block is wide enough to have been wrapped
    // at all. Narrow blocks — code, short lists — keep their breaks.
    if wrap_width * 2 < cols {
        return false;
    }
    let (Some(last), Some(_)) = (line.last(), next.first()) else {
        return false;
    };
    let line_text: String = line.iter().map(|c| c.text).collect();
    let next_text: String = next.iter().map(|c| c.text).collect();
    if starts_new_block(&next_text) {
        return false;
    }
    let (line_indent, next_indent) =
        (leading_spaces(&line_text), leading_spaces(&next_text));
    // Equal indentation continues a paragraph. Deeper indentation only
    // continues a list item, whose wrapped lines hang under its marker —
    // anywhere else, deeper indentation means a different block.
    let continues = next_indent == line_indent
        || (next_indent > line_indent && starts_new_block(&line_text));
    if !continues {
        return false;
    }
    let first_word = next_text
        .trim_start()
        .split(' ')
        .next()
        .unwrap_or("")
        .chars()
        .count();
    usize::from(last.col) + 2 + first_word > wrap_width
}

fn leading_spaces(text: &str) -> usize {
    text.chars().take_while(|&c| c == ' ').count()
}

/// Markers that start a new block, so the line before them ended on purpose.
fn starts_new_block(text: &str) -> bool {
    let t = text.trim_start();
    if t.starts_with("- ")
        || t.starts_with("* ")
        || t.starts_with("+ ")
        || t.starts_with("• ")
        || t.starts_with("#")
        || t.starts_with("|")
        || t.starts_with("```")
    {
        return true;
    }
    // "1. " / "2) " list items
    let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
    !digits.is_empty()
        && t[digits.len()..]
            .starts_with(['.', ')'])
        && t[digits.len() + 1..].starts_with(' ')
}

fn trim_trailing_blanks<'a, 'b>(cells: &'a [Taken<'b>]) -> &'a [Taken<'b>] {
    let end = cells
        .iter()
        .rposition(|c| c.text != " ")
        .map_or(0, |i| i + 1);
    &cells[..end]
}

/// Index of the first cell after any leading gutter markers, and whether one
/// of them was an item marker rather than a mere container bar.
fn gutter_end(cells: &[Taken]) -> (usize, bool) {
    let (mut idx, mut item) = (0, false);
    // Two passes handle nesting like "│ > ".
    for _ in 0..2 {
        let marker = idx + cells[idx..].iter().take_while(|c| c.text == " ").count();
        let Some(cell) = cells.get(marker) else {
            break;
        };
        let ch = cell
            .text
            .chars()
            .next()
            .filter(|_| cell.text.chars().count() == 1);
        let Some(ch) = ch.filter(|ch| ITEM_MARKERS.contains(ch) || BAR_MARKERS.contains(ch))
        else {
            break;
        };
        // Require a separating space (or end of line) so real content that
        // merely starts with a marker character is left alone.
        let separated = cells
            .get(marker + 1)
            .is_none_or(|next| next.text == " ");
        if !separated {
            break;
        }
        item |= ITEM_MARKERS.contains(&ch);
        idx = (marker + 2).min(cells.len());
    }
    (idx, item)
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
