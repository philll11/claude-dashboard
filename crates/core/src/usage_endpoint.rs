//! Plan limits from Anthropic's usage endpoint, when the person turns it on (ADR 0005).
//!
//! The endpoint is undocumented, so its readings are experimental, and it needs Claude Code's
//! login. claude-dashboard reads that login without changing it, never refreshes it, and sends
//! it only to the endpoint, over HTTPS, checked against the operating system's certificates.
//! Every process on the machine together asks at most once per [`INTERVAL_MS`], and only when no
//! reading is newer than that, and waits longer after the endpoint says it is asked too often.

mod answer;
mod login;

pub use answer::plan_limits;
pub use login::{Login, LoginError, LoginSource};

use crate::history::{History, HistoryError};
use crate::http;
use crate::limits::{FIVE_HOUR, LimitSnapshot};

/// Where the endpoint is. The login goes nowhere else.
pub const URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// The beta header the endpoint needs.
const BETA: &str = "oauth-2025-04-20";

/// How often every process on the machine together asks the endpoint at most.
pub const INTERVAL_MS: i64 = 5 * 60 * 1000;

/// The longest wait after the endpoint says it is asked too often, unless it asks for longer.
const LONGEST_WAIT_MS: i64 = 60 * 60 * 1000;

/// What came of asking the endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It answered with plan limits.
    Answered,
    /// Claude Code has no claude.ai login on this computer, or it cannot be read.
    NoLogin,
    /// Claude Code's login has expired, and only Claude Code renews it.
    LoginExpired,
    /// The endpoint did not accept the login, such as one Claude Code has replaced since.
    LoginRefused,
    /// The endpoint said it is asked too often.
    RateLimited,
    /// The answer held no plan limits this version can read.
    Unreadable,
    /// The endpoint could not be reached, or answered with an error.
    Failed,
}

impl Outcome {
    /// The name the history stores the outcome under.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Answered => "answered",
            Self::NoLogin => "no-login",
            Self::LoginExpired => "login-expired",
            Self::LoginRefused => "login-refused",
            Self::RateLimited => "rate-limited",
            Self::Unreadable => "unreadable",
            Self::Failed => "failed",
        }
    }

    /// The outcome the history stores under `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Answered,
            Self::NoLogin,
            Self::LoginExpired,
            Self::LoginRefused,
            Self::RateLimited,
            Self::Unreadable,
            Self::Failed,
        ]
        .into_iter()
        .find(|outcome| outcome.as_str() == name)
    }
}

/// The endpoint's state on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    /// Whether the person has turned the endpoint on.
    pub enabled: bool,
    /// No process asks before this time, in milliseconds since the Unix epoch.
    pub next_ask_ms: i64,
    /// Answers in a row that said the endpoint is asked too often.
    pub rate_limited: u32,
    /// When a process last took its turn to ask.
    pub asked_ms: Option<i64>,
    /// What came of that; `None` while the process is still asking.
    pub outcome: Option<Outcome>,
    /// What went wrong, for an outcome that needs it.
    pub detail: Option<String>,
}

/// What a refresh did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// The endpoint is off.
    Off,
    /// A reading of the five-hour window is newer than [`INTERVAL_MS`], such as one the status
    /// line recorded.
    Fresh,
    /// Another process asked within the interval, or the endpoint asked for a longer wait.
    Waiting { until_ms: i64 },
    /// This process took its turn, with this outcome.
    Asked(Outcome),
}

/// What the endpoint answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub body: String,
    /// The wait the endpoint asked for, in seconds.
    pub retry_after_s: Option<u64>,
}

/// Asks the endpoint for the plan limits and records what it answers, when the endpoint is on,
/// no reading is fresh and it is this machine's turn.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read or written. Whatever goes wrong with the
/// login or the request is recorded as the outcome instead.
pub fn refresh(
    history: &mut History,
    login: &LoginSource,
    now_ms: i64,
) -> Result<Refresh, HistoryError> {
    refresh_with(history, login, now_ms, ask)
}

