mod common;

use common::{smart, smart_plain, COLS};
use summoner::ui::selection::SelectionMode;
use summoner::ui::smart::{self as spans, RowIn};

const TABLE: [&str; 5] = [
    "┌──────┬────────────────────────┐",
    "│  #   │          Item          │",
    "├──────┼────────────────────────┤",
    "│ MF-1 │ Add a fix              │",
    "└──────┴────────────────────────┘",
];

#[test]
fn reconstructs_a_box_table() {
    assert_eq!(
        smart_plain(&TABLE),
        "| # | Item |\n|:---:|---|\n| MF-1 | Add a fix |"
    );
}

#[test]
fn a_centred_header_does_not_override_a_left_aligned_body() {
    // Claude Code centres every header; only the body knows the real alignment.
    let delimiter = smart_plain(&TABLE).lines().nth(1).unwrap().to_string();
    assert_eq!(delimiter, "|:---:|---|");
}

#[test]
fn joins_a_cell_wrapped_across_physical_lines() {
    let table = [
        "┌──────┬────────────────────────┐",
        "│  #   │          Item          │",
        "├──────┼────────────────────────┤",
        "│      │ Add a confluence fence │",
        "│ MF-1 │ here and there         │",
        "├──────┼────────────────────────┤",
        "│ MF-2 │ Flip the fallback      │",
        "└──────┴────────────────────────┘",
    ];
    assert_eq!(
        smart_plain(&table),
        "| # | Item |\n\
         |:---:|---|\n\
         | MF-1 | Add a confluence fence here and there |\n\
         | MF-2 | Flip the fallback |"
    );
}

#[test]
fn no_interior_rules_means_one_row_per_line() {
    let table = [
        "┌─────┬─────┐",
        "│ a   │ b   │",
        "│ c   │ d   │",
        "└─────┴─────┘",
    ];
    assert_eq!(smart_plain(&table), "| a | b |\n|---|---|\n| c | d |");
}

#[test]
fn boundaries_come_from_the_rule_not_from_bars_in_cell_text() {
    let table = [
        "┌─────┬─────┐",
        "│ a│b │ c   │",
        "│ d   │ e   │",
        "└─────┴─────┘",
    ];
    assert_eq!(smart_plain(&table), "| a│b | c |\n|---|---|\n| d | e |");
}

#[test]
fn a_pipe_inside_a_cell_is_escaped() {
    let table = [
        "┌─────┬─────┐",
        "│ a|b │ c   │",
        "│ d   │ e   │",
        "└─────┴─────┘",
    ];
    assert_eq!(smart_plain(&table), "| a\\|b | c |\n|---|---|\n| d | e |");
}

#[test]
fn inline_code_works_inside_a_table_cell() {
    let table = [
        ("┌─────┬─────────────┐", ""),
        ("│ cmd │ what it is  │", ""),
        ("├─────┼─────────────┤", ""),
        ("│ run │ uses --json │", ".............cccccc.."),
        ("└─────┴─────────────┘", ""),
    ];
    assert_eq!(
        smart(&table),
        "| cmd | what it is |\n|---|---|\n| run | uses `--json` |"
    );
}

#[test]
fn a_quote_bar_block_is_not_a_table() {
    assert_eq!(
        smart_plain(&["▎ pasted text", "▎ more of it"]),
        "pasted text\nmore of it"
    );
}

#[test]
fn a_single_box_border_is_still_dropped() {
    assert_eq!(
        smart_plain(&["╭──────────╮", "│ content", "╰──────────╯"]),
        "content"
    );
}

#[test]
fn a_table_keeps_the_dedent_of_the_prose_around_it() {
    let block = [
        "  prose above",
        "  ┌─────┬─────┐",
        "  │ a   │ b   │",
        "  │ c   │ d   │",
        "  └─────┴─────┘",
        "  prose below",
    ];
    assert_eq!(
        smart_plain(&block),
        "prose above\n\n| a | b |\n|---|---|\n| c | d |\n\nprose below"
    );
}

#[test]
fn a_table_does_not_widen_the_wrap_estimate_of_the_prose_around_it() {
    let block = [
        "Ordinary prose in this reply that wraps here",
        "onto a second line",
        "┌─────────────────────────┬─────────────────────────┐",
        "│ a                       │ b                       │",
        "│ c                       │ d                       │",
        "└─────────────────────────┴─────────────────────────┘",
    ];
    let text = smart_plain(&block);
    assert!(
        text.starts_with("Ordinary prose in this reply that wraps here onto a second line\n\n|"),
        "{text}"
    );
}

#[test]
fn a_horizontally_clipped_selection_falls_back_to_plain_lines() {
    let rows: Vec<RowIn> = TABLE.iter().map(|l| RowIn::from_text(l, false, COLS)).collect();
    let text = spans::spans_core(&rows, 1, COLS - 1, SelectionMode::Smart).text;
    assert!(!text.contains("|---|"), "{text}");
    assert!(!text.contains(":---:"), "{text}");
}

#[test]
fn a_vertically_clipped_selection_promotes_the_rows_it_has() {
    let table = [
        "│ a   │ b   │",
        "├─────┼─────┤",
        "│ c   │ d   │",
    ];
    assert_eq!(smart_plain(&table), "| a | b |\n|---|---|\n| c | d |");
}

#[test]
fn table_spans_cover_content_cells_only() {
    let table = [
        "┌─────┬─────┐",
        "│ a   │ b   │",
        "│ c   │ d   │",
        "└─────┴─────┘",
    ];
    let rows: Vec<RowIn> = table.iter().map(|l| RowIn::from_text(l, false, COLS)).collect();
    let computed = spans::spans_core(&rows, 0, COLS - 1, SelectionMode::Smart);
    // The rule lines contribute no cells at all.
    assert!(computed.spans.iter().all(|s| s.row == 1 || s.row == 2), "{:?}", computed.spans);
    // The frame bars sit outside the highlighted range.
    assert_eq!(computed.spans[0].col_start, 2);
    assert_eq!(computed.spans[0].col_end, 8);
}

#[test]
fn a_soft_wrapped_table_falls_back_to_plain_lines() {
    let rows = vec![
        RowIn::from_text("┌─────┬─────┐", true, COLS),
        RowIn::from_text("spilled over", false, COLS),
        RowIn::from_text("│ a   │ b   │", false, COLS),
        RowIn::from_text("└─────┴─────┘", false, COLS),
    ];
    let text = spans::spans_core(&rows, 0, COLS - 1, SelectionMode::Smart).text;
    assert!(!text.contains("|---|"), "{text}");
}

#[test]
fn raw_mode_still_copies_a_table_verbatim() {
    let rows: Vec<RowIn> = TABLE.iter().map(|l| RowIn::from_text(l, false, COLS)).collect();
    let text = spans::spans_core(&rows, 0, COLS - 1, SelectionMode::Raw).text;
    assert_eq!(text, TABLE.join("\n"));
}

#[test]
fn a_table_gets_exactly_one_blank_line_around_it() {
    let with_gap = smart_plain(&[
        "prose above",
        "",
        "┌─────┬─────┐",
        "│ a   │ b   │",
        "│ c   │ d   │",
        "└─────┴─────┘",
        "",
        "prose below",
    ]);
    let without_gap = smart_plain(&[
        "prose above",
        "┌─────┬─────┐",
        "│ a   │ b   │",
        "│ c   │ d   │",
        "└─────┴─────┘",
        "prose below",
    ]);
    assert_eq!(with_gap, without_gap);
    assert_eq!(
        with_gap,
        "prose above\n\n| a | b |\n|---|---|\n| c | d |\n\nprose below"
    );
}
