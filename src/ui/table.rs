//! Box-drawn table reconstruction.
//!
//! Claude Code draws tables with box-drawing glyphs. Smart selection would
//! otherwise destroy them: the rule lines look like box borders and get
//! dropped, and the left frame looks like a quote bar and gets stripped. So
//! detection runs before either, anchored on a rule line — a data row is
//! indistinguishable from a quoted block until you know a table started.

use std::ops::Range;

use super::markdown::{self, Guards};
use super::smart::{Sink, Taken, BOX_CHARS};

/// Glyphs that only ever come from a frame, never from cell text.
const JUNCTIONS: &[char] = &[
    '┌', '┬', '┐', '├', '┼', '┤', '└', '┴', '┘', '╭', '╮', '╰', '╯', '┏', '┳', '┓', '┣', '╋',
    '┫', '┗', '┻', '┛', '╔', '╦', '╗', '╠', '╬', '╣', '╚', '╩', '╝',
];

const BARS: &[char] = &['│', '┃', '║'];

const RULES: &[char] = &['─', '═', '━'];

/// A table needs at least two columns, so at least three frame columns.
const MIN_BOUNDS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Centre,
    Right,
}

impl Align {
    fn delimiter(self) -> &'static str {
        match self {
            Align::Left => "---",
            Align::Centre => ":---:",
            Align::Right => "---:",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Table {
    /// Logical lines the table occupies.
    pub lines: Range<usize>,
    /// Frame columns; column `i` holds the cells in `bounds[i]+1 ..= bounds[i+1]-1`.
    bounds: Vec<u16>,
    /// Logical rows, each one or more physical lines.
    rows: Vec<Range<usize>>,
    align: Vec<Align>,
    /// Outermost frame columns, used to reject a horizontally clipped drag.
    pub frame: (u16, u16),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Rule,
    Bar,
    Other,
}

pub(crate) fn detect(lines: &[Vec<Taken>]) -> Vec<Table> {
    let mut tables = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if classify(&lines[i]) == Kind::Other {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < lines.len() && classify(&lines[end]) != Kind::Other {
            end += 1;
        }
        if let Some(table) = build(lines, i..end) {
            tables.push(table);
        }
        i = end;
    }
    tables
}

fn classify(cells: &[Taken]) -> Kind {
    let glyphs: Vec<&Taken> = cells.iter().filter(|c| c.text != " ").collect();
    if glyphs.is_empty() {
        return Kind::Other;
    }
    let all_box = glyphs
        .iter()
        .all(|c| c.text.chars().all(|ch| BOX_CHARS.contains(&ch) || JUNCTIONS.contains(&ch)));
    if all_box && glyphs.iter().any(|c| c.text.chars().any(|ch| RULES.contains(&ch))) {
        return Kind::Rule;
    }
    if glyphs.iter().filter(|c| is_bar(c)).count() >= 2 {
        return Kind::Bar;
    }
    Kind::Other
}

fn build(lines: &[Vec<Taken>], range: Range<usize>) -> Option<Table> {
    let kinds: Vec<Kind> = range.clone().map(|i| classify(&lines[i])).collect();
    if !kinds.contains(&Kind::Rule) || kinds.iter().filter(|k| **k == Kind::Bar).count() < 2 {
        return None;
    }
    // Every line of one table shares one frame. Two tables printed back to
    // back at different widths are two regions, not one.
    let frame = frame_of(&lines[range.start])?;
    if !range.clone().all(|i| frame_of(&lines[i]) == Some(frame)) {
        return None;
    }

    let rule = range.clone().find(|&i| kinds[i - range.start] == Kind::Rule)?;
    let mut bounds: Vec<u16> = lines[rule]
        .iter()
        .filter(|c| is_junction(c))
        .map(|c| c.col)
        .collect();
    if bounds.len() < MIN_BOUNDS {
        bounds = shared_bars(lines, range.clone(), &kinds);
    }
    if bounds.len() < MIN_BOUNDS {
        return None;
    }

    let rows = group_rows(&kinds, range.clone());
    if rows.is_empty() {
        return None;
    }
    let align = (0..bounds.len() - 1)
        .map(|col| column_align(lines, &rows, &bounds, col))
        .collect();
    Some(Table { lines: range, bounds, rows, align, frame })
}

/// Columns carrying a bar on *every* data line — the fallback when a rule line
/// has no junctions to read. Taking the intersection rather than the union
/// keeps a stray bar inside cell text from inventing a column.
fn shared_bars(lines: &[Vec<Taken>], range: Range<usize>, kinds: &[Kind]) -> Vec<u16> {
    let data = || {
        range
            .clone()
            .filter(|i| kinds[i - range.start] == Kind::Bar)
            .map(|i| &lines[i])
    };
    let Some(first) = data().next() else {
        return Vec::new();
    };
    first
        .iter()
        .filter(|c| is_bar(c))
        .map(|c| c.col)
        .filter(|&col| data().all(|line| line.iter().any(|c| c.col == col && is_bar(c))))
        .collect()
}

/// Consecutive data lines between two rules are one row, so a cell that wrapped
/// stays one cell. A table drawn without interior rules has no such signal, so
/// every line is its own row — otherwise the whole table collapses into one.
fn group_rows(kinds: &[Kind], range: Range<usize>) -> Vec<Range<usize>> {
    let interior_rule = kinds[1..kinds.len().saturating_sub(1)].contains(&Kind::Rule);
    let mut rows = Vec::new();
    let mut start = None;
    for i in range.clone() {
        if kinds[i - range.start] != Kind::Bar {
            if let Some(from) = start.take() {
                rows.push(from..i);
            }
            continue;
        }
        if !interior_rule {
            rows.push(i..i + 1);
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(from) = start {
        rows.push(from..range.end);
    }
    rows
}

/// Claude Code centres header text whatever the column really is, so the body
/// decides and the header is only the fallback for a table with no body.
fn column_align(
    lines: &[Vec<Taken>],
    rows: &[Range<usize>],
    bounds: &[u16],
    col: usize,
) -> Align {
    let mut body = rows[1..].iter().filter_map(|row| cell_align(lines, row, bounds, col));
    let agreed = match body.next() {
        Some(first) => body.all(|a| a == first).then_some(first),
        None => None,
    };
    agreed
        .or_else(|| cell_align(lines, &rows[0], bounds, col))
        .unwrap_or(Align::Left)
}

fn cell_align(
    lines: &[Vec<Taken>],
    row: &Range<usize>,
    bounds: &[u16],
    col: usize,
) -> Option<Align> {
    let (lo, hi) = (bounds[col] + 1, bounds[col + 1].checked_sub(1)?);
    for i in row.clone() {
        let glyphs: Vec<&Taken> = lines[i]
            .iter()
            .filter(|c| c.col >= lo && c.col <= hi && c.text != " ")
            .collect();
        let (Some(first), Some(last)) = (glyphs.first(), glyphs.last()) else {
            continue;
        };
        let (left, right) = (first.col - lo, hi - last.col);
        // A single space each side is padding, not centring, and says nothing
        // either way — such a cell abstains so the header can decide.
        return match (left, right) {
            _ if left >= 2 && left.abs_diff(right) <= 1 => Some(Align::Centre),
            _ if left > right => Some(Align::Right),
            _ if right > left => Some(Align::Left),
            _ => None,
        };
    }
    None
}

pub(crate) fn emit(table: &Table, clipped: &[Vec<&Taken>], guards: &Guards, sink: &mut Sink) {
    for (n, row) in table.rows.iter().enumerate() {
        if n > 0 {
            sink.marker("\n");
        }
        emit_row(table, row, clipped, guards, sink);
        if n == 0 {
            sink.marker("\n|");
            for align in &table.align {
                sink.marker(align.delimiter());
                sink.marker("|");
            }
        }
    }
}

fn emit_row(
    table: &Table,
    row: &Range<usize>,
    clipped: &[Vec<&Taken>],
    guards: &Guards,
    sink: &mut Sink,
) {
    sink.marker("|");
    for col in 0..table.bounds.len() - 1 {
        sink.marker(" ");
        let (lo, hi) = (table.bounds[col] + 1, table.bounds[col + 1].saturating_sub(1));
        let mut first = true;
        for i in row.clone() {
            let segment: Vec<&Taken> = clipped[i]
                .iter()
                .copied()
                .filter(|cell| cell.col >= lo && cell.col <= hi)
                .collect();
            let segment = trim_blanks(&segment);
            if segment.is_empty() {
                continue;
            }
            // A markdown cell cannot hold a newline, so a wrapped cell rejoins.
            if !first {
                sink.marker(" ");
            }
            first = false;
            markdown::emit_line(segment, guards, true, sink);
        }
        sink.marker(" |");
    }
}

fn trim_blanks<'a, 'b>(cells: &'a [&'b Taken<'b>]) -> &'a [&'b Taken<'b>] {
    let start = cells.iter().take_while(|c| c.text == " ").count();
    let end = cells.iter().rposition(|c| c.text != " ").map_or(0, |i| i + 1);
    if start >= end {
        return &cells[..0];
    }
    &cells[start..end]
}

fn frame_of(cells: &[Taken]) -> Option<(u16, u16)> {
    let mut glyphs = cells.iter().filter(|c| c.text != " ");
    let first = glyphs.next()?;
    Some((first.col, glyphs.next_back().unwrap_or(first).col))
}

fn is_bar(cell: &Taken) -> bool {
    single_char(cell).is_some_and(|ch| BARS.contains(&ch))
}

fn is_junction(cell: &Taken) -> bool {
    single_char(cell).is_some_and(|ch| JUNCTIONS.contains(&ch))
}

fn single_char(cell: &Taken) -> Option<char> {
    let mut chars = cell.text.chars();
    chars.next().filter(|_| chars.next().is_none())
}
