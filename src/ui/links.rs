//! URL detection for PTY session views.
//!
//! vt100 drops OSC 8 hyperlinks and the host terminal only sees Summoner's
//! own grid, so links are found by scanning the rendered text. Soft-wrapped
//! rows are joined before scanning, which is what makes a URL split across
//! two rows resolve to one string instead of two broken halves.

use super::selection::viewport_top;

/// How far outside the viewport to follow soft wraps when reassembling a
/// logical line.
const WRAP_LOOKAROUND: u64 = 32;

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

/// Scan the visible rows (plus any soft-wrap continuations just outside the
/// viewport) for URLs.
pub fn scan_screen(screen: &vt100::Screen) -> Links {
    let (rows, cols) = screen.size();
    if rows == 0 || cols == 0 {
        return Links::default();
    }
    let top = viewport_top(screen);
    let bottom = top + u64::from(rows - 1);

    // A URL may start above the viewport or continue below it.
    let mut start = top;
    let mut budget = WRAP_LOOKAROUND;
    while start > 0 && budget > 0 && screen.stream_row_wrapped(start - 1) == Some(true) {
        start -= 1;
        budget -= 1;
    }
    let mut end = bottom;
    let mut budget = WRAP_LOOKAROUND;
    while budget > 0
        && screen.stream_row_wrapped(end) == Some(true)
        && screen.stream_row_wrapped(end + 1).is_some()
    {
        end += 1;
        budget -= 1;
    }

    let mut spans = Vec::new();
    let mut row = start;
    while row <= end {
        // Gather one logical line: this row plus its wrap continuations.
        let mut text = String::new();
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
                            text.push(ch);
                            map.push((last, col));
                        }
                    } else {
                        text.push(' ');
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

        for (from, to, url) in scan_logical_line(&text) {
            let cells = group_cells(&map[from..to]);
            // Keep only links with at least one cell on screen.
            if cells.iter().any(|&(r, _, _)| r >= top && r <= bottom) {
                spans.push(UrlSpan { url, cells });
            }
        }

        row = last + 1;
    }

    Links { spans }
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

        end = trim_trailing(&chars, i, end);
        if end > i + scheme_len {
            out.push((i, end, chars[i..end].iter().collect()));
        }
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

/// Open a URL in the user's browser, without blocking the UI.
pub fn open_url(url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }

    // wslview and xdg-open take the URL as a plain argument. The Windows
    // fallbacks re-parse their argument line, so the URL is quoted there —
    // the scanner never produces a URL containing a single quote.
    let quoted = format!("'{}'", url);
    let attempts: [(&str, Vec<&str>); 4] = [
        ("wslview", vec![url]),
        ("xdg-open", vec![url]),
        (
            "powershell.exe",
            vec!["-NoProfile", "-Command", "Start-Process", &quoted],
        ),
        ("cmd.exe", vec!["/c", "start", "", url]),
    ];

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
