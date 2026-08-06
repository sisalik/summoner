use summoner::terminal::PtySession;
use std::time::Duration;
use std::thread;

#[test]
fn pty_session_spawns_and_reads_output() {
    let session = PtySession::spawn("/bin/bash", "/tmp", 24, 80).unwrap();
    session.write(b"echo SUMMONER_TEST\r\n").unwrap();
    thread::sleep(Duration::from_millis(200));
    let output = session.read_available();
    let text: String = output.iter().map(|b| String::from_utf8_lossy(b).to_string()).collect();
    assert!(text.contains("SUMMONER_TEST"), "Expected output to contain SUMMONER_TEST, got: {}", text);
}

#[test]
fn pty_session_has_valid_pid() {
    let session = PtySession::spawn("/bin/bash", "/tmp", 24, 80).unwrap();
    assert!(session.pid().is_some());
}

#[test]
fn pty_session_can_resize() {
    let session = PtySession::spawn("/bin/bash", "/tmp", 24, 80).unwrap();
    session.resize(40, 120).unwrap();
}

#[test]
fn bracketed_paste_mode_tracks_child_requests() {
    // The paste path only wraps in \x1b[200~..201~ when the child app
    // enabled bracketed paste; guard the vendored vt100 API it relies on
    let mut parser = vt100::Parser::new(24, 80, 0);
    assert!(!parser.screen().bracketed_paste());
    parser.process(b"\x1b[?2004h");
    assert!(parser.screen().bracketed_paste());
    parser.process(b"\x1b[?2004l");
    assert!(!parser.screen().bracketed_paste());
}
