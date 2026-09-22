use summoner::statusline::StatusLineData;

#[test]
fn parse_statusline_json_full() {
    let json = r#"{
        "session_id": "abc-123",
        "context_window": {
            "used_percentage": 42,
            "remaining_percentage": 58,
            "context_window_size": 1000000
        },
        "rate_limits": {
            "five_hour": { "used_percentage": 35, "resets_at": 1774020000 },
            "seven_day": { "used_percentage": 67, "resets_at": 1774540000 }
        }
    }"#;
    let data = StatusLineData::from_json(json).unwrap();
    assert_eq!(data.session_id, "abc-123");
    assert_eq!(data.context_pct, Some(42));
    assert_eq!(data.five_hour_pct, Some(35));
    assert_eq!(data.five_hour_resets_at, Some(1774020000));
    assert_eq!(data.seven_day_pct, Some(67));
    assert_eq!(data.seven_day_resets_at, Some(1774540000));
}

#[test]
fn parse_statusline_json_missing_rate_limits() {
    let json = r#"{
        "session_id": "abc-123",
        "context_window": { "used_percentage": 10 }
    }"#;
    let data = StatusLineData::from_json(json).unwrap();
    assert_eq!(data.session_id, "abc-123");
    assert_eq!(data.context_pct, Some(10));
    assert_eq!(data.five_hour_pct, None);
    assert_eq!(data.seven_day_pct, None);
}

#[test]
fn parse_statusline_json_missing_session_id() {
    let json = r#"{"context_window": {"used_percentage": 5}}"#;
    let result = StatusLineData::from_json(json);
    assert!(result.is_none());
}

#[test]
fn wrapper_install_captures_an_existing_status_line_and_uninstall_restores_it() {
    let tmp = tempfile::TempDir::new().unwrap();
    let settings_path = tmp.path().join("settings.json");
    std::fs::write(
        &settings_path,
        r#"{"model": "opus", "statusLine": {"type": "command", "command": "my-prompt.sh"}}"#,
    )
    .unwrap();
    let summoner_dir = tmp.path().join("summoner");

    summoner::statusline::install_wrapper_at(&summoner_dir, &settings_path).unwrap();
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(settings["model"], "opus");
    assert_eq!(
        settings["statusLine"]["command"],
        "bash ~/.summoner/hooks/statusline-wrapper.sh"
    );

    let script_path = summoner_dir.join("hooks").join("statusline-wrapper.sh");
    assert!(std::fs::read_to_string(&script_path).unwrap().contains("my-prompt.sh"));

    summoner::statusline::uninstall_wrapper_at(&script_path, &settings_path).unwrap();
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(settings["statusLine"]["command"], "my-prompt.sh");
    assert_eq!(settings["model"], "opus");
}

#[test]
fn wrapper_install_reports_malformed_settings_and_leaves_them_alone() {
    let tmp = tempfile::TempDir::new().unwrap();
    let settings_path = tmp.path().join("settings.json");
    let broken = r#"{"statusLine": {"command": "my-prompt.sh",}}"#;
    std::fs::write(&settings_path, broken).unwrap();

    let err = summoner::statusline::install_wrapper_at(&tmp.path().join("summoner"), &settings_path)
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(std::fs::read_to_string(&settings_path).unwrap(), broken);
}
