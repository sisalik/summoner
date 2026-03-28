use summoner::claude::detect_claude_state;
use summoner::session::SessionState;

fn make_screen(lines: &[&str]) -> vt100::Parser {
    let mut parser = vt100::Parser::new(24, 80, 0);
    for (i, line) in lines.iter().enumerate() {
        let cmd = format!("\x1b[{};1H{}", i + 1, line);
        parser.process(cmd.as_bytes());
    }
    parser
}

#[test]
fn detects_working_state() {
    let mut lines = vec!["Claude is thinking..."];
    for _ in 0..22 { lines.push(""); }
    lines.push("                                                    esc to interrupt");
    let parser = make_screen(&lines);
    let state = detect_claude_state(parser.screen());
    assert_eq!(state, Some(SessionState::Working));
}

#[test]
fn detects_waiting_state() {
    let mut lines = vec!["Do you want to proceed?"];
    for _ in 0..22 { lines.push(""); }
    lines.push("                                                    Esc to cancel");
    let parser = make_screen(&lines);
    let state = detect_claude_state(parser.screen());
    assert_eq!(state, Some(SessionState::Waiting));
}

#[test]
fn returns_none_for_plain_shell() {
    let parser = make_screen(&[
        "siim@host:~/dev$ ls",
        "file1.rs  file2.rs",
        "siim@host:~/dev$",
    ]);
    let state = detect_claude_state(parser.screen());
    assert_eq!(state, None);
}
