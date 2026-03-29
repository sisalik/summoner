use crate::session::SessionState;

pub fn detect_claude_state(screen: &vt100::Screen) -> Option<SessionState> {
    let (rows, cols) = screen.size();

    // Collect last 10 non-empty lines, bottom-up
    let mut lines: Vec<String> = Vec::new();
    for row in (0..rows).rev() {
        let text = row_text(screen, row, cols);
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
            if lines.len() >= 10 {
                break;
            }
        }
    }

    if lines.is_empty() {
        return None;
    }

    // Check last non-empty line for "Esc to cancel" -> Waiting/Input
    if lines[0].contains("Esc to cancel") {
        return Some(SessionState::Waiting);
    }

    // Check any of the last 10 lines for spinner + ellipsis -> Working
    for line in &lines {
        // Skip box-drawing lines (UI chrome)
        if line.starts_with(|c: char| "│├└─┌┐┘┤┬┴┼╭╰╮╯".contains(c)) {
            continue;
        }
        if has_spinner_and_ellipsis(line) {
            return Some(SessionState::Working);
        }
    }

    // Check any of the last 10 lines for ❯ followed by digit -> Waiting (selection menu)
    for line in &lines {
        if is_selection_menu(line) {
            return Some(SessionState::Waiting);
        }
    }

    // Check if last non-empty line is just the ❯ prompt -> Idle (Claude is running but idle)
    let last = &lines[0];
    // Normalize NBSP (U+00A0) to regular space
    let normalized = last.replace('\u{00A0}', " ");
    let normalized = normalized.trim();
    if normalized == "❯" || normalized == ">" || normalized.starts_with("❯ ") {
        return Some(SessionState::Idle);
    }

    // Also check for older patterns as fallback
    for line in &lines {
        if line.contains("esc to interrupt") || line.contains("ctrl+c to interrupt") {
            return Some(SessionState::Working);
        }
    }

    // Check for Claude Code UI chrome (borders, input box)
    for line in &lines {
        if line.contains("Claude Code") || line.contains("claude.ai") {
            return Some(SessionState::Idle);
        }
        // Claude's input box uses rounded corners
        if line.starts_with('╭') || line.starts_with('╰') {
            return Some(SessionState::Idle);
        }
    }

    // No Claude Code patterns detected
    None
}

fn has_spinner_and_ellipsis(line: &str) -> bool {
    let has_ellipsis = line.contains('\u{2026}'); // …
    if !has_ellipsis {
        return false;
    }
    // Check for spinner characters at the start of the line (after trimming)
    let first_char = line.chars().next();
    match first_char {
        Some(c) => is_spinner_char(c),
        None => false,
    }
}

fn is_spinner_char(c: char) -> bool {
    matches!(c,
        '\u{2720}'..='\u{2767}' |  // Dingbats (✠✡✢✣...✽✾✿❀...❧)
        '\u{23FA}' |                 // ⏺ (record symbol)
        '\u{00B7}' |                 // · (middle dot)
        '\u{2800}'..='\u{28FF}'      // Braille dots (older spinner)
    )
}

fn is_selection_menu(line: &str) -> bool {
    // ❯ followed by a digit
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix('❯') {
        let rest = rest.trim_start();
        rest.starts_with(|c: char| c.is_ascii_digit())
    } else {
        false
    }
}

fn row_text(screen: &vt100::Screen, row: u16, cols: u16) -> String {
    screen.rows(0, cols).nth(row as usize).unwrap_or_default()
}

/// Check if a `claude` process is running as a descendant of the given PID.
/// Returns the claude process PID if found.
pub fn find_claude_child(shell_pid: u32) -> Option<u32> {
    find_descendant_by_name(shell_pid, "claude")
}

fn find_descendant_by_name(pid: u32, name: &str) -> Option<u32> {
    let children = read_children(pid);
    for child in children {
        if process_name(child).as_deref() == Some(name) {
            return Some(child);
        }
        // Recurse into grandchildren
        if let Some(found) = find_descendant_by_name(child, name) {
            return Some(found);
        }
    }
    None
}

fn read_children(pid: u32) -> Vec<u32> {
    let path = format!("/proc/{}/task/{}/children", pid, pid);
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect()
}

fn process_name(pid: u32) -> Option<String> {
    let path = format!("/proc/{}/comm", pid);
    std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
}

/// Find conversation ID by looking for a claude child process and its session file.
pub fn find_conversation_id(shell_pid: u32) -> Option<String> {
    let sessions_dir = dirs::home_dir()?.join(".claude/sessions");

    // First try: look for session file matching claude's PID
    if let Some(claude_pid) = find_claude_child(shell_pid) {
        let pid_file = sessions_dir.join(format!("{}.json", claude_pid));
        if pid_file.exists() {
            let content = std::fs::read_to_string(&pid_file).ok()?;
            return extract_json_field(&content, "session_id")
                .or_else(|| extract_json_field(&content, "id"));
        }
    }

    // Fallback: try shell PID directly (in case claude replaces the shell process)
    let pid_file = sessions_dir.join(format!("{}.json", shell_pid));
    if pid_file.exists() {
        let content = std::fs::read_to_string(&pid_file).ok()?;
        return extract_json_field(&content, "session_id")
            .or_else(|| extract_json_field(&content, "id"));
    }

    None
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
