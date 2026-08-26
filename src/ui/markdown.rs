//! Markdown reconstruction from rendered cell styles.
//!
//! Claude Code prints markdown, and everything that made it markdown survives
//! on the grid as vt100 attributes: inline code is a foreground-colour run,
//! emphasis is the italic/bold bits, a heading is a whole line of italic plus
//! underline. This module turns those runs back into punctuation.
//!
//! The code colour is never hardcoded — themes differ. The body colour is the
//! modal colour of the selection's own cells, and a run that differs from it is
//! a candidate for backticks. That inference is only trustworthy when the
//! selection looks like prose at all, which is what [`Guards`] decides.

use std::ops::Range;

use super::smart::{Sink, Taken};

/// Past this many distinct foreground colours a selection is not prose, and
/// inline code is switched off for all of it.
const MAX_PALETTE: usize = 16;

/// Share of cells the modal colour must hold before it counts as a body colour.
const BODY_SHARE: f32 = 0.6;

/// Share of off-colour cells that makes a line syntax-highlighted code.
const HEAVY_COLOUR_RATIO: f32 = 0.5;

/// A lone colourful line is never a code block.
const MIN_FENCE_LINES: usize = 2;

/// Below this many glyphs a line's colour mix says nothing.
const MIN_CODE_LINE_CELLS: usize = 4;

pub(crate) struct Guards {
    body: vt100::Color,
    /// Whether inline-code detection runs at all for this selection.
    code: bool,
}

/// Derive the body colour and decide whether inline code can be inferred.
pub(crate) fn guards(lines: &[Vec<Taken>]) -> Guards {
    let mut palette: Vec<(vt100::Color, usize)> = Vec::new();
    let mut total = 0usize;
    let mut overflow = false;
    for cell in lines.iter().flatten().filter(|c| is_glyph(c) && !c.style.dim) {
        total += 1;
        match palette.iter().position(|(fg, _)| *fg == cell.style.fg) {
            Some(i) => palette[i].1 += 1,
            None if palette.len() < MAX_PALETTE => palette.push((cell.style.fg, 1)),
            None => overflow = true,
        }
    }
    let (body, seen) = palette
        .iter()
        .copied()
        .max_by_key(|&(_, seen)| seen)
        .unwrap_or((vt100::Color::Default, 0));
    let code = !overflow && total > 0 && seen as f32 / total as f32 >= BODY_SHARE;
    Guards { body, code }
}

/// A whole line of italic + underline is how Claude Code draws a heading.
///
/// The level is not recoverable from the grid — every heading becomes `##`.
pub(crate) fn is_heading(cells: &[Taken]) -> bool {
    let mut any = false;
    for cell in cells.iter().filter(|c| is_glyph(c)) {
        if cell.url.is_some() || !(cell.style.italic && cell.style.underline) {
            return false;
        }
        any = true;
    }
    any
}

/// Whether this line is part of a rendered fenced code block.
///
/// How Claude Code marks a code block is unverified: this is the conservative
/// fallback. A tinted background is the strong signal; failing that, a line
/// that is mostly *not* body-coloured is syntax highlighting. Both failures
/// degrade to "no fence" rather than to mangled output, because the same
/// colour test also stops inline code being inferred on such a line.
pub(crate) fn is_code_line(cells: &[Taken], guards: &Guards) -> bool {
    let glyphs: Vec<&Taken> = cells.iter().filter(|c| is_glyph(c)).collect();
    if glyphs.len() < MIN_CODE_LINE_CELLS {
        return false;
    }
    if glyphs.iter().all(|c| c.style.bg != vt100::Color::Default) {
        return true;
    }
    let off = glyphs
        .iter()
        .filter(|c| c.style.fg != guards.body && !c.style.dim)
        .count();
    off as f32 / glyphs.len() as f32 > HEAVY_COLOUR_RATIO
}

/// Runs of consecutive code lines, skipping lines already claimed by a table.
pub(crate) fn detect_code_blocks(
    lines: &[Vec<Taken>],
    guards: &Guards,
    claimed: &[bool],
) -> Vec<Range<usize>> {
    let mut blocks = Vec::new();
    let mut start = None;
    for i in 0..=lines.len() {
        let code = i < lines.len() && !claimed[i] && is_code_line(&lines[i], guards);
        match (code, start) {
            (true, None) => start = Some(i),
            (false, Some(from)) => {
                if i - from >= MIN_FENCE_LINES {
                    blocks.push(from..i);
                }
                start = None;
            }
            _ => {}
        }
    }
    blocks
}

/// Emit a line's cells with markdown markers around each style run.
pub(crate) fn emit_line(cells: &[&Taken], guards: &Guards, in_table: bool, sink: &mut Sink) {
    if cells.is_empty() {
        return;
    }
    let body_present = cells
        .iter()
        .any(|c| is_glyph(c) && !c.style.dim && c.style.fg == guards.body);
    let mut marks: Vec<Marks> = cells
        .iter()
        .map(|c| cell_marks(c, guards, body_present))
        .collect();
    bridge_blanks(cells, &mut marks);

    let mut i = 0;
    while i < cells.len() {
        let mut end = i + 1;
        while end < cells.len() && marks[end] == marks[i] {
            end += 1;
        }
        emit_run(&cells[i..end], marks[i], in_table, sink);
        i = end;
    }
}

