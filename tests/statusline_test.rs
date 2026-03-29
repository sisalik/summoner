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
