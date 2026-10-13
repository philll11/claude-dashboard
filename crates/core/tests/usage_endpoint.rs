use std::fs;
use std::path::PathBuf;

use claude_dashboard_core::history::History;
use claude_dashboard_core::limits::{FIVE_HOUR, LimitSnapshot, SEVEN_DAY, Source};
use claude_dashboard_core::roots;
use claude_dashboard_core::usage_endpoint::{
    self, Answer, INTERVAL_MS, LoginSource, Outcome, Refresh,
};

const NOW: i64 = 1_791_640_800_000;
const MINUTE: i64 = 60 * 1000;

/// A history and a Claude Code configuration folder of their own. The folder is named the way
/// `CLAUDE_CONFIG_DIR` names one, so that on macOS its Keychain entry is one nobody has.
struct Fixture {
    config: PathBuf,
    login: LoginSource,
    history: History,
    /// Last, so that the history in it closes first: fields drop in the order they are declared,
    /// and Windows cannot remove a folder that holds an open file.
    _dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("claude-config");
        fs::create_dir_all(&config).unwrap();
        let roots = roots::data_roots(Some(config.as_os_str()), None, &[]);
        let login = LoginSource::of(&roots, Some(config.as_os_str())).unwrap();
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();
        Self {
            config,
            login,
            history,
            _dir: dir,
        }
    }

    fn signed_in_until(&self, expires_ms: i64) {
        fs::write(
            self.config.join(".credentials.json"),
            format!(
                r#"{{"claudeAiOauth": {{"accessToken": "made-up-token", "refreshToken": "made-up",
                    "expiresAt": {expires_ms}, "scopes": ["user:inference", "user:profile"]}}}}"#
            ),
        )
        .unwrap();
    }

    fn turn_on(&mut self) {
        self.history.write(|w| w.set_usage_endpoint(true)).unwrap();
    }

    /// Refreshes at `now_ms` with `answer` as what the endpoint says, and returns what the
    /// refresh did and whether it asked.
    fn refresh(&mut self, now_ms: i64, answer: Result<Answer, String>) -> (Refresh, bool) {
        let mut asked = false;
        let refresh =
            usage_endpoint::refresh_with(&mut self.history, &self.login, now_ms, |login| {
                asked = true;
                assert!(!login.expired(now_ms));
                answer
            })
            .unwrap();
        (refresh, asked)
    }
}

fn answer(status: u16, body: &str) -> Answer {
    Answer {
        status,
        body: body.to_owned(),
        retry_after_s: None,
    }
}

fn limits(five_hour: f64, weekly: f64) -> Answer {
    answer(
        200,
        &format!(
            r#"{{"five_hour": null, "seven_day": null, "limits": [
                {{"kind": "session", "percent": {five_hour}, "resets_at": "2026-10-10T15:00:00Z"}},
                {{"kind": "weekly_all", "percent": {weekly}, "resets_at": "2026-10-15T09:00:00Z"}}
            ]}}"#
        ),
    )
}

#[test]
fn an_endpoint_that_is_off_is_never_asked() {
    let mut fixture = Fixture::new();
    fixture.signed_in_until(NOW + 60 * MINUTE);

    assert_eq!(
        fixture.refresh(NOW, Ok(limits(42.0, 71.0))),
        (Refresh::Off, false)
    );
}

#[test]
fn the_answer_becomes_experimental_readings_of_both_windows() {
    let mut fixture = Fixture::new();
    fixture.signed_in_until(NOW + 60 * MINUTE);
    fixture.turn_on();

    let refreshed = fixture.refresh(NOW, Ok(limits(42.0, 71.5)));

    assert_eq!(refreshed, (Refresh::Asked(Outcome::Answered), true));
    let readings: Vec<(String, Source, i64, f64, Option<i64>)> = fixture
        .history
        .latest_limits()
        .unwrap()
        .into_iter()
        .map(|reading| {
            (
                reading.window,
                reading.source,
                reading.observed_ms,
                reading.used_percentage,
                reading.resets_at_ms,
            )
        })
        .collect();
    assert_eq!(
        readings,
        [
            (
                FIVE_HOUR.to_owned(),
                Source::UsageEndpoint,
                NOW,
                42.0,
                Some(1_791_644_400_000)
            ),
            (
                SEVEN_DAY.to_owned(),
                Source::UsageEndpoint,
                NOW,
                71.5,
                Some(1_792_054_800_000)
            ),
        ]
    );
    let state = fixture.history.usage_endpoint().unwrap();
    assert_eq!(state.outcome, Some(Outcome::Answered));
    assert_eq!(state.asked_ms, Some(NOW));
    assert_eq!(state.next_ask_ms, NOW + INTERVAL_MS);
}

#[test]
fn a_status_line_reading_from_the_last_five_minutes_needs_no_question() {
    let mut fixture = Fixture::new();
    fixture.signed_in_until(NOW + 60 * MINUTE);
    fixture.turn_on();
    fixture
        .history
        .write(|w| {
            w.limit_snapshot(&LimitSnapshot {
                window: FIVE_HOUR.to_owned(),
                source: Source::StatusLine,
                observed_ms: NOW - MINUTE,
                used_percentage: 40.0,
                resets_at_ms: None,
            })
        })
        .unwrap();

    assert_eq!(
        fixture.refresh(NOW, Ok(limits(42.0, 71.0))),
        (Refresh::Fresh, false)
    );
    assert_eq!(
        fixture.refresh(NOW + 4 * MINUTE, Ok(limits(42.0, 71.0))),
        (Refresh::Asked(Outcome::Answered), true)
    );
}

