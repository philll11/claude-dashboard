//! Plan limits: how much of each plan window is used, as Claude Code reports it (ADR 0005).

use serde::Deserialize;
use serde_json::Value;

use crate::history::{History, HistoryError};
use crate::pricing::PriceTable;
use crate::report::{self, Filter, Range, Totals};

/// The rolling five-hour window.
pub const FIVE_HOUR: &str = "five_hour";
/// The weekly window.
pub const SEVEN_DAY: &str = "seven_day";

/// The weekly window of one model, such as Fable's, which only the usage endpoint reports.
#[must_use]
pub fn weekly_model_window(model: &str) -> Option<String> {
    let model = model.trim();
    (!model.is_empty()).then(|| format!("{SEVEN_DAY}:{model}"))
}

/// The model whose weekly window `window` is, by the name the usage endpoint gives it.
#[must_use]
pub fn weekly_model(window: &str) -> Option<&str> {
    window.strip_prefix(SEVEN_DAY)?.strip_prefix(':')
}

/// How old a reading may be before it is shown as stale: Claude Code reports limits only while
/// it runs in a terminal, and other Claude apps use the same windows meanwhile.
pub const STALE_AFTER_MS: i64 = 60 * 60 * 1000;

/// Where a limit snapshot comes from, and so how far to trust it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// Claude Code's status line, which documents the data: official.
    StatusLine,
    /// Anthropic's undocumented usage endpoint, used only when the person turns it on:
    /// experimental.
    UsageEndpoint,
}

impl Source {
    /// The name the history stores the source under.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StatusLine => "status-line",
            Self::UsageEndpoint => "usage-endpoint",
        }
    }

    /// The source the history stores under `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "status-line" => Some(Self::StatusLine),
            "usage-endpoint" => Some(Self::UsageEndpoint),
            _ => None,
        }
    }
}

/// How close a window is to its limit, with the thresholds the design sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Normal,
    /// From 70%.
    Warning,
    /// From 90%, until the window resets.
    Critical,
}

/// How much of one plan window was used, as reported at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct LimitSnapshot {
    /// The window, such as [`FIVE_HOUR`] or [`SEVEN_DAY`].
    pub window: String,
    pub source: Source,
    /// When the reading was taken, in milliseconds since the Unix epoch.
    pub observed_ms: i64,
    /// From 0 to 100, or above 100 past the limit.
    pub used_percentage: f64,
    /// When the window resets, in milliseconds since the Unix epoch.
    pub resets_at_ms: Option<i64>,
}

impl Level {
    /// The level of a window with `used_percentage` used.
    #[must_use]
    pub fn of(used_percentage: f64) -> Self {
        if used_percentage >= 90.0 {
            Self::Critical
        } else if used_percentage >= 70.0 {
            Self::Warning
        } else {
            Self::Normal
        }
    }
}

impl LimitSnapshot {
    #[must_use]
    pub fn level(&self) -> Level {
        Level::of(self.used_percentage)
    }

    /// Whether the window has reset since the reading, which no longer tells how much of the
    /// new window is used.
    #[must_use]
    pub fn expired(&self, now_ms: i64) -> bool {
        self.resets_at_ms.is_some_and(|resets| resets <= now_ms)
    }

    /// Whether the reading is old enough to show as stale.
    #[must_use]
    pub fn stale(&self, now_ms: i64) -> bool {
        now_ms.saturating_sub(self.observed_ms) > STALE_AFTER_MS
    }
}

/// What the history takes from the JSON that Claude Code passes to a status line command.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusLineInput {
    pub session_id: Option<String>,
    /// The session's transcript.
    pub transcript_path: Option<String>,
    pub model_id: Option<String>,
    /// Claude Code's short name for the model, such as `Opus`.
    pub model_name: Option<String>,
    /// Each plan window's use and reset time, in seconds since the Unix epoch, in the order
    /// Claude Code lists them.
    pub windows: Vec<(String, f64, Option<i64>)>,
}

#[derive(Deserialize)]
struct RawInput {
    #[serde(default)]
    session_id: Value,
    #[serde(default)]
    transcript_path: Value,
    #[serde(default)]
    model: Value,
    #[serde(default)]
    rate_limits: Value,
}

