use summoner::ui::selection::{Selection, SelectionMode};
use summoner::ui::smart::{self, RowIn};

const COLS: u16 = 40;

fn rows(lines: &[&str]) -> Vec<RowIn<'static>> {
    lines
        .iter()
        .map(|l| RowIn::from_text(l, false, COLS))
        .collect()
}

fn smart_text(lines: &[&str]) -> String {
    let rows = rows(lines);
    smart::spans_core(&rows, 0, COLS - 1, SelectionMode::Smart).text
}

#[test]
fn strips_claude_gutter_markers() {
    assert_eq!(smart_text(&["⏺ Ran tool"]), "Ran tool");
    assert_eq!(smart_text(&["│ inside a box"]), "inside a box");
    assert_eq!(smart_text(&["⎿ tool result"]), "tool result");
    assert_eq!(smart_text(&["> quoted line"]), "quoted line");
}

#[test]
fn strips_quote_bar_markers() {
    assert_eq!(smart_text(&["▎ pasted text"]), "pasted text");
    assert_eq!(smart_text(&["▌ pasted text"]), "pasted text");
    let text = smart_text(&["▎ first line", "▎", "▎ second line"]);
    assert_eq!(text, "first line\n\nsecond line");
}

#[test]
fn strips_indented_quote_bars() {
    let text = smart_text(&["    ▎ short", "    ▎ a second line of the same block"]);
    assert_eq!(text, "short\na second line of the same block");
}

#[test]
fn strips_nested_gutters() {
    assert_eq!(smart_text(&["│ > nested quote"]), "nested quote");
}

#[test]
fn leaves_content_that_merely_starts_with_a_marker_character() {
    assert_eq!(smart_text(&[">>= operator"]), ">>= operator");
}

#[test]
fn drops_box_border_lines() {
    let text = smart_text(&["╭──────────╮", "│ content", "╰──────────╯"]);
    assert_eq!(text, "content");
}

#[test]
fn applies_common_dedent() {
    let text = smart_text(&["  fn main() {", "    body();", "  }"]);
    assert_eq!(text, "fn main() {\n  body();\n}");
}

#[test]
fn blank_lines_survive_but_do_not_drive_the_dedent() {
    let text = smart_text(&["    first", "", "    second"]);
    assert_eq!(text, "first\n\nsecond");
}

#[test]
fn handles_mixed_gutter_and_plain_lines() {
    let text = smart_text(&["⏺ Summary", "  plain line"]);
    assert_eq!(text, "Summary\nplain line");
}

#[test]
fn code_indentation_inside_a_gutter_survives() {
    let text = smart_text(&["│ fn main() {", "│     body();", "│ }"]);
    assert_eq!(text, "fn main() {\n    body();\n}");
}

/// Starting the drag on the first character of a line used to make that line
/// look un-indented, which cancelled the dedent for the whole block.
#[test]
fn dedents_even_when_the_drag_starts_at_the_first_character() {
    let rows = rows(&["    alpha", "    beta"]);
    let computed = smart::spans_core(&rows, 4, COLS - 1, SelectionMode::Smart);
    assert_eq!(computed.text, "alpha\nbeta");
}

fn wide(lines: &[&str]) -> Vec<RowIn<'static>> {
    lines
        .iter()
        .map(|l| RowIn::from_text(l, false, 80))
        .collect()
}

#[test]
fn rejoins_prose_the_program_wrapped() {
    let rows = wide(&[
        "  Selections were anchored to the live screen top, so streaming output slid",
        "  the text out from under the highlight.",
    ]);
    let computed = smart::spans_core(&rows, 0, 79, SelectionMode::Smart);
    assert_eq!(
        computed.text,
        "Selections were anchored to the live screen top, so streaming output slid \
         the text out from under the highlight.",
    );
}

#[test]
fn rejoins_prose_wrapped_behind_a_quote_bar() {
    let rows = wide(&[
        "  ▎ Selections were anchored to the live screen top, so streaming output slid",
        "  ▎ the text out from under the highlight.",
    ]);
    let computed = smart::spans_core(&rows, 0, 79, SelectionMode::Smart);
    assert_eq!(
        computed.text,
        "Selections were anchored to the live screen top, so streaming output slid \
         the text out from under the highlight.",
    );
}

/// Bars are containers, but an item marker behind one still starts a block.
#[test]
fn keeps_a_break_at_an_item_marker_behind_a_bar() {
    let rows = wide(&[
        "  ▎ Selections were anchored to the live screen top, so streaming output slid",
        "  ▎ > the text out from under the highlight.",
    ]);
    let computed = smart::spans_core(&rows, 0, 79, SelectionMode::Smart);
    assert_eq!(
        computed.text,
        "Selections were anchored to the live screen top, so streaming output slid\n\
         the text out from under the highlight.",
    );
}

