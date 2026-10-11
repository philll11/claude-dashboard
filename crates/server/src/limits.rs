//! What the dashboard and the reports say about Anthropic's usage endpoint: the consent turning it
//! on asks for, and what went wrong the last time it was asked.

use claude_dashboard_core::usage_endpoint::{Outcome, State};
use jiff::Timestamp;

use crate::app::App;
use crate::fmt::when;

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
