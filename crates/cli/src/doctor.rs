//! `claude-dashboard doctor`: what claude-dashboard can find, read and check.

use std::fmt::Write as _;
use std::process::ExitCode;

use claude_dashboard_core::history::{Access, History};
use claude_dashboard_core::import::{self, CLAUDE_USAGE};
use claude_dashboard_core::limits::Source;
use claude_dashboard_core::reconcile;
use claude_dashboard_core::report::{self, Range};
use claude_dashboard_core::roots::{self, CONFIG_DIR_ENV, RootSource, RootStatus};
use jiff::Timestamp;

use claude_dashboard_server::app::App;
use claude_dashboard_server::fmt::{ago, grouped, short_date};
use claude_dashboard_server::names::model_name;
use claude_dashboard_server::statusline::{self, Connection};

use crate::limits;
use crate::progress;
use crate::style::Paint;

/// How a check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    Good,
    Warning,
    Failed,
    Off,
}

struct Line {
    mark: Mark,
    label: &'static str,
    /// A path shown in its own column, for data folders.
    path: Option<String>,
    detail: String,
}

const LABEL_WIDTH: usize = 15;
/// A price table older than this may miss price changes.
const OLD_PRICE_TABLE_DAYS: i32 = 90;

pub fn run(app: &App) -> ExitCode {
    let mut lines = data_folders(app);
    let history = app.open_history();
    match history {
        Ok(mut history) => {
            if let Err(error) = progress::ingest(app, &mut history) {
                lines.push(failed(
                    "History",
                    format!("new transcripts cannot be read in: {error}"),
                ));
            }
            lines.push(history_line(app, &history));
            lines.push(prices_line(app, &history));
            lines.extend(limits_lines(app, &history));
            lines.extend(import_line(app, &history));
        }
        Err(error) => lines.push(failed("History", error)),
    }

    let paint = Paint::for_stdout();
    print!("{}", render(paint, &lines));
    let found = lines
        .iter()
        .any(|line| line.label == "Data folder" && line.mark == Mark::Good);
    if found && lines.iter().all(|line| line.mark != Mark::Failed) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn failed(label: &'static str, detail: String) -> Line {
    Line {
        mark: Mark::Failed,
        label,
        path: None,
        detail,
    }
}

fn data_folders(app: &App) -> Vec<Line> {
    if app.roots.is_empty() {
        return vec![failed(
            "Data folder",
            format!("none found: set {CONFIG_DIR_ENV}, because the home folder is unknown"),
        )];
    }
    app.roots
        .iter()
        .map(|root| {
            let (mark, detail) = match roots::inspect(&root.path) {
                RootStatus::Found { transcripts, bytes } => (
                    Mark::Good,
                    format!(
                        "{} {}, {}",
                        grouped(transcripts),
                        if transcripts == 1 {
                            "transcript"
                        } else {
                            "transcripts"
                        },
                        human_bytes(bytes)
                    ),
                ),
                RootStatus::Missing => (Mark::Failed, "not found".to_owned()),
                RootStatus::Unreachable { reason } => {
                    (Mark::Warning, format!("not reachable: {reason}"))
                }
                RootStatus::Unreadable { reason } => {
                    (Mark::Failed, format!("cannot be read: {reason}"))
                }
            };
            let detail = match root.source {
                RootSource::Environment => format!("{detail} ({CONFIG_DIR_ENV})"),
                RootSource::Default | RootSource::Added => detail,
            };
            Line {
                mark,
                label: "Data folder",
                path: Some(root.path.display().to_string()),
                detail,
            }
        })
        .collect()
}

fn history_line(app: &App, history: &History) -> Line {
    let size = app
        .history_path
        .as_ref()
        .and_then(|path| std::fs::metadata(path).ok())
        .map_or_else(String::new, |metadata| {
            format!("{}, ", human_bytes(metadata.len()))
        });
    let schema = history.schema_version().unwrap_or_default();
    match history.integrity_check() {
        Ok(problems) if problems.is_empty() => match history.access() {
            Access::ReadWrite => Line {
                mark: Mark::Good,
                label: "History",
                path: None,
                detail: format!("{size}schema {schema}, integrity check passed"),
            },
            Access::ReadOnly { schema } => Line {
                mark: Mark::Warning,
                label: "History",
                path: None,
                detail: format!(
                    "{size}upgraded by a newer claude-dashboard (schema {schema}): update claude-dashboard to keep recording"
                ),
            },
        },
        Ok(problems) => failed(
            "History",
            format!(
                "integrity check found {} problems, first: {}",
                problems.len(),
                problems[0]
            ),
        ),
        Err(error) => failed("History", format!("cannot be checked: {error}")),
    }
}

fn prices_line(app: &App, history: &History) -> Line {
    let table = format!("table of {}", short_date(app.prices.checked()));
    let all_time = Range {
        from_ms: i64::MIN,
        to_ms: i64::MAX,
    };
    let models = match report::by_model(history, app.prices, all_time) {
        Ok(models) => models,
        Err(error) => return failed("Prices", format!("cannot be checked: {error}")),
    };
    let unpriced: Vec<&str> = models
        .iter()
        .filter(|model| model.totals.unpriced_requests > 0)
        .map(|model| model.model.as_str())
        .collect();
    let coverage = if unpriced.is_empty() {
        format!(
            "all {} {} priced",
            models.len(),
            if models.len() == 1 { "model" } else { "models" }
        )
    } else {
        format!(
            "{} of {} models unpriced: {}",
            unpriced.len(),
            models.len(),
            unpriced.join(", ")
        )
    };
    let reconciliation = reconcile::reconcile(history, app.prices, app.now.as_millisecond());
    let (mark, comparison) = match &reconciliation {
        Ok(result) => match (result.out_of_line().next(), result.drift()) {
            (Some(model), _) => (
                Mark::Warning,
                format!(
                    "prices may be out of date: {} is {:+.1}% from Claude Code",
                    model_name(app, &model.model),
                    model.drift().unwrap_or_default() * 100.0
                ),
            ),
            (None, Some(drift)) => (
                Mark::Good,
                format!(
                    "within {:.1}% of Claude Code",
                    (drift.abs() * 100.0).max(0.1)
                ),
            ),
            (None, None) => (
                Mark::Good,
                "no finished sessions to compare with Claude Code yet".to_owned(),
            ),
        },
        Err(error) => (
            Mark::Warning,
            format!("cannot be compared with Claude Code: {error}"),
        ),
    };
    let age_days = (app.now_zoned().date() - app.prices.checked()).get_days();
    let mut mark = if unpriced.is_empty() {
        mark
    } else {
        Mark::Warning
    };
    let mut detail = format!("{table}, {coverage}, {comparison}");
    if age_days > OLD_PRICE_TABLE_DAYS {
        mark = Mark::Warning;
        let _ = write!(
            detail,
            "; the table is {age_days} days old, so a newer claude-dashboard may have newer prices"
        );
    }
    Line {
        mark,
        label: "Prices",
        path: None,
        detail,
    }
}

fn limits_lines(app: &App, history: &History) -> Vec<Line> {
    let readings = history.latest_limits().unwrap_or_default();
    let last_reading = |source: Source| {
        readings
            .iter()
            .filter(|reading| reading.source == source)
            .map(|reading| reading.observed_ms)
            .max()
            .and_then(|ms| Timestamp::from_millisecond(ms).ok())
    };
    let status_line = match statusline::connection(app) {
        Connection::Connected { .. } => match last_reading(Source::StatusLine) {
            Some(at) => Line {
                mark: Mark::Good,
                label: "Limits",
                path: None,
                detail: format!("status line connected, last reading {}", ago(at, app.now)),
            },
            None => Line {
                mark: Mark::Warning,
                label: "Limits",
                path: None,
                detail: "status line connected, no reading yet: Claude Code passes plan limits after its first response on a Pro or Max plan".to_owned(),
            },
        },
        Connection::NotConnected { .. } => Line {
            mark: Mark::Off,
            label: "Limits",
            path: None,
            detail: "status line not connected; to record plan limits, run claude-dashboard statusline connect".to_owned(),
        },
        Connection::Unknown(reason) => Line {
            mark: Mark::Warning,
            label: "Limits",
            path: None,
            detail: format!("cannot tell whether the status line is connected: {reason}"),
        },
    };
    let state = history.usage_endpoint().unwrap_or_default();
    let (mark, detail) = if state.enabled {
        match limits::doctor_detail(app, &state) {
            (true, detail) => (Mark::Good, detail),
            (false, detail) => (Mark::Warning, detail),
        }
    } else {
        (
            Mark::Off,
            "usage endpoint off (experimental, opt-in)".to_owned(),
        )
    };
    let endpoint = Line {
        mark,
        label: "Limits",
        path: None,
        detail,
    };
    vec![status_line, endpoint]
}

fn import_line(app: &App, history: &History) -> Option<Line> {
    if let Ok(Some(done)) = history.import(CLAUDE_USAGE) {
        let day = Timestamp::from_millisecond(done.imported_ms)
            .map(|at| short_date(at.to_zoned(app.zone.clone()).date()))
            .unwrap_or_default();
        return Some(Line {
            mark: Mark::Good,
            label: "Old history",
            path: None,
            detail: format!(
                "{} requests imported from claude-usage on {day}",
                grouped(done.requests)
            ),
        });
    }
    let database = app
        .roots
        .iter()
        .find_map(|root| import::claude_usage_database(&root.path))?;
    Some(Line {
        mark: Mark::Warning,
        label: "Old history",
        path: None,
        detail: format!(
            "claude-usage's database at {} can be imported: run claude-dashboard import claude-usage",
            database.display()
        ),
    })
}

fn render(paint: Paint, lines: &[Line]) -> String {
    let path_width = lines
        .iter()
        .filter_map(|line| line.path.as_ref().map(|path| path.chars().count()))
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for line in lines {
        let mark = match line.mark {
            Mark::Good => paint.paint("✓", paint.palette().good, false),
            Mark::Warning => paint.paint("!", paint.palette().warning, false),
            Mark::Failed => paint.paint("✗", paint.palette().critical, false),
            Mark::Off => paint.muted("-"),
        };
        let mut text = format!("{mark} {:<LABEL_WIDTH$}", line.label);
        if let Some(path) = &line.path {
            let _ = write!(text, "{path:<path_width$}   ");
        }
        text.push_str(&line.detail);
        let _ = writeln!(out, "{}", text.trim_end());
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
    use super::*;

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(12_400), "12 KB");
        assert_eq!(human_bytes(310_000_000), "310 MB");
        assert_eq!(human_bytes(2_900_000_000), "2.9 GB");
    }

    #[test]
    fn lines_line_up_as_in_the_design() {
        let lines = [
            Line {
                mark: Mark::Good,
                label: "Data folder",
                path: Some(r"C:\Users\you\.claude".to_owned()),
                detail: "254 transcripts, 310 MB".to_owned(),
            },
            Line {
                mark: Mark::Warning,
                label: "Data folder",
                path: Some(r"\\wsl.localhost\Ubuntu\home\you\.claude".to_owned()),
                detail: "not reachable: WSL is not running".to_owned(),
            },
            Line {
                mark: Mark::Good,
                label: "History",
                path: None,
                detail: "9 MB, schema 3, integrity check passed".to_owned(),
            },
            Line {
                mark: Mark::Off,
                label: "Limits",
                path: None,
                detail: "usage endpoint off (experimental, opt-in)".to_owned(),
            },
        ];
        assert_eq!(
            render(Paint::PLAIN, &lines),
            "✓ Data folder    C:\\Users\\you\\.claude                      254 transcripts, 310 MB\n\
             ! Data folder    \\\\wsl.localhost\\Ubuntu\\home\\you\\.claude   not reachable: WSL is not running\n\
             ✓ History        9 MB, schema 3, integrity check passed\n\
             - Limits         usage endpoint off (experimental, opt-in)\n"
        );
    }
}
