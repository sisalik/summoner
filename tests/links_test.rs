use summoner::ui::links::{scan_logical_line, scan_screen};

fn urls(line: &str) -> Vec<String> {
    scan_logical_line(line)
        .into_iter()
        .map(|(_, _, url)| url)
        .collect()
}

#[test]
fn finds_plain_urls() {
    assert_eq!(urls("https://example.com"), vec!["https://example.com"]);
    assert_eq!(urls("go to http://a.b/c now"), vec!["http://a.b/c"]);
}

#[test]
fn trims_sentence_punctuation() {
    assert_eq!(urls("see https://example.com."), vec!["https://example.com"]);
    assert_eq!(urls("see https://example.com, ok"), vec!["https://example.com"]);
    assert_eq!(urls("here: https://example.com;"), vec!["https://example.com"]);
}

#[test]
fn keeps_balanced_parentheses() {
    assert_eq!(
        urls("https://en.wikipedia.org/wiki/Rust_(language)"),
        vec!["https://en.wikipedia.org/wiki/Rust_(language)"],
    );
    assert_eq!(
        urls("(see https://example.com/a)"),
        vec!["https://example.com/a"],
    );
}

#[test]
fn keeps_query_strings() {
    assert_eq!(
        urls("https://example.com/s?a=b&c=d"),
        vec!["https://example.com/s?a=b&c=d"],
    );
}

#[test]
fn finds_several_urls_on_one_line() {
    assert_eq!(
        urls("https://a.test and https://b.test"),
        vec!["https://a.test", "https://b.test"],
    );
}

#[test]
fn ignores_non_links() {
    assert!(urls("https://").is_empty());
    assert!(urls("no links here").is_empty());
    assert!(urls("xhttps://example.com").is_empty());
}

#[test]
fn reports_character_ranges() {
    let found = scan_logical_line("see https://example.com.");
    assert_eq!(found.len(), 1);
    let (start, end, ref url) = found[0];
    assert_eq!(start, 4);
    assert_eq!(end, 4 + url.chars().count());
}

#[test]
fn detects_url_split_across_a_soft_wrap() {
    let cols = 20;
    let mut p = vt100::Parser::new(5, cols, 100);
    p.process(b"see https://example.com/some/deep/path here");

    let links = scan_screen(p.screen());
    assert_eq!(links.spans.len(), 1);
    let span = &links.spans[0];
    assert_eq!(span.url, "https://example.com/some/deep/path");
    // The URL starts on the first row and continues onto the second.
    assert!(span.cells.len() >= 2, "expected a multi-row span: {:?}", span.cells);
    assert_eq!(span.cells[0].1, 4);
    assert!(links.contains(span.cells[0].0, 4));
    assert!(!links.contains(span.cells[0].0, 0));
}

/// Claude Code wraps its own output, so a split URL arrives as two rows with
/// a real newline between them and no wrap flag to follow.
#[test]
fn detects_url_split_across_a_hard_wrap() {
    let cols = 20;
    let mut p = vt100::Parser::new(5, cols, 100);
    // Exactly `cols` characters, then an explicit line break.
    p.process(b"see https://ex.com/x\r\ny/z.png) passed");
    assert_eq!(p.screen().stream_row_wrapped(0), Some(false));

    let links = scan_screen(p.screen());
    assert_eq!(
        links.spans.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
        vec!["https://ex.com/xy/z.png"],
    );
    let cells = &links.spans[0].cells;
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0], (0, 4, 19));
    assert_eq!(cells[1], (1, 0, 6));
}

#[test]
fn keeps_following_prose_out_of_a_hard_wrapped_url() {
    let cols = 20;
    let mut p = vt100::Parser::new(5, cols, 100);
    p.process(b"see https://ex.com/x\r\nQuality gate passed");

    let links = scan_screen(p.screen());
    assert_eq!(
        links.spans.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
        vec!["https://ex.com/x"],
    );
}

/// Claude Code wraps to its own width inside its gutter, so the cut lands
/// several columns short of the terminal's last column.
#[test]
fn detects_a_hard_wrap_short_of_the_last_column() {
    let cols = 70;
    let mut p = vt100::Parser::new(6, cols, 100);
    p.process(
        "  ⎿  ### ![Passed](https://sonarqube.test/static/comBranchPlugi\r\n     n/checks/passed-16px.png) Quality Gate passed"
            .as_bytes(),
    );
    // The cut is well short of the terminal's last column.
    assert_eq!(p.screen().stream_row_wrapped(0), Some(false));

    let links = scan_screen(p.screen());
    assert_eq!(
        links.spans.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
        vec!["https://sonarqube.test/static/comBranchPlugin/checks/passed-16px.png"],
    );
}

