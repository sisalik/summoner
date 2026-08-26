//! URL detection for PTY session views.
//!
//! Links come from two places. A program may mark one explicitly with an
//! OSC 8 escape, which the patched vt100 records as a link id on every cell of
//! the label; those are read straight off the grid. Everything else is found
//! by scanning the rendered text, because the host terminal only ever sees
//! Summoner's composed grid and cannot do it for us. Soft-wrapped rows are
//! joined before scanning, which is what makes a URL split across two rows
//! resolve to one string instead of two broken halves. Claude Code wraps its
//! own output instead of letting the terminal do it, so a URL can also be
//! split by a real newline; those rows are rejoined too, but only when the
//! break falls on the right margin mid-URL.

use super::selection::viewport_top;

/// How far outside the viewport to follow soft wraps when reassembling a
/// logical line.
const WRAP_LOOKAROUND: u64 = 32;

/// How far outside the viewport to look for hard-wrapped continuations.
const HARD_LOOKAROUND: u64 = 2;

/// Characters that make a token look like part of a URL rather than a word.
const URL_PUNCT: [char; 11] = ['/', '.', '-', '_', '?', '=', '&', '#', '%', '~', '+'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlSpan {
    pub url: String,
    /// (stream_row, col_start, col_end inclusive), one entry per row the URL
    /// occupies.
    pub cells: Vec<(u64, u16, u16)>,
}

#[derive(Debug, Clone, Default)]
pub struct Links {
    pub spans: Vec<UrlSpan>,
}

impl Links {
    pub fn contains(&self, stream_row: u64, col: u16) -> bool {
        self.span_at(stream_row, col).is_some()
    }

    pub fn span_at(&self, stream_row: u64, col: u16) -> Option<&UrlSpan> {
        self.spans.iter().find(|span| {
            span.cells
                .iter()
                .any(|&(r, start, end)| r == stream_row && col >= start && col <= end)
        })
    }
}

/// Collect the OSC 8 hyperlinks marked on the visible cells.
///
/// Needs no wrap lookaround: the link id travels on the cell, so a label split
/// across rows is reassembled by grouping on the id alone.
fn scan_osc8(screen: &vt100::Screen, top: u64, rows: u16, cols: u16) -> Vec<UrlSpan> {
    let mut spans: Vec<(u16, UrlSpan)> = Vec::new();
    for row in 0..rows {
        let stream_row = top + u64::from(row);
        let mut col = 0;
        while col < cols {
            let id = screen.cell(row, col).map_or(0, vt100::Cell::link_id);
            if id == 0 {
                col += 1;
                continue;
            }
            let start = col;
            while col < cols
                && screen.cell(row, col).map_or(0, vt100::Cell::link_id) == id
            {
                col += 1;
            }
            let run = (stream_row, start, col - 1);
            if let Some(entry) = spans.iter_mut().find(|(seen, _)| *seen == id) {
                entry.1.cells.push(run);
            } else if let Some(url) = screen.hyperlink(id).filter(|u| is_openable(u)) {
                spans.push((id, UrlSpan { url: url.to_string(), cells: vec![run] }));
            }
        }
    }
    spans.into_iter().map(|(_, span)| span).collect()
}

/// One logical line: a row plus its soft-wrap continuations, with each
/// character mapped back to the cell it came from.
struct Logical {
    chars: Vec<char>,
    map: Vec<(u64, u16)>,
}

/// Scan the visible rows (plus any continuations just outside the viewport)
/// for URLs.
pub fn scan_screen(screen: &vt100::Screen) -> Links {
    let (rows, cols) = screen.size();
    if rows == 0 || cols == 0 {
        return Links::default();
    }
    let top = viewport_top(screen);
    let bottom = top + u64::from(rows - 1);

    // A URL may start above the viewport or continue below it.
    let mut start = top;
    let mut budget = WRAP_LOOKAROUND + HARD_LOOKAROUND;
    for _ in 0..=HARD_LOOKAROUND {
        while start > 0 && budget > 0 && screen.stream_row_wrapped(start - 1) == Some(true) {
            start -= 1;
            budget -= 1;
        }
        if start > 0 && budget > 0 {
            start -= 1;
            budget -= 1;
        }
    }
    let mut end = bottom;
    let mut budget = WRAP_LOOKAROUND + HARD_LOOKAROUND;
    for _ in 0..=HARD_LOOKAROUND {
        while budget > 0
            && screen.stream_row_wrapped(end) == Some(true)
            && screen.stream_row_wrapped(end + 1).is_some()
        {
            end += 1;
            budget -= 1;
        }
        if budget > 0 && screen.stream_row_wrapped(end + 1).is_some() {
            end += 1;
            budget -= 1;
        }
    }

    let lines = logical_lines(screen, start, end, cols);
    // OSC 8 spans come first so `span_at` prefers a link the program declared
    // over a bare URL matched in the same cells.
    let mut spans = scan_osc8(screen, top, rows, cols);
    for (i, line) in lines.iter().enumerate() {
        for (from, scheme_len, raw_end) in find_urls(&line.chars) {
            let mut chars = line.chars[from..raw_end].to_vec();
            let mut map = line.map[from..raw_end].to_vec();
            join_hard_wraps(&lines, i, raw_end, cols, &mut chars, &mut map);

            let end = trim_trailing(&chars, 0, chars.len());
            if end <= scheme_len {
                continue;
            }
            let cells = group_cells(&map[..end]);
            // Keep only links with at least one cell on screen.
            if cells.iter().any(|&(r, _, _)| r >= top && r <= bottom) {
                spans.push(UrlSpan {
                    url: chars[..end].iter().collect(),
                    cells,
                });
            }
        }
    }

    Links { spans }
}

fn logical_lines(screen: &vt100::Screen, start: u64, end: u64, cols: u16) -> Vec<Logical> {
    let mut lines = Vec::new();
    let mut row = start;
    while row <= end {
        let mut chars = Vec::new();
        let mut map: Vec<(u64, u16)> = Vec::new();
        let mut last = row;
        loop {
            if screen.stream_row_wrapped(last).is_none() {
                break;
            }
            if let Some(cells) = screen.stream_row_cells(last) {
                for (col, cell) in cells.take(usize::from(cols)).enumerate() {
                    let col = col as u16;
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    if cell.has_contents() {
                        for ch in cell.contents().chars() {
                            chars.push(ch);
                            map.push((last, col));
                        }
                    } else {
                        chars.push(' ');
                        map.push((last, col));
                    }
                }
            }
            if screen.stream_row_wrapped(last) == Some(true)
                && screen.stream_row_wrapped(last + 1).is_some()
            {
                last += 1;
            } else {
                break;
            }
        }

        lines.push(Logical { chars, map });
        row = last + 1;
    }
    lines
}

/// Follow a URL that a hard line break cut in two.
///
/// The URL has to run to the end of its line, and the next line's leading
/// token has to be one the break can explain: too long to have fitted after
/// the URL. Text wrapped by the producing program is wrapped at its own
/// width, not the terminal's, so the cut lands short of the last column and
/// the column alone says nothing. The token must also still look like a URL —
/// a path or query fragment, not a word — so a line that merely ends with a
/// link keeps the prose below it out.
fn join_hard_wraps(
    lines: &[Logical],
    from: usize,
    url_end: usize,
    cols: u16,
    chars: &mut Vec<char>,
    map: &mut Vec<(u64, u16)>,
) {
    let mut i = from;
    let mut run_end = url_end;
    loop {
        if !lines[i].chars[run_end..].iter().all(|c| *c == ' ') {
            return;
        }
        let (Some(next), Some(&(_, last_col))) = (lines.get(i + 1), map.last()) else {
            return;
        };
        let Some((start, end)) = continuation_token(&next.chars) else {
            return;
        };
        if usize::from(last_col) + 1 + (end - start) <= usize::from(cols) {
            return;
        }
        chars.extend_from_slice(&next.chars[start..end]);
        map.extend_from_slice(&next.map[start..end]);
        i += 1;
        run_end = end;
    }
}

/// Shortest punctuation-free token accepted as a URL tail. Prose words run
/// shorter; base64-ish query values (OAuth state, code challenges) run longer.
const BARE_TAIL_MIN: usize = 16;

/// The leading token of a line, if it could be the tail of a split URL.
fn continuation_token(chars: &[char]) -> Option<(usize, usize)> {
    let start = chars.iter().take_while(|c| **c == ' ').count();
    let end = start + chars[start..].iter().take_while(|c| is_url_char(**c)).count();
    let token = &chars[start..end];
    if token.is_empty() {
        return None;
    }
    // A token with a scheme of its own is the next link, not this one's tail.
    if token.windows(3).any(|w| w == [':', '/', '/']) {
        return None;
    }
    if token.iter().any(|c| URL_PUNCT.contains(c)) {
        return Some((start, end));
    }
    // No URL punctuation: a long bare value (e.g. a base64 state parameter)
    // can still end a URL. Accept it only when it is the line's sole content —
    // prose below a link comes as a sentence, not one long lone word.
    let alone = chars[end..].iter().all(|c| *c == ' ');
    (alone && end - start >= BARE_TAIL_MIN).then_some((start, end))
}

fn group_cells(map: &[(u64, u16)]) -> Vec<(u64, u16, u16)> {
    let mut out: Vec<(u64, u16, u16)> = Vec::new();
    for &(row, col) in map {
        match out.last_mut() {
            Some(last) if last.0 == row => last.2 = last.2.max(col),
            _ => out.push((row, col, col)),
        }
    }
    out
}

/// Find `http(s)://` URLs in one logical line.
///
/// Returns (char start, char end exclusive, url) triples, with char indices
/// into the line's character sequence.
pub fn scan_logical_line(line: &str) -> Vec<(usize, usize, String)> {
    let chars: Vec<char> = line.chars().collect();
    find_urls(&chars)
        .into_iter()
        .filter_map(|(start, scheme_len, raw_end)| {
            let end = trim_trailing(&chars, start, raw_end);
            (end > start + scheme_len)
                .then(|| (start, end, chars[start..end].iter().collect()))
        })
        .collect()
}

/// Locate URL runs, untrimmed: (start, scheme length, end exclusive).
///
/// Trailing punctuation is left on so callers that stitch rows together can
/// decide what belongs to the URL once the whole thing is assembled.
fn find_urls(chars: &[char]) -> Vec<(usize, usize, usize)> {
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_ascii_lowercase())
        .collect();
    const HTTPS: [char; 8] = ['h', 't', 't', 'p', 's', ':', '/', '/'];
    const HTTP: [char; 7] = ['h', 't', 't', 'p', ':', '/', '/'];

    let mut out = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        if lower[i] != 'h' {
            i += 1;
            continue;
        }
        let scheme_len = if lower[i..].starts_with(&HTTPS) {
            HTTPS.len()
        } else if lower[i..].starts_with(&HTTP) {
            HTTP.len()
        } else {
            i += 1;
            continue;
        };
        // Don't match inside a longer token (e.g. "xhttp://").
        if i > 0 && chars[i - 1].is_alphanumeric() {
            i += 1;
            continue;
        }

        let mut end = i + scheme_len;
        while end < chars.len() && is_url_char(chars[end]) {
            end += 1;
        }
        // Bare scheme with no host is not a link.
        if end == i + scheme_len {
            i = end;
            continue;
        }

        out.push((i, scheme_len, end));
        i = end.max(i + 1);
    }

    out
}

