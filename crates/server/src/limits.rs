//! What the dashboard and the reports say about plan limits: each window with its use and pace,
//! the recent five-hour windows, where the readings come from, the consent turning Anthropic's
//! usage endpoint on asks for, and what went wrong the last time it was asked.

use claude_dashboard_core::history::{History, HistoryError};
use claude_dashboard_core::limits::{self, FIVE_HOUR, FIVE_HOURS_MS};
use claude_dashboard_core::usage_endpoint::{Outcome, State};
use jiff::Timestamp;

use crate::app::App;
use crate::fmt::when;
use crate::protocol::{Endpoint, FiveHourWindow, Limits, Pace, StatusLine, UsePoint, WindowView};
use crate::requests::{totals, window_reading};
use crate::statusline::{self, Connection};

/// How long a weekly window lasts.
const WEEK_MS: i64 = 7 * DAY_MS;
/// How far back a weekly window's pace is measured.
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// How far back the recent five-hour windows go.
pub(crate) const RECENT_WINDOWS_MS: i64 = 7 * DAY_MS;

/// Every plan window with its readings since it started and its pace, the recent five-hour
/// windows, and where the readings come from.
///
/// # Errors
///
/// When the history cannot be read.
pub fn view(app: &App, history: &History) -> Result<Limits, HistoryError> {
    let now_ms = app.now.as_millisecond();
    let mut windows = Vec::new();
    for reading in history.latest_limits()? {
        let length = if reading.window == FIVE_HOUR {
            FIVE_HOURS_MS
        } else {
            WEEK_MS
        };
        let start_ms = reading.resets_at_ms.map(|resets| resets - length);
        let series: Vec<UsePoint> = match (start_ms, reading.resets_at_ms) {
            (Some(start_ms), Some(resets_at_ms)) => history
                .limit_snapshots(&reading.window, start_ms, now_ms.saturating_add(1))?
                .into_iter()
                .filter(|point| {
                    point.resets_at_ms.is_some_and(|resets| {
                        resets.div_euclid(60_000) == resets_at_ms.div_euclid(60_000)
                    })
                })
                .map(|point| UsePoint {
                    ms: point.observed_ms,
                    used: point.used_percentage,
                })
                .collect(),
            _ => Vec::new(),
        };
        let pace = match (start_ms, reading.resets_at_ms) {
            (Some(start_ms), Some(resets_at_ms)) if !reading.expired(now_ms) => {
                let since = if reading.window == FIVE_HOUR {
                    (start_ms, 0.0)
                } else {
                    // The reading nearest to a day before, or the window's start.
                    let target = reading.observed_ms - DAY_MS;
                    series
                        .iter()
                        .rev()
                        .find(|point| point.ms <= target)
                        .map_or((start_ms.max(target), 0.0), |point| (point.ms, point.used))
                };
                limits::pace(
                    since,
                    (reading.observed_ms, reading.used_percentage),
                    resets_at_ms,
                )
                .map(|pace| Pace {
                    at_reset: pace.at_reset,
                    limit_reached_ms: pace.limit_reached_ms,
                    since_ms: since.0,
                })
            }
            _ => None,
        };
        windows.push(WindowView {
            reading: window_reading(app, &reading),
            start_ms,
            series,
            pace,
        });
    }
    let recent =
        limits::recent_five_hour_windows(history, &app.prices, now_ms - RECENT_WINDOWS_MS, now_ms)?
            .into_iter()
            .map(|(window, window_totals)| FiveHourWindow {
                start_ms: window.start_ms,
                resets_at_ms: window.resets_at_ms,
                highest: window.highest,
                totals: totals(&window_totals),
            })
            .collect();
    let state = history.usage_endpoint()?;
    Ok(Limits {
        windows,
        recent,
        status_line: match statusline::connection(app) {
            Connection::Connected { wrapping } => StatusLine::Connected { wrapping },
            Connection::NotConnected { .. } => StatusLine::NotConnected,
            Connection::Unknown(reason) => StatusLine::Unknown { reason },
        },
        endpoint: Endpoint {
            enabled: state.enabled,
            asked_ms: state.asked_ms,
            outcome: state.outcome.map(|outcome| outcome.as_str().to_owned()),
            problem: problem(app, &state),
            detail: state.detail,
            next_ask_ms: state.next_ask_ms,
            consent: CONSENT.map(str::to_owned).to_vec(),
        },
    })
}

/// What turning the endpoint on means, in the design's words, and what Anthropic's terms say
/// about it.
pub const CONSENT: [&str; 5] = [
    "It reads the login Claude Code keeps on this computer and sends it only to api.anthropic.com.",
    "It never changes or refreshes that login.",
    "It asks at most once every 5 minutes, shared by every claude-dashboard window.",
    "The endpoint is undocumented, so it can change or stop working at any time.",
    "Anthropic's terms say Claude Code's login is for Claude Code and Anthropic's own apps, and Anthropic may act on other uses without notice.",
];

const LOGIN_EXPIRED: &str = "Claude Code's login on this computer has expired, so the usage endpoint can't be asked. Open Claude Code once to sign in again; claude-dashboard never refreshes the login itself.";

const LOGIN_REFUSED: &str = "The usage endpoint did not accept Claude Code's login on this computer. Open Claude Code once to sign in again; claude-dashboard never refreshes the login itself.";

/// What a report says under the plan limits while the endpoint needs Claude Code's login
/// renewed, which only the person can do.
#[must_use]
pub fn login_note(state: &State) -> Option<&'static str> {
    if !state.enabled {
        return None;
    }
    match state.outcome? {
        Outcome::LoginExpired => Some(LOGIN_EXPIRED),
        Outcome::LoginRefused => Some(LOGIN_REFUSED),
        _ => None,
    }
}

/// What went wrong the last time the endpoint was asked, as a sentence; `None` when nothing did.
pub fn problem(app: &App, state: &State) -> Option<String> {
    if !state.enabled {
        return None;
    }
    login_note(state)
        .map(str::to_owned)
        .or_else(|| trouble(app, state).map(|clause| sentence(&clause)))
}

/// What went wrong the last time the endpoint was asked, as a clause; `None` when nothing did.
#[must_use]
pub fn trouble(app: &App, state: &State) -> Option<String> {
    Some(match state.outcome? {
        Outcome::Answered => return None,
        Outcome::LoginExpired => {
            "Claude Code's login on this computer has expired; open Claude Code once to sign in again"
                .to_owned()
        }
        Outcome::LoginRefused => {
            "it did not accept Claude Code's login; open Claude Code once to sign in again"
                .to_owned()
        }
        Outcome::NoLogin => state
            .detail
            .clone()
            .unwrap_or_else(|| "Claude Code has no claude.ai login on this computer".to_owned()),
        Outcome::RateLimited => format!(
            "it says it is asked too often, so it waits until {}",
            at(app, state.next_ask_ms)
        ),
        Outcome::Unreadable => "its answer held no plan limits this version can read".to_owned(),
        Outcome::Failed => format!(
            "it could not be asked: {}",
            state.detail.as_deref().unwrap_or("no reason was given")
        ),
    })
}

/// A clause as a sentence of its own.
pub fn sentence(clause: &str) -> String {
    let mut chars = clause.chars();
    chars.next().map_or_else(String::new, |first| {
        format!("{}{}.", first.to_uppercase(), chars.as_str())
    })
}

fn at(app: &App, ms: i64) -> String {
    Timestamp::from_millisecond(ms)
        .map(|at| when(&at.to_zoned(app.zone.clone()), &app.now_zoned()))
        .unwrap_or_default()
}
