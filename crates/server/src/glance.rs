//! The glance: plan limits and today's cost in a few words, for the desktop app's item in the
//! notification area or the menu bar. VS Code's status bar says the same; `design/glance.json`
//! holds the examples both are tested against.

use claude_dashboard_core::history::{History, HistoryError};
use claude_dashboard_core::report::{self, Filter, Range};
use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::app::App;
use crate::fmt::{ago, money, percent, when};
use crate::protocol::{Level, Provenance, WindowReading};
use crate::requests::window_reading;

/// What the app is called in the notification area, the menu bar and its menu.
pub const NAME: &str = "Dashboard for Claude Code";

/// What a window is called: `Five-hour window`, `Weekly, all models`, `Weekly, Fable`.
#[must_use]
pub fn window_name(reading: &WindowReading) -> String {
    match (reading.window.as_str(), &reading.model) {
        ("five_hour", _) => "Five-hour window".to_owned(),
        ("seven_day", _) => "Weekly, all models".to_owned(),
        (_, Some(model)) => format!("Weekly, {model}"),
        ("spend_limit", None) => "Spend limit".to_owned(),
        (window, None) => window.to_owned(),
    }
}

/// Five-hour first, then weekly, then each model's weekly window, then any other.
#[must_use]
pub fn in_order<T>(items: &[T], reading: impl Fn(&T) -> &WindowReading) -> Vec<&T> {
    let rank = |reading: &WindowReading| match reading.window.as_str() {
        "five_hour" => 0,
        "seven_day" => 1,
        _ if reading.model.is_some() => 2,
        _ => 3,
    };
    let key = |reading: &WindowReading| {
        reading
            .model
            .clone()
            .unwrap_or_else(|| reading.window.clone())
    };
    let mut ordered: Vec<&T> = items.iter().collect();
    ordered.sort_by(|a, b| {
        let (a, b) = (reading(a), reading(b));
        rank(a).cmp(&rank(b)).then_with(|| key(a).cmp(&key(b)))
    });
    ordered
}

/// A line of the glance: a plan window's use, or today's cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// `Five-hour window`, or `Today`.
    pub label: String,
    /// `42% · resets 14:20`, `71% ▲ · resets Thu 09:00`, or `$5.05`.
    pub value: String,
}

impl Line {
    /// The line as a menu writes it: `Five-hour window: 42% · resets 14:20`.
    #[must_use]
    pub fn text(&self) -> String {
        format!("{}: {}", self.label, self.value)
    }
}

/// The five-hour window's use, while a reading of it is current.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FiveHour {
    /// The share of the limit used, in percent.
    pub used: f64,
    pub level: Level,
}

/// Everything the glance shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Glance {
    /// The five-hour window, while its reading is current; without it, the icon shows the gauge
    /// and the title today's cost.
    pub five_hour: Option<FiveHour>,
    /// A line per plan window, five-hour first, then today's cost.
    pub lines: Vec<Line>,
    /// Where the newest reading comes from and how old it is: `Official, status line, 2 min ago`.
    pub source: Option<String>,
    /// The tooltip, in Microsoft's "name - status" form: `Dashboard for Claude Code - 5h 42% ·
    /// resets 14:20`.
    pub tooltip: String,
    /// The title beside the menu bar symbol on macOS: `42%`, `71% ▲`, or `$5.05`.
    pub title: String,
}

/// Today's API-equivalent cost, as the glance needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Today {
    pub usd: f64,
    pub unpriced_requests: u64,
}

/// The glance, from the history as it is at `app.now`.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn read(app: &App, history: &History) -> Result<Glance, HistoryError> {
    let readings: Vec<WindowReading> = history
        .latest_limits()?
        .iter()
        .map(|reading| window_reading(app, reading))
        .collect();
    let today = app.now_zoned().date();
    let totals = report::totals(
        history,
        &app.prices,
        Range::days(today, today, &app.zone),
        &Filter::default(),
    )?;
    Ok(compose(
        &readings,
        Today {
            usd: totals.cost.total(),
            unpriced_requests: totals.unpriced_requests,
        },
        &app.zone,
        app.now.as_millisecond(),
    ))
}

/// Whether a window has reset since its reading, so that the reading no longer tells its use.
fn expired(reading: &WindowReading, now_ms: i64) -> bool {
    reading.expired
        || reading
            .resets_at_ms
            .is_some_and(|resets_at_ms| resets_at_ms <= now_ms)
}

fn moment(ms: i64, zone: &TimeZone, now_ms: i64) -> String {
    let zoned = |ms: i64| {
        Timestamp::from_millisecond(ms)
            .unwrap_or_default()
            .to_zoned(zone.clone())
    };
    when(&zoned(ms), &zoned(now_ms))
}

