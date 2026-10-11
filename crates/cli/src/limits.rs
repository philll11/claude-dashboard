//! `claude-dashboard limits endpoint`: turning Anthropic's usage endpoint on or off, and what
//! the reports and `doctor` say about it.

use std::process::ExitCode;

use claude_dashboard_core::history::History;
use claude_dashboard_core::usage_endpoint::{self, Outcome, Refresh, State};
use jiff::Timestamp;

use crate::app::App;
use crate::fmt::{ago, when};
use crate::prompt;
use crate::report::limits_line;
use crate::style::{Paint, palette};

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

pub fn endpoint(app: &App, on: bool, yes: bool) -> ExitCode {
    let mut history = match app.open_history() {
        Ok(history) => history,
        Err(error) => {
            eprintln!("Cannot open the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let enabled = match history.usage_endpoint() {
        Ok(state) => state.enabled,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let paint = Paint::for_stdout();
    if on == enabled {
        println!(
            "The usage endpoint is already {}.",
            if on { "on" } else { "off" }
        );
        return ExitCode::SUCCESS;
    }
    if on {
        println!("{}\n", paint.strong("Use Anthropic's usage endpoint?"));
        for sentence in CONSENT {
            println!("{sentence}");
        }
        if !prompt::confirmed(paint, yes, "Turn it on?") {
            return ExitCode::FAILURE;
        }
    }
    if let Err(error) = history.write(|w| w.set_usage_endpoint(on)) {
        eprintln!("Cannot record the change: {error}");
        return ExitCode::FAILURE;
    }
    if !on {
        println!("The usage endpoint is off.");
        return ExitCode::SUCCESS;
    }
    println!("\nThe usage endpoint is on. Turn it off with claude-dashboard limits endpoint off.");
    // Asking once now shows whether the endpoint works with this computer's login.
    let asked = matches!(refresh(app, &mut history), Some(Refresh::Asked(_)));
    let state = history.usage_endpoint().unwrap_or_default();
    if !asked {
        println!(
            "{}",
            paint.muted("It is asked whenever no plan-limit reading is newer than 5 minutes.")
        );
    } else if state.outcome == Some(Outcome::Answered) {
        let limits = history.latest_limits().unwrap_or_default();
        println!("{}", limits_line(app, paint, &limits));
    } else if let Some(note) = login_note(&state) {
        println!("{}", paint.paint(note, palette::WARNING, false));
    } else if let Some(trouble) = trouble(app, &state) {
        println!(
            "{}",
            paint.paint(&sentence(&trouble), palette::WARNING, false)
        );
    }
    ExitCode::SUCCESS
}

/// Asks the usage endpoint for the plan limits when it is on and its turn has come. What comes
/// of it is recorded in the history, so a report never fails over it.
pub fn refresh(app: &App, history: &mut History) -> Option<Refresh> {
    let login = app.login_source()?;
    usage_endpoint::refresh(history, &login, app.now.as_millisecond()).ok()
}

/// What a report says under the plan limits while the endpoint needs Claude Code's login
/// renewed, which only the person can do.
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
fn trouble(app: &App, state: &State) -> Option<String> {
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

/// The `doctor` line for the endpoint while it is on: whether it works, and what to say.
pub fn doctor_detail(app: &App, state: &State) -> (bool, String) {
    match (state.outcome, state.asked_ms) {
        (Some(Outcome::Answered), Some(asked_ms)) => {
            let asked = Timestamp::from_millisecond(asked_ms)
                .map(|at| ago(at, app.now))
                .unwrap_or_default();
            (
                true,
                format!("usage endpoint on (experimental), last answer {asked}"),
            )
        }
        (Some(_), Some(_)) => (
            false,
            format!(
                "usage endpoint on (experimental): {}",
                trouble(app, state).unwrap_or_default()
            ),
        ),
        _ => (
            true,
            "usage endpoint on (experimental), no answer yet".to_owned(),
        ),
    }
}

/// A clause as a sentence of its own.
fn sentence(clause: &str) -> String {
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