/// A line that merely ends with a link is not a wrap: the token below it
/// would have fitted, so the break was the program's own.
#[test]
fn keeps_a_short_line_ending_in_a_url_separate() {
    let cols = 60;
    let mut p = vt100::Parser::new(6, cols, 100);
    p.process(b"see https://ex.com/x\r\ndocs/setup.md explains it");

    let links = scan_screen(p.screen());
    assert_eq!(
        links.spans.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
        vec!["https://ex.com/x"],
    );
}

#[test]
fn does_not_glue_two_links_together() {
    let cols = 20;
    let mut p = vt100::Parser::new(5, cols, 100);
    p.process(b"go https://a.test/xy\r\nhttps://b.test/other");

    let links = scan_screen(p.screen());
    assert_eq!(
        links.spans.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
        vec!["https://a.test/xy", "https://b.test/other"],
    );
}

#[test]
fn follows_a_url_split_across_three_rows() {
    let cols = 20;
    let mut p = vt100::Parser::new(5, cols, 100);
    p.process(b"see https://ex.com/x\r\nabcx/def/ghi/jkl/mno\r\np.png here");

    let links = scan_screen(p.screen());
    assert_eq!(
        links.spans.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
        vec!["https://ex.com/xabcx/def/ghi/jkl/mnop.png"],
    );
}

#[test]
fn follows_a_bare_alphanumeric_url_tail() {
    // An OAuth state value has no URL punctuation, but a long lone token
    // on its own line is still the URL's tail (real case: claude login)
    let cols = 30;
    let mut p = vt100::Parser::new(5, cols, 100);
    p.process(b"https://ex.com/a?x=y&state=8AB\r\nCNlVoy0nQt6Q3uhCsmqjRTTWZCc");

    let links = scan_screen(p.screen());
    assert_eq!(
        links.spans.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
        vec!["https://ex.com/a?x=y&state=8ABCNlVoy0nQt6Q3uhCsmqjRTTWZCc"],
    );
}

#[test]
fn keeps_short_bare_words_out_of_a_hard_wrapped_url() {
    let cols = 20;
    let mut p = vt100::Parser::new(5, cols, 100);
    p.process(b"see https://ex.com/x\r\nDone");

    let links = scan_screen(p.screen());
    assert_eq!(
        links.spans.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
        vec!["https://ex.com/x"],
    );
}

#[test]
fn keeps_a_long_word_with_prose_after_it_out() {
    // Long bare token, but not alone on its line — a sentence, not a tail
    let cols = 40;
    let mut p = vt100::Parser::new(5, cols, 100);
    p.process(b"see https://ex.com/abcdefghijklmnopqrstuvwxy\r\nInternationalisation is hard");

    let links = scan_screen(p.screen());
    assert_eq!(links.spans.len(), 1);
    assert!(links.spans[0].url.ends_with("uvwxy"));
}

/// Wraps `label` in an OSC 8 hyperlink pointing at `url`, ST-terminated.
fn osc8(url: &str, label: &str) -> Vec<u8> {
    format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, label).into_bytes()
}

#[test]
fn finds_osc8_hyperlink() {
    let mut p = vt100::Parser::new(4, 40, 100);
    p.process(b"see ");
    p.process(&osc8("https://example.com/docs", "the docs"));
    p.process(b" ok");

    // The label renders as plain text, with no escape residue.
    assert!(p.screen().contents().starts_with("see the docs ok"));

    let links = scan_screen(p.screen());
    assert_eq!(links.spans.len(), 1);
    assert_eq!(links.spans[0].url, "https://example.com/docs");
    assert_eq!(links.spans[0].cells, vec![(0, 4, 11)]);
    assert!(links.contains(0, 4));
    assert!(links.contains(0, 11));
    assert!(!links.contains(0, 3));
    assert!(!links.contains(0, 12));
}

