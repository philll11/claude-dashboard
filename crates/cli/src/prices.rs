//! `claude-dashboard prices`: the price table in use, and the check for a newer one in
//! claude-dashboard's repository.

use std::process::ExitCode;

use claude_dashboard_core::history::History;
use claude_dashboard_core::price_refresh::{self, Check, Outcome, State, When};
use claude_dashboard_core::pricing::PriceTable;
use claude_dashboard_server::app::App;
use claude_dashboard_server::fmt::{ago, short_date};
use jiff::Timestamp;

use crate::style::Paint;

fn open(app: &App) -> Result<History, ExitCode> {
    app.open_history().map_err(|error| {
        eprintln!("Cannot open the history: {error}");
        ExitCode::FAILURE
    })
}

/// `claude-dashboard prices`: the table in use and the daily check.
pub fn status(app: &App) -> ExitCode {
    let history = match open(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    let paint = Paint::for_stdout();
    let state = price_refresh::state(&history.settings().unwrap_or_default());
    println!("{}", table_words(app, &state));
    if state.enabled {
        let mut line = "Daily check: on. A running dashboard checks once a day".to_owned();
        if let Some(checked) = last_check(app, &state) {
            line.push_str("; ");
            line.push_str(&checked);
        }
        line.push('.');
        println!("{line}");
    } else {
        println!(
            "Daily check: off. {} switches it on, and {} checks once now.",
            paint.strong("claude-dashboard prices refresh on"),
            paint.strong("claude-dashboard prices update")
        );
        if let Some(checked) = last_check(app, &state) {
            println!("{}", sentence(&checked));
        }
    }
    ExitCode::SUCCESS
}

/// `claude-dashboard prices refresh on|off`.
pub fn switch(app: &App, on: bool) -> ExitCode {
    let mut history = match open(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    if let Err(error) = price_refresh::switch(&mut history, on) {
        eprintln!("Cannot change the setting: {error}");
        return ExitCode::FAILURE;
    }
    if on {
        println!(
            "The daily check for a newer price table is on. A running dashboard checks claude-dashboard's repository on GitHub once a day."
        );
    } else {
        println!(
            "The daily check for a newer price table is off. The prices in use stay as they are."
        );
    }
    ExitCode::SUCCESS
}

/// `claude-dashboard prices update`: checks for a newer table now.
pub fn update(app: &App) -> ExitCode {
    let mut history = match open(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    let outcome = match price_refresh::refresh(&mut history, app.now.as_millisecond(), When::Now) {
        Ok(Check::Checked(outcome)) => outcome,
        Ok(_) => return ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Cannot check for a newer price table: {error}");
            return ExitCode::FAILURE;
        }
    };
    let settings = history.settings().unwrap_or_default();
    let state = price_refresh::state(&settings);
    let checked = short_date(price_refresh::in_use(&settings).checked());
    match outcome {
        Outcome::Updated => {
            println!("Updated to the prices of {checked} from claude-dashboard's repository.");
            ExitCode::SUCCESS
        }
        Outcome::UpToDate => {
            println!("The prices of {checked} are up to date.");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "{}",
                sentence(&outcome_words(outcome, state.detail.as_deref()))
            );
            ExitCode::FAILURE
        }
    }
}

/// Which table is in use and where it came from.
pub fn table_words(app: &App, state: &State) -> String {
    let checked = short_date(app.prices.checked());
    match state
        .downloaded_ms
        .filter(|_| !PriceTable::is_bundled(&app.prices))
    {
        Some(downloaded_ms) => format!(
            "Prices of {checked}, from Anthropic's pricing page, downloaded {} from claude-dashboard's repository.",
            moment(app, downloaded_ms)
        ),
        None => format!(
            "Prices of {checked}, from Anthropic's pricing page, as this version of claude-dashboard has them."
        ),
    }
}

/// When the last check ended and what came of it, such as `last checked 2 hours ago: up to date`.
pub fn last_check(app: &App, state: &State) -> Option<String> {
    let checked_ms = state.checked_ms?;
    let outcome = state.outcome?;
    Some(format!(
        "last checked {}: {}",
        moment(app, checked_ms),
        outcome_words(outcome, state.detail.as_deref())
    ))
}

/// What came of a check, as the end of a sentence.
pub fn outcome_words(outcome: Outcome, detail: Option<&str>) -> String {
    match outcome {
        Outcome::Updated => "a newer price table arrived".to_owned(),
        Outcome::UpToDate => "up to date".to_owned(),
        Outcome::Unreachable => format!(
            "cannot reach claude-dashboard's repository: {}",
            detail.unwrap_or("no answer")
        ),
        Outcome::NewerFormat => "the newest price table needs a newer claude-dashboard".to_owned(),
        Outcome::Refused => format!(
            "the newest price table cannot be used: {}",
            detail.unwrap_or("it is not valid")
        ),
    }
}

fn moment(app: &App, ms: i64) -> String {
    Timestamp::from_millisecond(ms)
        .map(|at| ago(at, app.now))
        .unwrap_or_default()
}

/// `text` as a sentence: a capital first letter and a full stop.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    let mut sentence: String = chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    if !sentence.ends_with('.') {
        sentence.push('.');
    }
    sentence
}
