use summoner::git::{parse_shortstat, format_diff_compact};

#[test]
fn parse_shortstat_full_output() {
    let output = " 3 files changed, 47 insertions(+), 12 deletions(-)";
    let (adds, dels) = parse_shortstat(output);
    assert_eq!(adds, 47);
    assert_eq!(dels, 12);
}

#[test]
fn parse_shortstat_insertions_only() {
    let output = " 1 file changed, 5 insertions(+)";
    let (adds, dels) = parse_shortstat(output);
    assert_eq!(adds, 5);
    assert_eq!(dels, 0);
}

#[test]
fn parse_shortstat_deletions_only() {
    let output = " 2 files changed, 10 deletions(-)";
    let (adds, dels) = parse_shortstat(output);
    assert_eq!(adds, 0);
    assert_eq!(dels, 10);
}

#[test]
fn parse_shortstat_empty_output() {
    let (adds, dels) = parse_shortstat("");
    assert_eq!(adds, 0);
    assert_eq!(dels, 0);
}

#[test]
fn format_diff_compact_small_numbers() {
    assert_eq!(format_diff_compact(47, 12), "+47-12");
}

#[test]
fn format_diff_compact_large_numbers() {
    assert_eq!(format_diff_compact(1200, 340), "+1.2k-340");
}

#[test]
fn format_diff_compact_zero() {
    assert_eq!(format_diff_compact(0, 0), "");
}

#[test]
fn format_diff_compact_only_additions() {
    assert_eq!(format_diff_compact(5, 0), "+5");
}

#[test]
fn format_diff_compact_only_deletions() {
    assert_eq!(format_diff_compact(0, 8), "-8");
}