#[test]
fn accepts_bel_terminated_hyperlink() {
    let mut p = vt100::Parser::new(4, 40, 100);
    p.process(b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07 after");

    let links = scan_screen(p.screen());
    assert_eq!(links.spans.len(), 1);
    assert_eq!(links.spans[0].url, "https://example.com");
    assert_eq!(links.spans[0].cells, vec![(0, 0, 3)]);
}

#[test]
fn hyperlink_label_spanning_soft_wrap_is_one_span() {
    let cols = 10;
    let mut p = vt100::Parser::new(4, cols, 100);
    p.process(&osc8("https://example.com", "aaaaaaaaaaaa"));

    assert_eq!(p.screen().stream_row_wrapped(0), Some(true));
    let links = scan_screen(p.screen());
    assert_eq!(links.spans.len(), 1);
    assert_eq!(links.spans[0].url, "https://example.com");
    assert_eq!(links.spans[0].cells, vec![(0, 0, 9), (1, 0, 1)]);
}

#[test]
fn hyperlink_survives_scrolling_into_scrollback() {
    let mut p = vt100::Parser::new(3, 40, 100);
    p.process(&osc8("https://example.com", "link"));
    p.process(b"\r\nsecond\r\nthird\r\nfourth");

    // The label has scrolled off the top; stream row 0 still carries it.
    let links = scan_screen(p.screen());
    assert!(links.spans.is_empty(), "not visible, so not scanned");
    assert_eq!(
        p.screen().stream_cell(0, 0).map(vt100::Cell::link_id),
        Some(1)
    );

    p.screen_mut().set_scrollback(1);
    let links = scan_screen(p.screen());
    assert_eq!(links.spans.len(), 1);
    assert_eq!(links.spans[0].cells, vec![(0, 0, 3)]);
}

#[test]
fn close_sequence_ends_the_link() {
    let mut p = vt100::Parser::new(4, 40, 100);
    p.process(&osc8("https://example.com", "link"));
    p.process(b"after");

    let ids: Vec<u16> = (0..9)
        .map(|c| p.screen().cell(0, c).unwrap().link_id())
        .collect();
    assert_eq!(ids, vec![1, 1, 1, 1, 0, 0, 0, 0, 0]);
}

#[test]
fn ignores_hyperlinks_summoner_cannot_open() {
    for url in ["file:///etc/passwd", "mailto:a@example.com", "javascript:alert(1)"] {
        let mut p = vt100::Parser::new(4, 40, 100);
        p.process(&osc8(url, "click"));
        assert!(
            scan_screen(p.screen()).spans.is_empty(),
            "{url} should not render as clickable"
        );
    }
}

#[test]
fn rejects_malformed_hyperlink_targets() {
    // A URI carrying whitespace or controls never becomes a link.
    let mut p = vt100::Parser::new(4, 40, 100);
    p.process(b"\x1b]8;;https://exa mple.com\x1b\\click\x1b]8;;\x1b\\");
    assert!(scan_screen(p.screen()).spans.is_empty());
    assert_eq!(p.screen().cell(0, 0).unwrap().link_id(), 0);
}

#[test]
fn rejoins_uri_containing_a_semicolon() {
    let mut p = vt100::Parser::new(4, 40, 100);
    p.process(b"\x1b]8;;https://example.com/a;b\x1b\\click\x1b]8;;\x1b\\");

    let links = scan_screen(p.screen());
    assert_eq!(links.spans.len(), 1);
    assert_eq!(links.spans[0].url, "https://example.com/a;b");
}

#[test]
fn ignores_osc8_params_but_honours_the_uri() {
    let mut p = vt100::Parser::new(4, 40, 100);
    p.process(b"\x1b]8;id=xyz;https://example.com\x1b\\click\x1b]8;;\x1b\\");

    let links = scan_screen(p.screen());
    assert_eq!(links.spans.len(), 1);
    assert_eq!(links.spans[0].url, "https://example.com");
}

#[test]
fn sgr_reset_does_not_break_a_link_but_erase_clears_it() {
    let mut p = vt100::Parser::new(4, 40, 100);
    p.process(b"\x1b]8;;https://example.com\x1b\\ab\x1b[0mcd\x1b]8;;\x1b\\");
    let links = scan_screen(p.screen());
    assert_eq!(links.spans.len(), 1, "SGR reset must not close the link");
    assert_eq!(links.spans[0].cells, vec![(0, 0, 3)]);

    // Erasing the line drops the ids with the text.
    p.process(b"\r\x1b[K");
    assert_eq!(p.screen().cell(0, 0).unwrap().link_id(), 0);
    assert!(scan_screen(p.screen()).spans.is_empty());
}

#[test]
fn same_target_twice_shares_one_span() {
    let mut p = vt100::Parser::new(4, 40, 100);
    p.process(&osc8("https://example.com", "one"));
    p.process(b" gap ");
    p.process(&osc8("https://example.com", "two"));

    // Equal targets intern to one id, so both runs belong to the same link.
    let links = scan_screen(p.screen());
    assert_eq!(links.spans.len(), 1);
    assert_eq!(links.spans[0].cells, vec![(0, 0, 2), (0, 8, 10)]);
}

#[test]
fn osc8_span_wins_over_a_bare_url_in_the_label() {
    let mut p = vt100::Parser::new(4, 60, 100);
    p.process(&osc8("https://real.example.com", "https://decoy.example.com"));

    let links = scan_screen(p.screen());
    // Both scanners match these cells; the declared target is what a click gets.
    assert_eq!(
        links.span_at(0, 0).map(|s| s.url.as_str()),
        Some("https://real.example.com")
    );
}

#[test]
fn finds_osc8_and_bare_urls_together() {
    let mut p = vt100::Parser::new(4, 60, 100);
    p.process(&osc8("https://example.com/a", "label"));
    p.process(b" and https://example.com/b");

    let links = scan_screen(p.screen());
    let mut urls: Vec<&str> = links.spans.iter().map(|s| s.url.as_str()).collect();
    urls.sort_unstable();
    assert_eq!(urls, vec!["https://example.com/a", "https://example.com/b"]);
}