impl StatusLineInput {
    /// Reads Claude Code's status line JSON; fields that are missing or of another type are left
    /// out, so that the status line keeps working when the format changes.
    #[must_use]
    pub fn parse(json: &str) -> Option<Self> {
        let raw: RawInput = serde_json::from_str(json).ok()?;
        let text = |value: &Value| value.as_str().map(str::to_owned);
        let windows = raw
            .rate_limits
            .as_object()
            .map(|windows| {
                windows
                    .iter()
                    .filter_map(|(name, window)| {
                        let used = window.get("used_percentage")?.as_f64()?;
                        let resets = window.get("resets_at").and_then(Value::as_i64);
                        Some((name.clone(), used, resets))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            session_id: text(&raw.session_id),
            transcript_path: text(&raw.transcript_path),
            model_id: raw.model.get("id").and_then(text),
            model_name: raw.model.get("display_name").and_then(text),
            windows,
        })
    }

    /// The readings this input holds, as taken at `now_ms`.
    #[must_use]
    pub fn snapshots(&self, now_ms: i64) -> Vec<LimitSnapshot> {
        self.windows
            .iter()
            .map(|(window, used, resets_at)| LimitSnapshot {
                window: window.clone(),
                source: Source::StatusLine,
                observed_ms: now_ms,
                used_percentage: *used,
                resets_at_ms: resets_at.map(|seconds| seconds.saturating_mul(1000)),
            })
            .collect()
    }
}

/// How long the five-hour window lasts.
pub const FIVE_HOURS_MS: i64 = 5 * 60 * 60 * 1000;

/// How long a pace must have run before it says where a window is heading.
const SHORTEST_PACE_MS: i64 = 15 * 60 * 1000;

/// Where a window is heading at its current pace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    /// The share of the limit used by the time the window resets.
    pub at_reset: f64,
    /// When the limit is reached, if that comes before the window resets.
    pub limit_reached_ms: Option<i64>,
}

/// The pace from `since` to `now`, each a time and the share of the limit used then, and where
/// it leads by `resets_at_ms`. `None` while too little time has passed to tell, or once the
/// window has reset.
#[must_use]
pub fn pace(since: (i64, f64), now: (i64, f64), resets_at_ms: i64) -> Option<Pace> {
    let (since_ms, since_used) = since;
    let (now_ms, now_used) = now;
    let elapsed = now_ms - since_ms;
    if elapsed < SHORTEST_PACE_MS || now_ms >= resets_at_ms {
        return None;
    }
    let per_ms = (now_used - since_used) / milliseconds(elapsed);
    if per_ms <= 0.0 {
        return Some(Pace {
            at_reset: now_used,
            limit_reached_ms: None,
        });
    }
    let limit_reached_ms = (now_used < 100.0)
        .then(|| now_ms.saturating_add(whole_ms((100.0 - now_used) / per_ms)))
        .filter(|at| *at < resets_at_ms);
    Some(Pace {
        at_reset: now_used + per_ms * milliseconds(resets_at_ms - now_ms),
        limit_reached_ms,
    })
}

#[expect(
    clippy::cast_precision_loss,
    reason = "times in milliseconds stay far below 2^53, where f64 starts to skip whole numbers"
)]
fn milliseconds(ms: i64) -> f64 {
    ms as f64
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the duration is clamped to fit, and a pace never looks past a week"
)]
fn whole_ms(ms: f64) -> i64 {
    ms.clamp(0.0, 1e15) as i64
}

/// One five-hour window, as its readings saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowSpan {
    pub start_ms: i64,
    pub resets_at_ms: i64,
    /// The largest share of the limit any reading in it saw.
    pub highest: f64,
}

/// The five-hour windows that `readings` saw, newest first. Readings belong to one window when
/// they say it resets in the same minute, because the status line gives seconds and the usage
/// endpoint fractions of them.
#[must_use]
pub fn five_hour_windows(readings: &[LimitSnapshot]) -> Vec<WindowSpan> {
    const MINUTE_MS: i64 = 60 * 1000;
    let mut windows: Vec<WindowSpan> = Vec::new();
    for reading in readings {
        let Some(resets_at_ms) = reading.resets_at_ms else {
            continue;
        };
        let minute = resets_at_ms.div_euclid(MINUTE_MS);
        match windows
            .iter_mut()
            .find(|window| window.resets_at_ms.div_euclid(MINUTE_MS) == minute)
        {
            Some(window) => window.highest = window.highest.max(reading.used_percentage),
            None => windows.push(WindowSpan {
                start_ms: resets_at_ms - FIVE_HOURS_MS,
                resets_at_ms,
                highest: reading.used_percentage,
            }),
        }
    }
    windows.sort_by_key(|window| std::cmp::Reverse(window.resets_at_ms));
    windows
}

