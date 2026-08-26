mod common;

use common::{row, row_link, smart, smart_pal, smart_rows, Palette};

#[test]
fn a_colour_run_becomes_inline_code() {
    assert_eq!(
        smart(&[("Use the --json flag here", "........cccccc..........")]),
        "Use the `--json` flag here"
    );
}

#[test]
fn the_code_colour_is_not_hardcoded() {
    let line = [("Use the --json flag here", "........cccccc..........")];
    let expected = "Use the `--json` flag here";
    for code in [vt100::Color::Idx(9), vt100::Color::Rgb(0, 255, 0)] {
        let palette = Palette { body: vt100::Color::Default, code };
        assert_eq!(smart_pal(&line, palette), expected);
    }
}

#[test]
fn the_body_colour_is_the_selections_own_not_the_terminal_default() {
    let palette = Palette { body: vt100::Color::Idx(7), code: vt100::Color::Default };
    assert_eq!(
        smart_pal(&[("Use the --json flag here", "........cccccc..........")], palette),
        "Use the `--json` flag here"
    );
}

#[test]
fn a_code_run_keeps_its_interior_space_and_drops_its_padding() {
    assert_eq!(
        smart(&[("Run the cargo build command again", "........ccccccccccc..............")]),
        "Run the `cargo build` command again"
    );
    // The space between two coloured words is body-coloured: still one span.
    assert_eq!(
        smart(&[("Run the cargo build command again", "........ccccc.ccccc..............")]),
        "Run the `cargo build` command again"
    );
    // Claude Code colours an inline span's trailing padding too.
    assert_eq!(
        smart(&[("Run the cargo command again now", "........cccccc.................")]),
        "Run the `cargo` command again now"
    );
}

#[test]
fn a_code_run_containing_a_backtick_uses_a_longer_fence() {
    assert_eq!(smart(&[("Type a`b now", ".....ccc....")]), "Type ``a`b`` now");
}

#[test]
fn a_code_run_touching_a_backtick_is_padded_at_both_ends() {
    assert_eq!(smart(&[("Type `x now", ".....cc....")]), "Type `` `x `` now");
}

#[test]
fn dim_hint_text_is_not_code() {
    assert_eq!(
        smart(&[("Press ctrl+o to expand", "......dddddd..........")]),
        "Press ctrl+o to expand"
    );
}

#[test]
fn a_line_with_no_body_coloured_text_gets_no_code_marks() {
    assert_eq!(
        smart(&[
            ("Ordinary prose in a reply", "........................"),
            ("col_line", "cccccccc"),
        ]),
        "Ordinary prose in a reply\ncol_line"
    );
}

#[test]
fn a_selection_with_no_dominant_colour_disables_inline_code() {
    assert_eq!(smart(&[("aaaabbbbcccc", "111122223333")]), "aaaabbbbcccc");
}

#[test]
fn emphasis_maps_to_one_delimiter_per_combination() {
    let text = "P0 makes it safe by construction now";
    let cases = [
        ('i', "P0 makes it safe *by construction* now"),
        ('b', "P0 makes it safe **by construction** now"),
        ('B', "P0 makes it safe ***by construction*** now"),
    ];
    for (mark, expected) in cases {
        let marks: String = (0..text.chars().count())
            .map(|i| if (17..32).contains(&i) { mark } else { '.' })
            .collect();
        assert_eq!(smart(&[(text, &marks)]), expected);
    }
}

#[test]
fn emphasis_delimiters_are_never_flanked_by_a_space() {
    assert_eq!(
        smart(&[("safe by construction now", ".....iiiiiiiiiiiiiiii...")]),
        "safe *by construction* now"
    );
}

#[test]
fn adjacent_italic_and_bold_runs_stay_balanced() {
    // CommonMark resolves this by the flanking rules; no nesting stack needed.
    assert_eq!(smart(&[("ab", "ib")]), "*a***b**");
}

#[test]
fn emphasis_inside_a_code_run_is_suppressed() {
    assert_eq!(smart(&[("run fast now", "....CCCC....")]), "run `fast` now");
}

#[test]
fn a_line_of_italic_and_underline_is_a_heading() {
    assert_eq!(
        smart(&[("markfluence fix list", "hhhhhhhhhhhhhhhhhhhh")]),
        "## markfluence fix list"
    );
}

#[test]
fn a_partly_italic_line_is_not_a_heading() {
    assert_eq!(
        smart(&[("markfluence fix list", "hhhhhhhhhhh.........")]),
        "*markfluence* fix list"
    );
}

#[test]
fn a_heading_gets_no_inline_markers() {
    assert_eq!(
        smart(&[
            ("Ordinary prose in this reply", "..........................."),
            ("A tinted heading", "HHHHHHHHHHHHHHHH"),
        ]),
        "Ordinary prose in this reply\n## A tinted heading"
    );
}

#[test]
fn an_osc8_label_becomes_a_markdown_link() {
    assert_eq!(
        smart_rows(vec![row_link("See the docs here", 8..=11, "https://example.com/docs")]),
        "See the [docs](https://example.com/docs) here"
    );
}

#[test]
fn an_osc8_label_equal_to_its_target_stays_bare() {
    assert_eq!(
        smart_rows(vec![row_link("https://x.io", 0..=11, "https://x.io")]),
        "https://x.io"
    );
}

#[test]
fn link_punctuation_is_not_highlighted() {
    use summoner::ui::selection::SelectionMode;
    use summoner::ui::smart;

    let rows = vec![row_link("See the docs here", 8..=11, "https://example.com/docs")];
    let computed = smart::spans_core(&rows, 0, common::COLS - 1, SelectionMode::Smart);
    // One span over the whole line; the URL and brackets add no cells.
    assert_eq!(computed.spans.len(), 1);
    assert_eq!(computed.spans[0].col_start, 0);
    assert_eq!(computed.spans[0].col_end, 16);
}

#[test]
fn two_heavily_coloured_lines_become_a_fence() {
    assert_eq!(
        smart(&[
            ("Ordinary prose in this reply", "..........................."),
            ("let x = compute(1);", "1111111111111111111"),
            ("let y = compute(2);", "2222222222222222222"),
        ]),
        "Ordinary prose in this reply\n\n```\nlet x = compute(1);\nlet y = compute(2);\n```"
    );
}

#[test]
fn a_background_tinted_block_becomes_a_fence() {
    assert_eq!(
        smart(&[
            ("Ordinary prose in this reply", "..........................."),
            ("let a = 1;", "gggggggggg"),
            ("let b = 2;", "gggggggggg"),
        ]),
        "Ordinary prose in this reply\n\n```\nlet a = 1;\nlet b = 2;\n```"
    );
}

#[test]
fn one_colourful_line_is_not_a_fence() {
    let text = smart(&[
        ("Ordinary prose in a reply", "........................"),
        ("let x = compute(1);", "1111111111111111111"),
        ("Ordinary prose in a reply", "........................"),
    ]);
    assert!(!text.contains("```"), "{text}");
}

#[test]
fn unstyled_text_is_untouched() {
    let plain = row("plain *text* with _marks_ and | pipes", "");
    assert_eq!(smart_rows(vec![plain]), "plain *text* with _marks_ and | pipes");
}