#[test]
fn every_process_together_asks_at_most_once_per_interval() {
    let mut fixture = Fixture::new();
    fixture.signed_in_until(NOW + 60 * MINUTE);
    fixture.turn_on();
    let mut other = History::open(fixture.history.path()).unwrap();
    let login = fixture.login.clone();
    let ask_other = |other: &mut History, now_ms: i64| {
        let mut asked = false;
        let refresh = usage_endpoint::refresh_with(other, &login, now_ms, |_| {
            asked = true;
            Ok(answer(200, "{}"))
        })
        .unwrap();
        (refresh, asked)
    };

    assert_eq!(
        fixture.refresh(NOW, Ok(answer(200, "{}"))),
        (Refresh::Asked(Outcome::Unreadable), true)
    );
    assert_eq!(
        ask_other(&mut other, NOW + MINUTE),
        (
            Refresh::Waiting {
                until_ms: NOW + INTERVAL_MS
            },
            false
        )
    );
    assert_eq!(
        ask_other(&mut other, NOW + INTERVAL_MS),
        (Refresh::Asked(Outcome::Unreadable), true)
    );
}

#[test]
fn an_expired_login_is_never_sent() {
    let mut fixture = Fixture::new();
    fixture.signed_in_until(NOW - MINUTE);
    fixture.turn_on();

    assert_eq!(
        fixture.refresh(NOW, Ok(limits(42.0, 71.0))),
        (Refresh::Asked(Outcome::LoginExpired), false)
    );
    assert_eq!(fixture.history.latest_limits().unwrap(), []);
}

#[test]
fn without_a_claude_ai_login_nothing_is_sent() {
    let mut fixture = Fixture::new();
    fixture.turn_on();

    assert_eq!(
        fixture.refresh(NOW, Ok(limits(42.0, 71.0))),
        (Refresh::Asked(Outcome::NoLogin), false)
    );

    fs::write(
        fixture.config.join(".credentials.json"),
        r#"{"primaryApiKey": "made-up"}"#,
    )
    .unwrap();
    assert_eq!(
        fixture.refresh(NOW + INTERVAL_MS, Ok(limits(42.0, 71.0))),
        (Refresh::Asked(Outcome::NoLogin), false)
    );
}

#[test]
fn answers_that_say_asked_too_often_lengthen_the_wait_until_one_answers() {
    let mut fixture = Fixture::new();
    fixture.signed_in_until(NOW + 600 * MINUTE);
    fixture.turn_on();
    let too_often = || answer(429, r#"{"error": {"type": "rate_limit_error"}}"#);

    assert_eq!(
        fixture.refresh(NOW, Ok(too_often())),
        (Refresh::Asked(Outcome::RateLimited), true)
    );
    assert_eq!(
        fixture.refresh(NOW + 5 * MINUTE, Ok(limits(42.0, 71.0))),
        (
            Refresh::Waiting {
                until_ms: NOW + 10 * MINUTE
            },
            false
        )
    );
    assert_eq!(
        fixture.refresh(NOW + 10 * MINUTE, Ok(too_often())),
        (Refresh::Asked(Outcome::RateLimited), true)
    );
    assert_eq!(
        fixture.history.usage_endpoint().unwrap().next_ask_ms,
        NOW + 30 * MINUTE
    );
    assert_eq!(
        fixture.refresh(NOW + 30 * MINUTE, Ok(limits(42.0, 71.0))),
        (Refresh::Asked(Outcome::Answered), true)
    );
    let state = fixture.history.usage_endpoint().unwrap();
    assert_eq!(state.rate_limited, 0);
    assert_eq!(state.next_ask_ms, NOW + 35 * MINUTE);
}

#[test]
fn a_refused_login_and_a_failed_request_are_recorded_with_what_went_wrong() {
    let mut fixture = Fixture::new();
    fixture.signed_in_until(NOW + 600 * MINUTE);
    fixture.turn_on();

    assert_eq!(
        fixture.refresh(NOW, Ok(answer(401, ""))),
        (Refresh::Asked(Outcome::LoginRefused), true)
    );
    assert_eq!(
        fixture.refresh(NOW + INTERVAL_MS, Err("connection refused".to_owned())),
        (Refresh::Asked(Outcome::Failed), true)
    );
    assert_eq!(
        fixture.history.usage_endpoint().unwrap().detail.as_deref(),
        Some("connection refused")
    );
    assert_eq!(
        fixture.refresh(NOW + 2 * INTERVAL_MS, Ok(answer(503, ""))),
        (Refresh::Asked(Outcome::Failed), true)
    );
    assert_eq!(
        fixture.history.usage_endpoint().unwrap().detail.as_deref(),
        Some("it answered with HTTP status 503")
    );
}

#[test]
fn turning_the_endpoint_off_stops_the_questions() {
    let mut fixture = Fixture::new();
    fixture.signed_in_until(NOW + 60 * MINUTE);
    fixture.turn_on();
    fixture
        .history
        .write(|w| w.set_usage_endpoint(false))
        .unwrap();

    assert_eq!(
        fixture.refresh(NOW, Ok(limits(42.0, 71.0))),
        (Refresh::Off, false)
    );
    assert!(!fixture.history.usage_endpoint().unwrap().enabled);
}
