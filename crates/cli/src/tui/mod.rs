//! The dashboard in the terminal: `claude-dashboard` without a command, in a terminal.
//!
//! It reads through the dashboard's server, run in this process on a thread of its own (ADR
//! 0009), so it shows what the desktop app and VS Code show. A thread reads the keys. The state
//! changes only by events and is drawn whole after each, which ratatui turns into the few cells
//! that changed (The Elm Architecture).

mod answers;
mod canvas;
mod data;
mod input;
mod state;
mod theme;
mod view;

#[cfg(test)]
mod tests;

use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use claude_dashboard_server::app::{self, App};
use crossbeam_channel::{RecvTimeoutError, unbounded};

use self::data::Data;
use self::state::{Event, Flow, State};
use self::theme::Theme;

/// How long quitting waits for the server to stop before it ends the program anyway.
const STOP_PATIENCE: Duration = Duration::from_secs(2);

/// Whether this is a terminal the interface can take: input and output are terminals, and
/// `TERM` is not `dumb`.
pub fn can_run() -> bool {
    std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::env::var_os("TERM").is_none_or(|term| term != "dumb")
}

pub fn run(app: App) -> ExitCode {
    // The terminal is asked for its colours before raw mode, which the question needs to itself.
    let theme = Theme::for_terminal();
    // NO_COLOR is applied here, by the theme; crossterm's own handling would write a bare reset
    // for each colour, which also clears bold and reverse video.
    ratatui::crossterm::style::force_color_output(true);
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = ratatui::try_restore();
            eprintln!(
                "Cannot take over this terminal: {error}. In Git Bash's own window, run claude-dashboard in Windows Terminal, or through winpty."
            );
            return ExitCode::FAILURE;
        }
    };
    let size = terminal
        .size()
        .map_or((80, 24), |size| (size.width, size.height));
    let (events, inbox) = unbounded::<Event>();
    input::keys(events.clone());
    input::minutes(events.clone());
    input::signals(events.clone());
    let data = Data::start(app, events);
    let mut state = State::new(theme, size, app::now());
    let mut failure = None;
    loop {
        // Each frame is laid out for the terminal's size as it is, even when no event said so.
        if let Ok(now) = terminal.size()
            && (now.width, now.height) != (state.width, state.height)
        {
            state.update(Event::Resize(now.width, now.height));
        }
        for ask in state.asks() {
            data.ask(ask);
        }
        if let Err(error) = terminal.draw(|frame| view::draw(frame.buffer_mut(), &state)) {
            failure = Some(format!("Cannot draw in this terminal: {error}"));
            break;
        }
        let first = match state.deadline() {
            Some(due) => match inbox.recv_timeout(due.saturating_duration_since(Instant::now())) {
                Ok(event) => Some(event),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match inbox.recv() {
                Ok(event) => Some(event),
                Err(_) => break,
            },
        };
        let Some(first) = first else {
            state.at(Instant::now());
            continue;
        };
        // Answers and notifications often come in a burst; one frame shows them all.
        let mut quit = false;
        for event in std::iter::once(first).chain(inbox.try_iter()) {
            if let Event::Failed(reason) = &event {
                failure = Some(reason.clone());
            }
            if state.update(event) == Flow::Quit {
                quit = true;
                break;
            }
        }
        if quit {
            break;
        }
    }
    ratatui::restore();
    data.stop(STOP_PATIENCE);
    match failure {
        Some(reason) => {
            eprintln!("{reason}");
            ExitCode::FAILURE
        }
        None => ExitCode::SUCCESS,
    }
}
