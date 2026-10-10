//! The plan limits in the usage endpoint's answer.

use serde_json::Value;

use crate::limits::{FIVE_HOUR, LimitSnapshot, SEVEN_DAY, Source};

/// The plan limits in the endpoint's answer, as read at `observed_ms`; none when it holds nothing
/// this version can read.
///
/// The answer lists the limits in a `limits` array, each with its `kind`, the `percent` used and
/// when it `resets_at`. Earlier answers kept each window under a key of its own, with its
/// `utilization`; those are read too, and the array wins for a window both have. Limits for one
/// model alone are left out, because the status line reports only the five-hour and weekly
/// windows.
#[must_use]
pub fn plan_limits(json: &str, observed_ms: i64) -> Vec<LimitSnapshot> {
    let Ok(answer) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut snapshots: Vec<LimitSnapshot> = Vec::new();
    let mut add = |window: &str, entry: &Value, used: Option<&Value>| {
        if snapshots.iter().any(|snapshot| snapshot.window == window) {
            return;
        }
        if let Some(used) = used
            .and_then(Value::as_f64)
            .filter(|used| used.is_finite() && *used >= 0.0)
        {
            snapshots.push(LimitSnapshot {
                window: window.to_owned(),
                source: Source::UsageEndpoint,
                observed_ms,
                used_percentage: used,
                resets_at_ms: resets_at_ms(entry.get("resets_at")),
            });
        }
    };
    for limit in answer
        .get("limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let window = match limit.get("kind").and_then(Value::as_str) {
            Some("session") => FIVE_HOUR,
            Some("weekly_all") => SEVEN_DAY,
            _ => continue,
        };
        add(window, limit, limit.get("percent"));
    }
    for window in [FIVE_HOUR, SEVEN_DAY] {
        if let Some(entry) = answer.get(window).filter(|entry| entry.is_object()) {
            add(window, entry, entry.get("utilization"));
        }
    }
    snapshots
}

/// A reset time given as an RFC 3339 timestamp, or as seconds or milliseconds since the Unix
/// epoch.
fn resets_at_ms(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::String(text) => text
            .parse::<jiff::Timestamp>()
            .ok()
            .map(jiff::Timestamp::as_millisecond),
        Value::Number(number) => {
            let number = number.as_i64()?;
            // Seconds until the year 5138; milliseconds after that.
            Some(if number < 100_000_000_000 {
                number.saturating_mul(1000)
            } else {
                number
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: i64 = 1_760_090_400_000;

    fn windows(json: &str) -> Vec<(String, f64, Option<i64>)> {
        plan_limits(json, AT)
            .into_iter()
            .map(|snapshot| {
                assert_eq!(snapshot.source, Source::UsageEndpoint);
                assert_eq!(snapshot.observed_ms, AT);
                (
                    snapshot.window,
                    snapshot.used_percentage,
                    snapshot.resets_at_ms,
                )
            })
            .collect()
    }

    #[test]
    fn reads_the_limits_array() {
        let json = r#"{
            "five_hour": null, "seven_day": null, "seven_day_opus": null,
            "limits": [
                {"kind": "session", "percent": 42, "resets_at": "2026-10-10T15:00:00.000000+00:00"},
                {"kind": "weekly_all", "percent": 71.5, "resets_at": "2026-10-15T09:00:00Z"},
                {"kind": "weekly_scoped", "percent": 12, "resets_at": "2026-10-15T09:00:00Z",
                 "scope": {"model": {"display_name": "Opus"}}}
            ]
        }"#;
        assert_eq!(
            windows(json),
            [
                (FIVE_HOUR.to_owned(), 42.0, Some(1_791_644_400_000)),
                (SEVEN_DAY.to_owned(), 71.5, Some(1_792_054_800_000)),
            ]
        );
    }

    #[test]
    fn reads_a_window_under_a_key_of_its_own() {
        let json = r#"{
            "five_hour": {"utilization": 37.0, "resets_at": "2026-10-10T15:00:00+00:00"},
            "seven_day": {"utilization": 90, "resets_at": null},
            "seven_day_sonnet": {"utilization": 5.0, "resets_at": null}
        }"#;
        assert_eq!(
            windows(json),
            [
                (FIVE_HOUR.to_owned(), 37.0, Some(1_791_644_400_000)),
                (SEVEN_DAY.to_owned(), 90.0, None),
            ]
        );
    }

    #[test]
    fn the_array_wins_over_a_key_of_its_own() {
        let json = r#"{
            "five_hour": {"utilization": 10, "resets_at": null},
            "limits": [{"kind": "session", "percent": 42, "resets_at": 1791644400}]
        }"#;
        assert_eq!(
            windows(json),
            [(FIVE_HOUR.to_owned(), 42.0, Some(1_791_644_400_000))]
        );
    }

    #[test]
    fn what_cannot_be_read_is_left_out() {
        assert_eq!(windows("not json"), []);
        assert_eq!(windows("[]"), []);
        assert_eq!(windows(r#"{"limits": "soon"}"#), []);
        assert_eq!(
            windows(
                r#"{"limits": [{"kind": "session", "percent": "a lot"},
                               {"kind": "weekly_all", "percent": -1},
                               {"kind": "monthly", "percent": 3}]}"#
            ),
            []
        );
    }
}
