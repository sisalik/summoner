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
