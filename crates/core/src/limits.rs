//! Plan limits: how much of each plan window is used, as Claude Code reports it (ADR 0005).

use serde::Deserialize;
use serde_json::Value;

/// The rolling five-hour window.
pub const FIVE_HOUR: &str = "five_hour";
/// The weekly window.
pub const SEVEN_DAY: &str = "seven_day";

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

#[cfg(test)]
mod tests {
    use super::*;

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
