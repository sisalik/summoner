use summoner::stats::{format_xp, level_from_tokens, parse_jsonl_stats, find_jsonl_path, tool_display};
use std::io::Write;

#[test]
fn format_xp_below_1000() {
    assert_eq!(format_xp(0), "✦0");
    assert_eq!(format_xp(847), "✦847");
    assert_eq!(format_xp(999), "✦999");
}

#[test]
fn format_xp_thousands() {
    assert_eq!(format_xp(12_400), "✦12.4k");
    assert_eq!(format_xp(1_000), "✦1.0k");
    assert_eq!(format_xp(999_999), "✦1000.0k");
}

#[test]
fn format_xp_millions() {
    assert_eq!(format_xp(1_200_000), "✦1.2M");
    assert_eq!(format_xp(1_000_000), "✦1.0M");
    assert_eq!(format_xp(10_000_000), "✦10.0M");
}

#[test]
fn level_from_tokens_thresholds() {
    assert_eq!(level_from_tokens(0), 1);
    assert_eq!(level_from_tokens(9_999), 1);
    assert_eq!(level_from_tokens(10_000), 2);
    assert_eq!(level_from_tokens(49_999), 2);
    assert_eq!(level_from_tokens(50_000), 3);
    assert_eq!(level_from_tokens(149_999), 3);
    assert_eq!(level_from_tokens(150_000), 4);
    assert_eq!(level_from_tokens(499_999), 4);
    assert_eq!(level_from_tokens(500_000), 5);
    assert_eq!(level_from_tokens(999_999), 5);
    assert_eq!(level_from_tokens(1_000_000), 6);
    assert_eq!(level_from_tokens(2_499_999), 6);
    assert_eq!(level_from_tokens(2_500_000), 7);
    assert_eq!(level_from_tokens(4_999_999), 7);
    assert_eq!(level_from_tokens(5_000_000), 8);
    assert_eq!(level_from_tokens(9_999_999), 8);
    assert_eq!(level_from_tokens(10_000_000), 9);
    assert_eq!(level_from_tokens(24_999_999), 9);
    assert_eq!(level_from_tokens(25_000_000), 10);
    assert_eq!(level_from_tokens(100_000_000), 10);
}

#[test]
fn parse_jsonl_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.jsonl");
    std::fs::write(&path, "").unwrap();
    let (tokens, messages, offset) = parse_jsonl_stats(&path, 0);
    assert_eq!(tokens, 0);
    assert_eq!(messages, 0);
    assert_eq!(offset, 0);
}

#[test]
fn parse_jsonl_counts_tokens_and_messages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.jsonl");
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, r#"{{"type":"user","message":{{"role":"user","content":"hello"}}}}"#).unwrap();
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"hi","usage":{{"inputTokens":100,"outputTokens":50,"cacheReadInputTokens":20,"cacheCreationInputTokens":10}}}}}}"#).unwrap();
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"ok","usage":{{"inputTokens":200,"outputTokens":100,"cacheReadInputTokens":0,"cacheCreationInputTokens":0}}}}}}"#).unwrap();

    let (tokens, messages, offset) = parse_jsonl_stats(&path, 0);
    assert_eq!(tokens, 450); // 100+50 + 200+100
    assert_eq!(messages, 3);
    assert!(offset > 0);
}

#[test]
fn parse_jsonl_incremental_from_offset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.jsonl");
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, r#"{{"type":"user","message":{{"role":"user","content":"hello"}}}}"#).unwrap();
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"hi","usage":{{"inputTokens":100,"outputTokens":50,"cacheReadInputTokens":0,"cacheCreationInputTokens":0}}}}}}"#).unwrap();

    let (tokens1, messages1, offset1) = parse_jsonl_stats(&path, 0);
    assert_eq!(tokens1, 150);
    assert_eq!(messages1, 2);

    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"more","usage":{{"inputTokens":300,"outputTokens":200,"cacheReadInputTokens":0,"cacheCreationInputTokens":0}}}}}}"#).unwrap();

    let (tokens2, messages2, _offset2) = parse_jsonl_stats(&path, offset1);
    assert_eq!(tokens2, 500);
    assert_eq!(messages2, 1);
}

#[test]
fn find_jsonl_path_locates_file() {
    let dir = tempfile::tempdir().unwrap();
    let project_hash = "-home-test-myproject";
    let session_id = "abc-123";
    let session_dir = dir.path().join("projects").join(project_hash).join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let jsonl = session_dir.join("agent-xyz.jsonl");
    std::fs::write(&jsonl, "{}\n").unwrap();
    let result = find_jsonl_path(dir.path(), "/home/test/myproject", session_id);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), jsonl);
}

#[test]
fn find_jsonl_path_returns_none_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    let result = find_jsonl_path(dir.path(), "/home/test/myproject", "no-such-id");
    assert!(result.is_none());
}

#[test]
fn tool_display_known_tools() {
    assert_eq!(tool_display("Edit"), "✏️ Editing");
    assert_eq!(tool_display("Write"), "📝 Writing");
    assert_eq!(tool_display("Read"), "📖 Reading");
    assert_eq!(tool_display("Bash"), "⚙️ Running");
    assert_eq!(tool_display("Glob"), "🔍 Searching");
    assert_eq!(tool_display("Grep"), "🔍 Searching");
    assert_eq!(tool_display("Agent"), "🤖 Delegating");
    assert_eq!(tool_display("WebSearch"), "🌐 Browsing");
    assert_eq!(tool_display("WebFetch"), "🌐 Fetching");
}

#[test]
fn tool_display_unknown_tool() {
    assert_eq!(tool_display("CustomTool"), "🔧 CustomTool");
}
