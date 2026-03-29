use summoner::stats::{format_xp, level_from_tokens};

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
