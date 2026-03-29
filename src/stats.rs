pub fn format_xp(tokens: u64) -> String {
    if tokens < 1_000 {
        format!("✦{}", tokens)
    } else if tokens < 1_000_000 {
        format!("✦{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("✦{:.1}M", tokens as f64 / 1_000_000.0)
    }
}

const LEVEL_THRESHOLDS: &[(u64, u8)] = &[
    (25_000_000, 10),
    (10_000_000, 9),
    (5_000_000, 8),
    (2_500_000, 7),
    (1_000_000, 6),
    (500_000, 5),
    (150_000, 4),
    (50_000, 3),
    (10_000, 2),
];

pub fn level_from_tokens(tokens: u64) -> u8 {
    for &(threshold, level) in LEVEL_THRESHOLDS {
        if tokens >= threshold {
            return level;
        }
    }
    1
}