/// Emit cells with no markers at all — headings and fenced blocks.
pub(crate) fn emit_plain(cells: &[&Taken], in_table: bool, sink: &mut Sink) {
    for cell in cells {
        push(cell, in_table, false, sink);
    }
}

/// Emit a run of lines verbatim inside a fence.
pub(crate) fn emit_fence(lines: &[Vec<&Taken>], sink: &mut Sink) {
    let dedent = lines
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.iter().take_while(|c| !is_glyph(c)).count())
        .min()
        .unwrap_or(0);
    let ticks = lines
        .iter()
        .flatten()
        .fold((0usize, 0usize), |(max, run), cell| {
            let run = if cell.text == "`" { run + 1 } else { 0 };
            (max.max(run), run)
        })
        .0;
    let fence = "`".repeat(ticks.max(2) + 1);

    sink.marker(&fence);
    for line in lines {
        sink.marker("\n");
        emit_plain(&line[dedent.min(line.len())..], false, sink);
    }
    sink.marker("\n");
    sink.marker(&fence);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Emphasis {
    None,
    Italic,
    Bold,
    Both,
}

impl Emphasis {
    /// The combined `***` delimiter is one token, so bold and italic can never
    /// interleave into an unbalanced pair.
    fn marker(self) -> &'static str {
        match self {
            Emphasis::None => "",
            Emphasis::Italic => "*",
            Emphasis::Bold => "**",
            Emphasis::Both => "***",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Marks<'a> {
    code: bool,
    emphasis: Emphasis,
    url: Option<&'a str>,
}

fn cell_marks<'a>(cell: &Taken<'a>, guards: &Guards, body_present: bool) -> Marks<'a> {
    let emphasis = match (cell.style.bold, cell.style.italic) {
        (true, true) => Emphasis::Both,
        (true, false) => Emphasis::Bold,
        (false, true) => Emphasis::Italic,
        (false, false) => Emphasis::None,
    };
    // Dim text is Claude Code's own hint chrome, not code. A line with no
    // body-coloured text of its own is a banner or a header, not prose with a
    // snippet in it — inline code is by definition a minority of its line.
    let code = guards.code
        && body_present
        && cell.url.is_none()
        && !cell.style.dim
        && cell.style.fg != guards.body;
    // `**x**` inside a code span renders the asterisks literally, so code wins.
    let emphasis = if code { Emphasis::None } else { emphasis };
    Marks { code, emphasis, url: cell.url }
}

/// Give a gap between two identically marked runs those marks, so `foo bar`
/// stays one code span instead of splitting either side of the space.
fn bridge_blanks(cells: &[&Taken], marks: &mut [Marks]) {
    let mut i = 0;
    while i < cells.len() {
        if is_glyph(cells[i]) {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < cells.len() && !is_glyph(cells[end]) {
            end += 1;
        }
        if i > 0 && end < cells.len() && marks[i - 1] == marks[end] {
            let carried = marks[i - 1];
            marks[i..end].fill(carried);
        }
        i = end;
    }
}

fn emit_run(run: &[&Taken], marks: Marks, in_table: bool, sink: &mut Sink) {
    // Delimiters may not be flanked by whitespace: `* text *` is literal
    // asterisks, and Claude Code colours an inline span's padding too.
    let lead = run.iter().take_while(|c| !is_glyph(c)).count();
    let trail = run[lead..].iter().rev().take_while(|c| !is_glyph(c)).count();
    let core = &run[lead..run.len() - trail];
    emit_plain(&run[..lead], in_table, sink);
    if core.is_empty() {
        emit_plain(&run[run.len() - trail..], in_table, sink);
        return;
    }

    let text: String = core.iter().map(|c| c.text).collect();
    let link = marks.url.filter(|url| *url != text);
    let emphasis = marks.emphasis.marker();
    let fence = marks.code.then(|| fence_for(&text));

    if link.is_some() {
        sink.marker("[");
    }
    sink.marker(emphasis);
    // A code span that touches a backtick at either end is padded at *both*,
    // since CommonMark only strips the pair.
    let pad = fence.is_some() && (text.starts_with('`') || text.ends_with('`'));
    if let Some(fence) = &fence {
        sink.marker(fence);
        if pad {
            sink.marker(" ");
        }
    }
    for cell in core {
        push(cell, in_table, link.is_some(), sink);
    }
    if let Some(fence) = &fence {
        if pad {
            sink.marker(" ");
        }
        sink.marker(fence);
    }
    sink.marker(emphasis);
    if let Some(url) = link {
        sink.marker("](");
        sink.marker(url);
        sink.marker(")");
    }
    emit_plain(&run[run.len() - trail..], in_table, sink);
}

/// A code span is fenced by one more backtick than its longest inner run.
fn fence_for(text: &str) -> String {
    let (max, _) = text.chars().fold((0usize, 0usize), |(max, run), ch| {
        let run = if ch == '`' { run + 1 } else { 0 };
        (max.max(run), run)
    });
    "`".repeat(max + 1)
}

fn push(cell: &Taken, in_table: bool, in_link: bool, sink: &mut Sink) {
    match cell.text {
        "|" if in_table => sink.cell_as(cell, "\\|"),
        "]" if in_link => sink.cell_as(cell, "\\]"),
        _ => sink.cell(cell),
    }
}

fn is_glyph(cell: &Taken) -> bool {
    cell.text != " "
}
