#![allow(dead_code)]

use summoner::ui::selection::SelectionMode;
use summoner::ui::smart::{self, CellStyle, RowIn};

pub const COLS: u16 = 60;

/// The colour Claude Code renders inline code in. Nothing in the algorithm
/// knows it — these tests pick it, and `code_colour_is_not_hardcoded` proves it.
pub const CODE: vt100::Color = vt100::Color::Idx(153);

#[derive(Clone, Copy)]
pub struct Palette {
    pub body: vt100::Color,
    pub code: vt100::Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self { body: vt100::Color::Default, code: CODE }
    }
}

/// One marker character per character of the line:
/// `.` body, `c` code colour, `d` dim code colour, `i` italic, `b` bold,
/// `B` bold+italic, `u` underline, `h` italic+underline, `1`-`9` other
/// colours.
pub fn style(mark: char, palette: Palette) -> CellStyle {
    let mut style = CellStyle { fg: palette.body, ..CellStyle::default() };
    match mark {
        'c' => style.fg = palette.code,
        'C' => {
            style.fg = palette.code;
            style.bold = true;
        }
        'H' => {
            style.fg = palette.code;
            style.italic = true;
            style.underline = true;
        }
        'd' => {
            style.fg = palette.code;
            style.dim = true;
        }
        'i' => style.italic = true,
        'b' => style.bold = true,
        'B' => {
            style.bold = true;
            style.italic = true;
        }
        'u' => style.underline = true,
        'h' => {
            style.italic = true;
            style.underline = true;
        }
        '1'..='9' => style.fg = vt100::Color::Idx(mark as u8 - b'0'),
        _ => {}
    }
    style
}

pub fn row(text: &str, marks: &str) -> RowIn<'static> {
    row_with(text, marks, Palette::default())
}

pub fn row_with(text: &str, marks: &str, palette: Palette) -> RowIn<'static> {
    let marks: Vec<char> = marks.chars().collect();
    let cells = text
        .chars()
        .enumerate()
        .map(|(i, ch)| (ch.to_string(), style(marks.get(i).copied().unwrap_or('.'), palette)))
        .collect();
    RowIn::from_styled(cells, false, COLS)
}

/// Smart-copy text for a block of `(text, marks)` pairs.
pub fn smart(lines: &[(&str, &str)]) -> String {
    smart_pal(lines, Palette::default())
}

pub fn smart_pal(lines: &[(&str, &str)], palette: Palette) -> String {
    let rows: Vec<RowIn> = lines.iter().map(|(t, m)| row_with(t, m, palette)).collect();
    smart::spans_core(&rows, 0, COLS - 1, SelectionMode::Smart).text
}

pub fn row_link(
    text: &str,
    cols: std::ops::RangeInclusive<u16>,
    url: &str,
) -> RowIn<'static> {
    row(text, "").with_link(cols, url)
}

pub fn smart_rows(rows: Vec<RowIn<'static>>) -> String {
    smart::spans_core(&rows, 0, COLS - 1, SelectionMode::Smart).text
}

/// Smart-copy text for plain, unstyled lines.
pub fn smart_plain(lines: &[&str]) -> String {
    let rows: Vec<RowIn> = lines
        .iter()
        .map(|l| RowIn::from_text(l, false, COLS))
        .collect();
    smart::spans_core(&rows, 0, COLS - 1, SelectionMode::Smart).text
}
