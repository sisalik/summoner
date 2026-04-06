/// Check if a `claude` process is running as a descendant of the given PID.
pub fn find_claude_child(shell_pid: u32) -> Option<u32> {
    find_descendant_by_name(shell_pid, "claude")
}

/// Check if a process (or any of its descendants named "claude") is stopped (Ctrl+Z).
pub fn is_claude_stopped(shell_pid: u32) -> bool {
    if let Some(claude_pid) = find_claude_child(shell_pid) {
        return is_process_stopped(claude_pid);
    }
    false
}

fn is_process_stopped(pid: u32) -> bool {
    let path = format!("/proc/{}/stat", pid);
    if let Ok(stat) = std::fs::read_to_string(&path) {
        // Format: pid (comm) state ... — state is the first char after the last ')'
        if let Some(after_comm) = stat.rfind(')') {
            let rest = &stat[after_comm + 1..];
            let state = rest.trim_start().chars().next();
            return state == Some('T');
        }
    }
    false
}

fn find_descendant_by_name(pid: u32, name: &str) -> Option<u32> {
    let children = read_children(pid);
    for child in children {
        if process_name(child).as_deref() == Some(name) {
            return Some(child);
        }
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

    if let Some(claude_pid) = find_claude_child(shell_pid) {
        let pid_file = sessions_dir.join(format!("{}.json", claude_pid));
        if pid_file.exists() {
            let content = std::fs::read_to_string(&pid_file).ok()?;
            return extract_json_field(&content, "session_id")
                .or_else(|| extract_json_field(&content, "id"));
        }
    }

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
