use summoner::ui::selection::{self, Selection, SelectionMode};
use summoner::ui::smart;

const ROWS: u16 = 5;
const COLS: u16 = 20;

fn parser(scrollback: usize) -> vt100::Parser {
    vt100::Parser::new(ROWS, COLS, scrollback)
}

fn extract(screen: &vt100::Screen, sel: &Selection) -> String {
    smart::compute_spans(screen, sel).text().to_string()
}

fn write_lines(parser: &mut vt100::Parser, lines: &[&str]) {
    for line in lines {
        parser.process(line.as_bytes());
        parser.process(b"\r\n");
    }
}

#[test]
fn scrolled_lines_counts_rows_pushed_into_scrollback() {
    let mut p = parser(100);
    assert_eq!(p.screen().scrolled_lines(), 0);

    // The first ROWS lines fill the screen; only what scrolls off counts.
    write_lines(&mut p, &["a", "b", "c", "d", "e"]);
    assert_eq!(p.screen().scrolled_lines(), 1);

    write_lines(&mut p, &["f", "g"]);
    assert_eq!(p.screen().scrolled_lines(), 3);
}

#[test]
fn scrolled_lines_keeps_counting_after_scrollback_evicts_rows() {
    let mut p = parser(4);
    let lines: Vec<String> = (0..40).map(|i| format!("line{}", i)).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&mut p, &refs);

    let scrolled = p.screen().scrolled_lines();
    assert_eq!(scrolled, 36);

    // The oldest retained row is scrolled - scrollback_len.
    assert!(p.screen().stream_row_contents(scrolled - 5, 0, COLS).is_none());
    assert_eq!(
        p.screen().stream_row_contents(scrolled - 4, 0, COLS).as_deref(),
        Some("line32"),
    );
}

#[test]
fn stream_row_stays_with_its_content_while_output_streams() {
    let mut p = parser(100);
    write_lines(&mut p, &["marker"]);
    // "marker" is on the top visible row right now.
    let marker_row = selection::viewport_top(p.screen());
    assert_eq!(
        p.screen().stream_row_contents(marker_row, 0, COLS).as_deref(),
        Some("marker"),
    );

    let lines: Vec<String> = (0..50).map(|i| format!("noise{}", i)).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&mut p, &refs);

    // Same stream row, same content — even though it is far up the scrollback.
    assert_eq!(
        p.screen().stream_row_contents(marker_row, 0, COLS).as_deref(),
        Some("marker"),
    );
}

#[test]
fn viewport_top_tracks_scrollback_position() {
    let mut p = parser(100);
    let lines: Vec<String> = (0..20).map(|i| format!("l{}", i)).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&mut p, &refs);

    let live_top = selection::viewport_top(p.screen());
    p.screen_mut().set_scrollback(7);
    assert_eq!(selection::viewport_top(p.screen()), live_top - 7);

    // Row drawn at viewport row v is viewport_top + v.
    for v in 0..ROWS {
        let expected = p.screen().cell(v, 0).unwrap().contents();
        let stream = selection::viewport_top(p.screen()) + u64::from(v);
        let contents = p.screen().stream_row_contents(stream, 0, COLS).unwrap();
        assert_eq!(contents.chars().next().unwrap().to_string(), expected);
    }
}

#[test]
fn extraction_spans_more_than_one_viewport() {
    let mut p = parser(100);
    let lines: Vec<String> = (0..30).map(|i| format!("line{:02}", i)).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&mut p, &refs);

    // line00 is the very first row of the stream.
    let sel = Selection {
        anchor: (0, 0),
        moving: (14, COLS - 1),
        dragged: true,
        mode: SelectionMode::Raw,
    };
    let text = extract(p.screen(), &sel);

    let expected: Vec<String> = (0..15).map(|i| format!("line{:02}", i)).collect();
    assert_eq!(text, expected.join("\n"));
}

#[test]
fn extraction_joins_soft_wrapped_rows() {
    let mut p = parser(100);
    let long = "x".repeat(usize::from(COLS) + 5);
    p.process(long.as_bytes());

    let top = selection::viewport_top(p.screen());
    let sel = Selection {
        anchor: (top, 0),
        moving: (top + 1, COLS - 1),
        dragged: true,
        mode: SelectionMode::Raw,
    };
    let text = extract(p.screen(), &sel);
    assert_eq!(text, long);
}

#[test]
fn select_word_treats_underscores_as_part_of_the_word() {
    let mut p = parser(100);
    write_lines(&mut p, &["let MAX_RETRY_COUNT", "x = some_var + 1"]);

    let top = selection::viewport_top(p.screen());
    // Click inside MAX_RETRY_COUNT (starts at column 4).
    let sel = selection::select_word(p.screen(), top, 8);
    assert_eq!(sel.anchor, (top, 4));
    assert_eq!(sel.moving, (top, 18));
    assert_eq!(extract(p.screen(), &sel), "MAX_RETRY_COUNT");

    // And the snake_case identifier on the next line.
    let sel = selection::select_word(p.screen(), top + 1, 6);
    assert_eq!(extract(p.screen(), &sel), "some_var");
}

#[test]
fn select_word_still_stops_at_other_punctuation() {
    let mut p = parser(100);
    p.process(b"foo.bar baz");
    let top = selection::viewport_top(p.screen());

    let sel = selection::select_word(p.screen(), top, 1);
    assert_eq!(extract(p.screen(), &sel), "foo");
}