/// How much of a window is used, marked from 70%, and when it resets: `71% ▲ · resets Thu 09:00`.
fn value(reading: &WindowReading, zone: &TimeZone, now_ms: i64) -> String {
    if expired(reading, now_ms) {
        return reading.resets_at_ms.map_or_else(
            || "reset".to_owned(),
            |resets_at_ms| format!("reset at {}", moment(resets_at_ms, zone, now_ms)),
        );
    }
    let mark = if reading.level == Level::Normal {
        ""
    } else {
        " ▲"
    };
    let used = format!("{}{mark}", percent(reading.used_percentage));
    match reading.resets_at_ms {
        Some(resets_at_ms) => format!("{used} · resets {}", moment(resets_at_ms, zone, now_ms)),
        None => used,
    }
}

/// A cost as the dashboard writes it, saying when some requests have no price.
fn cost(today: Today) -> String {
    if today.unpriced_requests > 0 {
        format!("{} + unpriced", money(today.usd))
    } else {
        money(today.usd)
    }
}

/// The glance for `readings` and today's cost, in `zone`, at `now_ms`.
#[must_use]
pub fn compose(readings: &[WindowReading], today: Today, zone: &TimeZone, now_ms: i64) -> Glance {
    let ordered = in_order(readings, |reading| reading);
    let mut lines: Vec<Line> = ordered
        .iter()
        .map(|reading| Line {
            label: window_name(reading),
            value: value(reading, zone, now_ms),
        })
        .collect();
    lines.push(Line {
        label: "Today".to_owned(),
        value: cost(today),
    });
    let source = ordered
        .iter()
        .max_by_key(|reading| reading.observed_ms)
        .and_then(|latest| {
            let observed = Timestamp::from_millisecond(latest.observed_ms).ok()?;
            let now = Timestamp::from_millisecond(now_ms).ok()?;
            let from = match latest.provenance {
                Provenance::Official => "Official, status line",
                Provenance::Experimental => "Experimental, usage endpoint",
            };
            Some(format!("{from}, {}", ago(observed, now)))
        });
    let current = ordered
        .iter()
        .find(|reading| reading.window == "five_hour" && !expired(reading, now_ms));
    let (five_hour, tooltip, title) = match current {
        Some(reading) => {
            let value = value(reading, zone, now_ms);
            let title = match reading.level {
                Level::Normal => percent(reading.used_percentage),
                Level::Warning | Level::Critical => {
                    format!("{} ▲", percent(reading.used_percentage))
                }
            };
            (
                Some(FiveHour {
                    used: reading.used_percentage,
                    level: reading.level,
                }),
                format!("{NAME} - 5h {value}"),
                title,
            )
        }
        None => (
            None,
            format!("{NAME} - {} today", cost(today)),
            money(today.usd),
        ),
    };
    Glance {
        five_hour,
        lines,
        source,
        tooltip,
        title,
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::Value;

    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Example {
        name: String,
        now_ms: i64,
        time_zone: String,
        limits: Vec<WindowReading>,
        today: TodayExample,
        expect: Expect,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TodayExample {
        usd: f64,
        unpriced_requests: u64,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Expect {
        lines: Vec<[String; 2]>,
        source: Option<String>,
        tooltip: String,
        title: String,
        icon: Option<f64>,
    }

    #[test]
    fn the_glance_says_what_the_shared_examples_say() {
        let examples: Value =
            serde_json::from_str(include_str!("../../../design/glance.json")).unwrap();
        let examples: Vec<Example> = serde_json::from_value(examples["examples"].clone()).unwrap();
        assert!(!examples.is_empty());
        for example in examples {
            let zone = TimeZone::get(&example.time_zone).unwrap();
            let glance = compose(
                &example.limits,
                Today {
                    usd: example.today.usd,
                    unpriced_requests: example.today.unpriced_requests,
                },
                &zone,
                example.now_ms,
            );
            let lines: Vec<[String; 2]> = glance
                .lines
                .iter()
                .map(|line| [line.label.clone(), line.value.clone()])
                .collect();
            assert_eq!(lines, example.expect.lines, "{}", example.name);
            assert_eq!(glance.source, example.expect.source, "{}", example.name);
            assert_eq!(glance.tooltip, example.expect.tooltip, "{}", example.name);
            assert_eq!(glance.title, example.expect.title, "{}", example.name);
            assert_eq!(
                glance.five_hour.map(|five_hour| five_hour.used),
                example.expect.icon,
                "{}",
                example.name
            );
        }
    }

    #[test]
    fn a_menu_line_joins_its_label_and_value() {
        let line = Line {
            label: "Today".to_owned(),
            value: "$5.05".to_owned(),
        };
        assert_eq!(line.text(), "Today: $5.05");
    }
}