/// [`refresh`], with the request made by `ask`.
///
/// # Errors
///
/// As for [`refresh`].
pub fn refresh_with(
    history: &mut History,
    login: &LoginSource,
    now_ms: i64,
    ask: impl FnOnce(&Login) -> Result<Answer, String>,
) -> Result<Refresh, HistoryError> {
    let state = history.usage_endpoint()?;
    if !state.enabled {
        return Ok(Refresh::Off);
    }
    let fresh = history.latest_limits()?.iter().any(|limit| {
        limit.window == FIVE_HOUR && now_ms.saturating_sub(limit.observed_ms) < INTERVAL_MS
    });
    if fresh {
        return Ok(Refresh::Fresh);
    }
    if !history.write(|w| w.take_usage_endpoint_turn(now_ms, INTERVAL_MS))? {
        return Ok(Refresh::Waiting {
            until_ms: history.usage_endpoint()?.next_ask_ms,
        });
    }
    let asked = match login.read() {
        Err(LoginError::Missing) => Asked::outcome(Outcome::NoLogin),
        Err(error) => Asked::failed(Outcome::NoLogin, error.to_string()),
        Ok(login) if login.expired(now_ms) => Asked::outcome(Outcome::LoginExpired),
        Ok(login) => match ask(&login) {
            Ok(answer) => Asked::from_answer(&answer, now_ms, state.rate_limited),
            Err(error) => Asked::failed(Outcome::Failed, error),
        },
    };
    history.write(|w| {
        for snapshot in &asked.snapshots {
            w.limit_snapshot(snapshot)?;
        }
        w.usage_endpoint_outcome(asked.outcome, asked.detail.as_deref(), asked.wait_until_ms)
    })?;
    Ok(Refresh::Asked(asked.outcome))
}

/// What one turn found, ready to record.
struct Asked {
    outcome: Outcome,
    detail: Option<String>,
    snapshots: Vec<LimitSnapshot>,
    wait_until_ms: Option<i64>,
}

impl Asked {
    fn outcome(outcome: Outcome) -> Self {
        Self {
            outcome,
            detail: None,
            snapshots: Vec::new(),
            wait_until_ms: None,
        }
    }

    fn failed(outcome: Outcome, detail: String) -> Self {
        Self {
            detail: Some(detail),
            ..Self::outcome(outcome)
        }
    }

    fn from_answer(answer: &Answer, now_ms: i64, rate_limited: u32) -> Self {
        match answer.status {
            200 => {
                let snapshots = plan_limits(&answer.body, now_ms);
                if snapshots.is_empty() {
                    Self::outcome(Outcome::Unreadable)
                } else {
                    Self {
                        snapshots,
                        ..Self::outcome(Outcome::Answered)
                    }
                }
            }
            401 => Self::outcome(Outcome::LoginRefused),
            429 => Self {
                wait_until_ms: Some(now_ms.saturating_add(wait_after_rate_limit(
                    rate_limited.saturating_add(1),
                    answer.retry_after_s,
                ))),
                ..Self::outcome(Outcome::RateLimited)
            },
            status => Self::failed(
                Outcome::Failed,
                format!("it answered with HTTP status {status}"),
            ),
        }
    }
}

/// How long to wait after the `count`th answer in a row that said the endpoint is asked too
/// often: twice the interval, doubling each time up to an hour, or longer when the endpoint asks
/// for that.
fn wait_after_rate_limit(count: u32, retry_after_s: Option<u64>) -> i64 {
    let doubled = INTERVAL_MS.saturating_mul(1_i64 << count.clamp(1, 16));
    let wait = doubled.min(LONGEST_WAIT_MS);
    let asked_for = retry_after_s
        .and_then(|seconds| i64::try_from(seconds).ok())
        .map_or(0, |seconds| seconds.saturating_mul(1000));
    wait.max(asked_for)
}

/// Asks the endpoint without following redirects, so that the login goes only to [`URL`].
fn ask(login: &Login) -> Result<Answer, String> {
    let authorization = format!("Bearer {}", login.token());
    let answer = http::get(&http::Get {
        url: URL,
        headers: &[
            ("Authorization", &authorization),
            ("anthropic-beta", BETA),
            ("Accept", "application/json"),
        ],
        redirect_hosts: &[],
    })?;
    Ok(Answer {
        status: answer.status,
        body: answer.body,
        retry_after_s: answer.retry_after_s,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: i64 = 60 * 1000;

    #[test]
    fn each_answer_that_says_asked_too_often_doubles_the_wait_up_to_an_hour() {
        let waits: Vec<i64> = (1..=5)
            .map(|count| wait_after_rate_limit(count, None) / MINUTE)
            .collect();
        assert_eq!(waits, [10, 20, 40, 60, 60]);
    }

    #[test]
    fn a_longer_wait_the_endpoint_asks_for_wins() {
        assert_eq!(wait_after_rate_limit(1, Some(7_200)) / MINUTE, 120);
        assert_eq!(wait_after_rate_limit(1, Some(30)) / MINUTE, 10);
    }

    #[test]
    fn outcomes_keep_their_stored_names() {
        for outcome in [
            Outcome::Answered,
            Outcome::NoLogin,
            Outcome::LoginExpired,
            Outcome::LoginRefused,
            Outcome::RateLimited,
            Outcome::Unreadable,
            Outcome::Failed,
        ] {
            assert_eq!(Outcome::from_name(outcome.as_str()), Some(outcome));
        }
        assert_eq!(Outcome::from_name("later"), None);
    }
}
