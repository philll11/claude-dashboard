//! `claude-dashboard doctor`: what claude-dashboard can find and read.

use std::process::ExitCode;

use claude_dashboard_core::roots::{self, CONFIG_DIR_ENV, RootSource, RootStatus};

pub fn run() -> ExitCode {
    let config_dir = std::env::var_os(CONFIG_DIR_ENV);
    let home = std::env::home_dir();
    let data_roots = roots::data_roots(config_dir.as_deref(), home.as_deref(), &[]);

    println!("Data folders");
    if data_roots.is_empty() {
        println!("  ✗ No data folder: set {CONFIG_DIR_ENV}, because the home folder is unknown");
        return ExitCode::FAILURE;
    }

    let mut found_any = false;
    for root in &data_roots {
        let source = match root.source {
            RootSource::Environment => CONFIG_DIR_ENV,
            RootSource::Default => "default",
            RootSource::Added => "added",
        };
        let (mark, detail) = match roots::inspect(&root.path) {
            RootStatus::Found { transcripts, bytes } => {
                found_any = true;
                (
                    "✓",
                    format!(
                        "{}, {}",
                        plural(transcripts, "transcript"),
                        human_bytes(bytes)
                    ),
                )
            }
            RootStatus::Missing => ("✗", "not found".to_owned()),
            RootStatus::Unreachable { reason } => ("!", format!("not reachable: {reason}")),
            RootStatus::Unreadable { reason } => ("✗", format!("cannot be read: {reason}")),
        };
        println!("  {mark} {}  ({source})  {detail}", root.path.display());
    }

    if found_any {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn plural(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{} {noun}s", grouped(count))
    }
}

fn grouped(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[allow(clippy::cast_precision_loss)]
fn human_bytes(bytes: u64) -> String {
    let value = bytes as f64;
    if bytes < 1_000 {
        format!("{bytes} B")
    } else if bytes < 1_000_000 {
        format!("{:.0} KB", value / 1e3)
    } else if bytes < 1_000_000_000 {
        format!("{:.0} MB", value / 1e6)
    } else {
        format!("{:.1} GB", value / 1e9)
    }
}

#[cfg(test)]
mod tests {
    use super::{human_bytes, plural};

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(12_400), "12 KB");
        assert_eq!(human_bytes(310_000_000), "310 MB");
        assert_eq!(human_bytes(2_900_000_000), "2.9 GB");
    }

    #[test]
    fn counts_read_naturally() {
        assert_eq!(plural(0, "transcript"), "0 transcripts");
        assert_eq!(plural(1, "transcript"), "1 transcript");
        assert_eq!(plural(254, "transcript"), "254 transcripts");
        assert_eq!(plural(2_775, "transcript"), "2,775 transcripts");
        assert_eq!(plural(1_234_567, "request"), "1,234,567 requests");
    }
}