#[test]
fn keeps_a_break_when_the_next_word_would_have_fit() {
    let rows = wide(&[
        "  Short.",
        "  This is a long line that runs most of the way out to the eightieth column",
    ]);
    let computed = smart::spans_core(&rows, 0, 79, SelectionMode::Smart);
    assert_eq!(
        computed.text,
        "Short.\nThis is a long line that runs most of the way out to the eightieth column",
    );
}

#[test]
fn rejoins_a_wrapped_list_item_across_its_hanging_indent() {
    let rows = wide(&[
        "  2. Links work in-app instead of via Windows Terminal. Windows Terminal",
        "     only ever sees the rendered grid, which is why URL detection kept",
        "     producing mangled links.",
    ]);
    let computed = smart::spans_core(&rows, 0, 79, SelectionMode::Smart);
    assert_eq!(
        computed.text,
        "2. Links work in-app instead of via Windows Terminal. Windows Terminal \
         only ever sees the rendered grid, which is why URL detection kept \
         producing mangled links.",
    );
}

#[test]
fn does_not_join_list_items() {
    let rows = wide(&[
        "  - first item that runs right out to the far end of the eighty column line",
        "  - second item",
    ]);
    let computed = smart::spans_core(&rows, 0, 79, SelectionMode::Smart);
    assert_eq!(
        computed.text,
        "- first item that runs right out to the far end of the eighty column line\n- second item",
    );
}

#[test]
fn joins_soft_wrapped_rows_into_one_logical_line() {
    let rows = vec![
        RowIn::from_text(&format!("{:width$}", "aaaa", width = COLS as usize), true, COLS),
        RowIn::from_text("bbbb", false, COLS),
    ];
    let computed = smart::spans_core(&rows, 0, COLS - 1, SelectionMode::Smart);
    assert!(!computed.text.contains('\n'));
    assert!(computed.text.starts_with("aaaa"));
    assert!(computed.text.ends_with("bbbb"));
}

#[test]
fn a_selection_starting_past_the_gutter_strips_nothing() {
    let rows = rows(&["⏺ Ran tool"]);
    // Start the selection at the "R" of "Ran".
    let computed = smart::spans_core(&rows, 2, COLS - 1, SelectionMode::Smart);
    assert_eq!(computed.text, "Ran tool");
    assert_eq!(computed.spans.len(), 1);
    assert_eq!(computed.spans[0].col_start, 2);
}

#[test]
fn spans_exclude_the_stripped_cells() {
    let rows = rows(&["⏺   indented"]);
    let computed = smart::spans_core(&rows, 0, COLS - 1, SelectionMode::Smart);
    assert_eq!(computed.text, "indented");
    // Gutter marker, its space and the dedented indent are not highlighted.
    assert_eq!(computed.spans[0].col_start, 4);
    assert_eq!(computed.spans[0].col_end, 11);
}

#[test]
fn raw_mode_copies_cells_verbatim() {
    let rows = rows(&["⏺   indented", "  second"]);
    let computed = smart::spans_core(&rows, 0, COLS - 1, SelectionMode::Raw);
    assert_eq!(computed.text, "⏺   indented\n  second");
    assert_eq!(computed.spans[0].col_start, 0);
    assert_eq!(computed.spans[0].col_end, COLS - 1);
}

#[test]
fn highlight_matches_the_copied_text_on_a_real_screen() {
    let mut p = vt100::Parser::new(6, COLS, 100);
    p.process("⏺ Ran tool\r\n".as_bytes());
    p.process("  │   indented body\r\n".as_bytes());

    let sel = Selection {
        anchor: (0, 0),
        moving: (1, COLS - 1),
        dragged: true,
        mode: SelectionMode::Smart,
    };
    let spans = smart::compute_spans(p.screen(), &sel);
    assert_eq!(spans.text(), "Ran tool\nindented body");

    // Every highlighted cell is part of the copied text, and nothing else is.
    let mut highlighted = String::new();
    for row in 0..2u64 {
        for col in 0..COLS {
            if spans.contains(row, col) {
                let cell = p.screen().stream_cell(row, col).unwrap();
                highlighted.push_str(if cell.has_contents() { cell.contents() } else { " " });
            }
        }
        if row == 0 {
            highlighted.push('\n');
        }
    }
    assert_eq!(highlighted, spans.text());
}
