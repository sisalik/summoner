use std::collections::HashMap;
use std::process::Command;
use std::time::Instant;

pub fn parse_shortstat(output: &str) -> (u32, u32) {
    let mut adds: u32 = 0;
    let mut dels: u32 = 0;
    for part in output.split(',') {
        let part = part.trim();
        if part.contains("insertion") {
            if let Some(n) = part.split_whitespace().next().and_then(|s| s.parse().ok()) {
                adds = n;
            }
        } else if part.contains("deletion")
            && let Some(n) = part.split_whitespace().next().and_then(|s| s.parse().ok()) {
                dels = n;
            }
    }
    (adds, dels)
}

pub fn format_diff_compact(additions: u32, deletions: u32) -> String {
    if additions == 0 && deletions == 0 {
        return String::new();
    }
    let mut result = String::new();
    if additions > 0 {
        result.push('+');
        result.push_str(&format_compact_number(additions));
    }
    if deletions > 0 {
        result.push('-');
        result.push_str(&format_compact_number(deletions));
    }
    result
}

fn format_compact_number(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

pub struct GitDiffCache {
    cache: HashMap<String, CachedDiff>,
    poll_interval_secs: u64,
}

struct CachedDiff {
    pub additions: u32,
    pub deletions: u32,
    pub last_checked: Instant,
}

impl GitDiffCache {
    pub fn new(poll_interval_secs: u64) -> Self {
        Self { cache: HashMap::new(), poll_interval_secs }
    }

    pub fn get(&mut self, directory: &str) -> (u32, u32) {
        let now = Instant::now();
        if let Some(cached) = self.cache.get(directory)
            && now.duration_since(cached.last_checked).as_secs() < self.poll_interval_secs {
                return (cached.additions, cached.deletions);
            }
        let (adds, dels) = run_git_shortstat(directory);
        self.cache.insert(directory.to_string(), CachedDiff {
            additions: adds, deletions: dels, last_checked: now,
        });
        (adds, dels)
    }
}

fn run_git_shortstat(directory: &str) -> (u32, u32) {
    let output = Command::new("git")
        .args(["diff", "--shortstat"])
        .current_dir(directory)
        .output();
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_shortstat(&stdout)
        }
        Err(_) => (0, 0),
    }
}
