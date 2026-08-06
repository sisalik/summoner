use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use summoner::session::{Session, SessionState};
use summoner::ui::session_switcher::{SessionSwitcher, SwitcherAction};

fn session(dir: &str, state: SessionState) -> Session {
    let mut s = Session::new(dir.to_string(), 0, "bipedal".to_string());
    s.state = state;
    s
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_str(switcher: &mut SessionSwitcher, text: &str) {
    for c in text.chars() {
        switcher.handle_key(key(KeyCode::Char(c)));
    }
}

/// Three sessions: idx 0 least recent, idx 2 most recent (the current one).
fn three_sessions() -> Vec<Session> {
    vec![
        session("/home/u/alpha", SessionState::Idle),
        session("/home/u/beta", SessionState::Idle),
        session("/home/u/gamma", SessionState::Idle),
    ]
}

#[test]
fn empty_query_orders_by_mru() {
    let sessions = three_sessions();
    let switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    assert_eq!(switcher.visible_sessions(), vec![2, 1, 0]);
}

#[test]
fn default_selection_is_previous_session() {
    let sessions = three_sessions();
    let switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    // Most recent non-current session is idx 1 — Enter toggles back to it
    assert_eq!(switcher.selected_session(), Some(1));
}

#[test]
fn default_selection_falls_back_with_single_session() {
    let sessions = vec![session("/home/u/solo", SessionState::Idle)];
    let switcher = SessionSwitcher::new(&sessions, &[1], Some(0));
    assert_eq!(switcher.selected_session(), Some(0));
}

#[test]
fn waiting_sessions_pinned_first_on_empty_query() {
    let mut sessions = three_sessions();
    sessions[0].state = SessionState::Waiting;
    let switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    // alpha (waiting) pinned above the MRU-ordered rest
    assert_eq!(switcher.visible_sessions(), vec![0, 2, 1]);
    // but the default selection is still the previous session, not the pin
    assert_eq!(switcher.selected_session(), Some(1));
}

#[test]
fn filter_matches_name() {
    let sessions = three_sessions();
    let mut switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    type_str(&mut switcher, "beta");
    assert_eq!(switcher.visible_sessions(), vec![1]);
    assert_eq!(switcher.selected_session(), Some(1));
}

#[test]
fn filter_matches_path() {
    let sessions = vec![
        session("/home/u/work/api", SessionState::Idle),
        session("/home/u/play/game", SessionState::Idle),
    ];
    let mut switcher = SessionSwitcher::new(&sessions, &[1, 2], None);
    type_str(&mut switcher, "work");
    assert_eq!(switcher.visible_sessions(), vec![0]);
}

#[test]
fn typing_resets_selection_to_best_match() {
    let sessions = three_sessions();
    let mut switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    switcher.handle_key(key(KeyCode::Down));
    type_str(&mut switcher, "a");
    // "a" matches all three; selection snapped back to the top row
    let visible = switcher.visible_sessions();
    assert_eq!(switcher.selected_session(), Some(visible[0]));
}

#[test]
fn clearing_query_restores_default_selection() {
    let sessions = three_sessions();
    let mut switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    type_str(&mut switcher, "gamma");
    switcher.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(switcher.selected_session(), Some(1));
}

#[test]
fn enter_returns_select_even_for_current_session() {
    let sessions = three_sessions();
    let mut switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    type_str(&mut switcher, "gamma");
    match switcher.handle_key(key(KeyCode::Enter)) {
        SwitcherAction::Select(idx) => assert_eq!(idx, 2),
        _ => panic!("expected Select"),
    }
}

#[test]
fn esc_cancels_and_no_match_enter_is_noop() {
    let sessions = three_sessions();
    let mut switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    type_str(&mut switcher, "zzzz");
    assert!(switcher.visible_sessions().is_empty());
    assert!(matches!(
        switcher.handle_key(key(KeyCode::Enter)),
        SwitcherAction::None
    ));
    assert!(matches!(
        switcher.handle_key(key(KeyCode::Esc)),
        SwitcherAction::Cancel
    ));
}

#[test]
fn arrow_navigation_moves_selection() {
    let sessions = three_sessions();
    let mut switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    // Order is [2, 1, 0], default selection on row 1 (session 1)
    switcher.handle_key(key(KeyCode::Down));
    assert_eq!(switcher.selected_session(), Some(0));
    switcher.handle_key(key(KeyCode::Down));
    assert_eq!(switcher.selected_session(), Some(0));
    switcher.handle_key(key(KeyCode::Up));
    switcher.handle_key(key(KeyCode::Up));
    assert_eq!(switcher.selected_session(), Some(2));
    switcher.handle_key(key(KeyCode::Up));
    assert_eq!(switcher.selected_session(), Some(2));
}

#[test]
fn ctrl_backspace_deletes_last_word() {
    let sessions = three_sessions();
    let mut switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    type_str(&mut switcher, "beta");
    switcher.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::CONTROL));
    // Query emptied — full MRU list and default selection restored
    assert_eq!(switcher.visible_sessions(), vec![2, 1, 0]);
    assert_eq!(switcher.selected_session(), Some(1));
}

#[test]
fn ctrl_h_also_deletes_last_word() {
    // Legacy terminals send Ctrl+Backspace as 0x08 = Ctrl+H
    let sessions = three_sessions();
    let mut switcher = SessionSwitcher::new(&sessions, &[1, 2, 3], Some(2));
    type_str(&mut switcher, "beta");
    switcher.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL));
    assert_eq!(switcher.visible_sessions(), vec![2, 1, 0]);
}

#[test]
fn delete_last_word_stops_at_separators() {
    use summoner::ui::delete_last_word;
    let mut q = String::from("foo bar");
    delete_last_word(&mut q);
    assert_eq!(q, "foo ");
    let mut q = String::from("~/dev/summoner/");
    delete_last_word(&mut q);
    assert_eq!(q, "~/dev/");
    let mut q = String::from("word");
    delete_last_word(&mut q);
    assert_eq!(q, "");
    let mut q = String::new();
    delete_last_word(&mut q);
    assert_eq!(q, "");
}

#[test]
fn fkey_positions_follow_session_order() {
    let mut sessions = Vec::new();
    for i in 0..13 {
        sessions.push(session(&format!("/home/u/proj{i:02}"), SessionState::Idle));
    }
    let focus: Vec<u64> = (0..13).collect();
    let switcher = SessionSwitcher::new(&sessions, &focus, None);
    assert_eq!(switcher.fkey_pos(0), Some(0));
    assert_eq!(switcher.fkey_pos(10), Some(10));
    // Positions beyond F11 carry no F-key label
    assert_eq!(switcher.fkey_pos(11), None);
    assert_eq!(switcher.fkey_pos(12), None);
}