fn is_url_char(ch: char) -> bool {
    !ch.is_whitespace()
        && !ch.is_control()
        && !matches!(ch, '<' | '>' | '"' | '\'' | '`' | '|' | '\\')
}

/// Drop punctuation that trails a URL in prose rather than belonging to it.
fn trim_trailing(chars: &[char], start: usize, mut end: usize) -> usize {
    while end > start {
        let ch = chars[end - 1];
        let drop = match ch {
            '.' | ',' | ';' | ':' | '!' | '?' => true,
            ')' | ']' | '}' => {
                // Keep balanced pairs: .../Rust_(language) stays intact.
                let open = match ch {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                let opens = chars[start..end].iter().filter(|&&c| c == open).count();
                let closes = chars[start..end].iter().filter(|&&c| c == ch).count();
                closes > opens
            }
            _ => false,
        };
        if !drop {
            break;
        }
        end -= 1;
    }
    end
}

/// Whether we are running under WSL, where links belong in the Windows
/// browser rather than whatever `xdg-open` picks on the Linux side.
fn is_wsl() -> bool {
    if std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::env::var_os("WSL_INTEROP").is_some()
    {
        return true;
    }
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .is_ok_and(|s| s.to_ascii_lowercase().contains("microsoft"))
}

/// PowerShell command that opens `url`, encoded so nothing re-parses it.
///
/// `-EncodedCommand` takes base64 of a UTF-16LE script, which sidesteps
/// every quoting layer between here and PowerShell — `&` in a query string
/// would otherwise break `-Command`.
fn powershell_encoded_command(url: &str) -> String {
    use base64::Engine;
    let script = format!("Start-Process '{}'", url.replace('\'', "''"));
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    base64::engine::general_purpose::STANDARD.encode(utf16)
}

/// Whether a URL is one Summoner will hand to a browser. Also gates which
/// OSC 8 links are drawn as clickable, so nothing is ever underlined that a
/// Ctrl+click would silently ignore.
pub(crate) fn is_openable(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Open a URL in the user's browser, without blocking the UI.
pub fn open_url(url: &str) {
    if !is_openable(url) {
        return;
    }

    let encoded = powershell_encoded_command(url);
    let mut attempts: Vec<(&str, Vec<&str>)> = Vec::new();
    if is_wsl() {
        // wslview hands off to the Windows default browser; the PowerShell
        // and cmd fallbacks do the same without needing wslu installed.
        // xdg-open comes last: on WSL it would open a Linux browser.
        attempts.push(("wslview", vec![url]));
        attempts.push(("powershell.exe", vec!["-NoProfile", "-EncodedCommand", &encoded]));
        attempts.push(("cmd.exe", vec!["/c", "start", "", url]));
    }
    attempts.push(("xdg-open", vec![url]));
    attempts.push(("open", vec![url]));

    for (cmd, args) in attempts {
        let spawned = std::process::Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Ok(mut child) = spawned {
            // Reap in the background so the opener doesn't linger as a zombie.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return;
        }
    }
}
