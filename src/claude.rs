use crate::session::SessionState;

pub fn detect_claude_state(screen: &vt100::Screen) -> Option<SessionState> {
    let (rows, _cols) = screen.size();
    let search_rows = 4.min(rows);
    for row_offset in 0..search_rows {
        let row = rows - 1 - row_offset;
        let text = row_text(screen, row);
        let trimmed = text.trim();

        if trimmed.contains("esc to interrupt") {
            return Some(SessionState::Working);
        }
        if trimmed.contains("Esc to cancel") {
            return Some(SessionState::Waiting);
        }
    }
    None
}

fn row_text(screen: &vt100::Screen, row: u16) -> String {
    let (_rows, cols) = screen.size();
    screen.rows(0, cols).nth(row as usize).unwrap_or_default()
}

pub fn find_conversation_id(pid: u32) -> Option<String> {
    let sessions_dir = dirs::home_dir()?.join(".claude/sessions");
    let pid_file = sessions_dir.join(format!("{}.json", pid));
    if pid_file.exists() {
        let content = std::fs::read_to_string(&pid_file).ok()?;
        extract_json_field(&content, "session_id")
            .or_else(|| extract_json_field(&content, "id"))
    } else {
        None
    }
}

fn extract_json_field(json: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\"", field);
    let pos = json.find(&pattern)?;
    let after_key = &json[pos + pattern.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    let after_quote = after_colon.strip_prefix('"')?;
    let end = after_quote.find('"')?;
    Some(after_quote[..end].to_string())
}