/// The five-hour windows seen from `since_ms` to `now_ms`, newest first, each with the totals of
/// the requests made in it.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn recent_five_hour_windows(
    history: &History,
    table: &PriceTable,
    since_ms: i64,
    now_ms: i64,
) -> Result<Vec<(WindowSpan, Totals)>, HistoryError> {
    let readings = history.limit_snapshots(FIVE_HOUR, since_ms, now_ms.saturating_add(1))?;
    five_hour_windows(&readings)
        .into_iter()
        .map(|window| {
            let range = Range {
                from_ms: window.start_ms,
                to_ms: window.resets_at_ms.min(now_ms.saturating_add(1)),
            };
            let totals = report::totals(history, table, range, &Filter::default())?;
            Ok((window, totals))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: i64 = 60 * 1000;

    #[test]
    fn a_steady_pace_says_where_the_window_ends_and_when_it_hits_the_limit() {
        let pace = pace((0, 0.0), (60 * MINUTE, 30.0), 300 * MINUTE).unwrap();
        assert!((pace.at_reset - 150.0).abs() < 1e-9);
        assert_eq!(pace.limit_reached_ms, Some(200 * MINUTE));
    }

    #[test]
    fn a_pace_that_stays_under_the_limit_never_reaches_it() {
        let pace = pace((0, 0.0), (60 * MINUTE, 10.0), 300 * MINUTE).unwrap();
        assert!((pace.at_reset - 50.0).abs() < 1e-9);
        assert_eq!(pace.limit_reached_ms, None);
    }

    #[test]
    fn too_short_a_pace_or_a_reset_window_says_nothing() {
        assert_eq!(pace((0, 0.0), (10 * MINUTE, 30.0), 300 * MINUTE), None);
        assert_eq!(pace((0, 0.0), (300 * MINUTE, 30.0), 300 * MINUTE), None);
        let flat = pace((0, 40.0), (60 * MINUTE, 40.0), 300 * MINUTE).unwrap();
        assert_eq!((flat.at_reset, flat.limit_reached_ms), (40.0, None));
    }

    #[test]
    fn readings_that_reset_in_the_same_minute_are_one_window() {
        let reading = |observed_ms: i64, used: f64, resets_at_ms: i64| LimitSnapshot {
            window: FIVE_HOUR.into(),
            source: Source::StatusLine,
            observed_ms,
            used_percentage: used,
            resets_at_ms: Some(resets_at_ms),
        };
        let first = 1_000 * MINUTE;
        let second = first + FIVE_HOURS_MS;
        let windows = five_hour_windows(&[
            reading(first - 60 * MINUTE, 20.0, first),
            reading(first - 30 * MINUTE, 35.0, first + 1_756),
            reading(second - 200 * MINUTE, 5.0, second),
        ]);
        let spans: Vec<_> = windows
            .iter()
            .map(|w| (w.start_ms, w.resets_at_ms, w.highest))
            .collect();
        assert_eq!(
            spans,
            [
                (second - FIVE_HOURS_MS, second, 5.0),
                (first - FIVE_HOURS_MS, first, 35.0),
            ]
        );
    }

    #[test]
    fn reads_the_plan_windows_claude_code_passes_to_a_status_line() {
        let input = StatusLineInput::parse(
            r#"{"session_id": "s1", "transcript_path": "/t/s1.jsonl",
                "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
                "cost": {"total_cost_usd": 0.01},
                "rate_limits": {
                    "five_hour": {"used_percentage": 23.5, "resets_at": 1738425600},
                    "seven_day": {"used_percentage": 41.2, "resets_at": 1738857600},
                    "spend_limit": {"used_percentage": 62.8, "resets_at": 1740787200, "used_usd": 314.12},
                    "a_window_without_a_percentage": {"resets_at": 1}
                }}"#,
        )
        .unwrap();

        assert_eq!(input.session_id.as_deref(), Some("s1"));
        assert_eq!(input.model_id.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(input.model_name.as_deref(), Some("Opus"));
        let snapshots = input.snapshots(1_738_420_000_000);
        let windows: Vec<_> = snapshots
            .iter()
            .map(|s| (s.window.as_str(), s.used_percentage, s.resets_at_ms))
            .collect();
        assert_eq!(
            windows,
            [
                ("five_hour", 23.5, Some(1_738_425_600_000)),
                ("seven_day", 41.2, Some(1_738_857_600_000)),
                ("spend_limit", 62.8, Some(1_740_787_200_000)),
            ]
        );
    }

    #[test]
    fn input_without_limits_has_no_windows() {
        let input =
            StatusLineInput::parse(r#"{"model": {"id": 5}, "rate_limits": "none"}"#).unwrap();
        assert_eq!(input, StatusLineInput::default());
        assert_eq!(StatusLineInput::parse("not json"), None);
    }

    #[test]
    fn levels_follow_the_design_thresholds() {
        let at = |used| LimitSnapshot {
            window: FIVE_HOUR.into(),
            source: Source::StatusLine,
            observed_ms: 0,
            used_percentage: used,
            resets_at_ms: Some(10),
        };
        assert_eq!(at(69.9).level(), Level::Normal);
        assert_eq!(at(70.0).level(), Level::Warning);
        assert_eq!(at(90.0).level(), Level::Critical);
        assert!(at(5.0).expired(10));
        assert!(!at(5.0).expired(9));
        assert!(at(5.0).stale(STALE_AFTER_MS + 1));
        assert!(!at(5.0).stale(STALE_AFTER_MS));
    }
}
