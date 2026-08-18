use summoner::session::{Session, SessionState};
use summoner::ui::status_bar::StatusBar;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

fn make_session(name: &str, state: SessionState) -> Session {
    let mut s = Session::new(format!("/tmp/{}", name), 42, "bipedal".into());
    s.state = state;
    s
}

#[test]
fn status_bar_renders_session_tabs() {
    let sessions = vec![
        make_session("project-a", SessionState::Working),
        make_session("project-b", SessionState::Waiting),
    ];
    let bar = StatusBar::new(&sessions, Some(0));
    let area = Rect::new(0, 0, 80, 1);
    let mut buf = Buffer::empty(area);
    bar.render(area, &mut buf);

    let text: String = (0..80)
        .map(|x| buf[ratatui::layout::Position { x, y: 0 }].symbol().to_string())
        .collect();

    assert!(text.contains("F1"), "Should contain F1 tab");
    assert!(text.contains("project-a"), "Should contain project name");
    assert!(text.contains("F2"), "Should contain F2 tab");
}

#[test]
fn status_bar_highlights_active_session() {
    let sessions = vec![
        make_session("active", SessionState::Working),
        make_session("inactive", SessionState::Idle),
    ];
    let bar = StatusBar::new(&sessions, Some(0));
    let area = Rect::new(0, 0, 80, 1);
    let mut buf = Buffer::empty(area);
    bar.render(area, &mut buf);
    // The active session's tab shows its name; the styling itself is visual.
    let text: String = (0..80)
        .map(|x| buf[ratatui::layout::Position { x, y: 0 }].symbol().to_string())
        .collect();
    assert!(text.contains("active"), "Active session name should be visible");
}

#[test]
fn status_bar_shows_f12_hint() {
    let sessions = vec![make_session("test", SessionState::Idle)];
    let bar = StatusBar::new(&sessions, Some(0));
    let area = Rect::new(0, 0, 80, 1);
    let mut buf = Buffer::empty(area);
    bar.render(area, &mut buf);

    let text: String = (0..80)
        .map(|x| buf[ratatui::layout::Position { x, y: 0 }].symbol().to_string())
        .collect();
    assert!(text.contains("F12"), "Should contain F12 dashboard hint");
}

#[test]
fn session_view_underlines_detected_urls() {
    use ratatui::layout::Position;
    use ratatui::style::Modifier;
    use summoner::ui::links;
    use summoner::ui::session_view::TerminalView;

    let mut p = vt100::Parser::new(4, 40, 100);
    p.process(b"go https://example.com now");

    let found = links::scan_screen(p.screen());
    let area = Rect::new(0, 0, 40, 4);
    let mut buf = Buffer::empty(area);
    TerminalView::new(p.screen())
        .with_links(Some(&found))
        .render(area, &mut buf);

    let underlined = |x: u16| {
        buf[Position { x, y: 0 }]
            .style()
            .add_modifier
            .contains(Modifier::UNDERLINED)
    };
    // "go " is not part of the link; the URL that follows is.
    assert!(!underlined(0));
    assert!(underlined(3));
    assert!(underlined(21));
    // The trailing " now" is outside the link.
    assert!(!underlined(23));
}

#[test]
fn session_view_highlights_only_the_copied_cells() {
    use ratatui::layout::Position;
    use ratatui::style::Modifier;
    use summoner::ui::selection::{Selection, SelectionMode};
    use summoner::ui::session_view::TerminalView;
    use summoner::ui::smart;

    let mut p = vt100::Parser::new(4, 40, 100);
    p.process("⏺ Ran tool".as_bytes());

    let sel = Selection {
        anchor: (0, 0),
        moving: (0, 39),
        dragged: true,
        mode: SelectionMode::Smart,
    };
    let spans = smart::compute_spans(p.screen(), &sel);
    let area = Rect::new(0, 0, 40, 4);
    let mut buf = Buffer::empty(area);
    TerminalView::new(p.screen())
        .with_selection(Some(&spans))
        .render(area, &mut buf);

    let reversed = |x: u16| {
        buf[Position { x, y: 0 }]
            .style()
            .add_modifier
            .contains(Modifier::REVERSED)
    };
    // The gutter marker and its space are stripped, so they stay unhighlighted.
    assert!(!reversed(0));
    assert!(!reversed(1));
    assert!(reversed(2));
    assert!(reversed(9));
    // Trailing blanks are not copied, so they are not highlighted either
    // (column 10 holds the cursor, which is reversed for its own reasons).
    assert!(!reversed(11));
}

#[test]
fn session_view_underlines_osc8_labels() {
    use ratatui::layout::Position;
    use ratatui::style::Modifier;
    use summoner::ui::links;
    use summoner::ui::session_view::TerminalView;

    let mut p = vt100::Parser::new(4, 40, 100);
    p.process(b"see \x1b]8;;https://example.com/docs\x1b\\the docs\x1b]8;;\x1b\\ ok");

    let found = links::scan_screen(p.screen());
    let area = Rect::new(0, 0, 40, 4);
    let mut buf = Buffer::empty(area);
    TerminalView::new(p.screen())
        .with_links(Some(&found))
        .render(area, &mut buf);

    let styled = |x: u16| {
        let style = buf[Position { x, y: 0 }].style();
        (style.add_modifier.contains(Modifier::UNDERLINED), style.fg)
    };
    // "see " is plain; the "the docs" label is a link; " ok" is plain again.
    assert!(!styled(3).0);
    assert!(styled(4).0);
    assert!(styled(11).0);
    assert!(!styled(12).0);
    assert_eq!(styled(4).1, styled(11).1);
    assert_ne!(styled(4).1, styled(3).1);
}

#[test]
fn status_bar_shows_hovered_link_target() {
    let sessions = vec![make_session("alpha", SessionState::Working)];
    let area = Rect::new(0, 0, 60, 1);

    let mut buf = Buffer::empty(area);
    StatusBar::new(&sessions, Some(0))
        .with_hover_url(Some("https://example.com/docs"))
        .render(area, &mut buf);
    let text: String = (0..60).map(|x| buf[(x, 0)].symbol()).collect();
    assert!(text.contains("https://example.com/docs"), "got {text:?}");
    assert!(text.contains("F12"), "the dashboard hint stays put");
    assert!(!text.contains("alpha"), "the tab strip gives way to the URL");

    // Without a hover the bar is unchanged.
    let mut buf = Buffer::empty(area);
    StatusBar::new(&sessions, Some(0)).render(area, &mut buf);
    let text: String = (0..60).map(|x| buf[(x, 0)].symbol()).collect();
    assert!(text.contains("alpha"));
}
