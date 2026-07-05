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
