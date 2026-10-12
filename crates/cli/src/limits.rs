//! `claude-dashboard limits endpoint`: turning Anthropic's usage endpoint on or off, and what
//! `doctor` says about it.

use std::process::ExitCode;

use claude_dashboard_core::history::History;
use claude_dashboard_core::usage_endpoint::{self, Outcome, Refresh, State};
use claude_dashboard_server::app::App;
use claude_dashboard_server::fmt::ago;
use claude_dashboard_server::limits::{CONSENT, login_note, sentence, trouble};
use jiff::Timestamp;

use crate::prompt;
use crate::report::limits_line;
use crate::style::{Paint, palette};

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
