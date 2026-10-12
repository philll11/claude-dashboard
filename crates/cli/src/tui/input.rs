//! The threads that feed the interface besides the server: keys and resizes, the minutes passing,
//! and, outside Windows, the signals that end a program.

use std::thread;
use std::time::Duration;

use claude_dashboard_server::app;
use crossbeam_channel::Sender;
use ratatui::crossterm::event::{self, KeyEventKind};

use super::state::Event;

/// Reads key and resize events, as crossterm asks, on one thread alone.
pub fn keys(events: Sender<Event>) {
    thread::spawn(move || {
        loop {
            let event = match event::read() {
                // Windows reports each key as pressed and as released; only presses count.
                Ok(event::Event::Key(key)) if key.kind != KeyEventKind::Release => Event::Key(key),
                Ok(event::Event::Resize(width, height)) => Event::Resize(width, height),
                Ok(_) => continue,
                Err(error) => Event::Failed(format!("Cannot read the keys: {error}")),
            };
            let failed = matches!(event, Event::Failed(_));
            if events.send(event).is_err() || failed {
                return;
            }
        }
    });
}

/// Moves the times shown on as each minute starts.
pub fn minutes(events: Sender<Event>) {
    thread::spawn(move || {
        loop {
            let now = app::now();
            let into_minute = u64::try_from(now.as_millisecond().rem_euclid(60_000)).unwrap_or(0);
            thread::sleep(Duration::from_millis(60_000 - into_minute));
            if events.send(Event::Tick(app::now())).is_err() {
                return;
            }
        }
    });
}

/// Ends the interface, giving the terminal back, when the program is terminated or its terminal
/// hangs up.
#[cfg(unix)]
pub fn signals(events: Sender<Event>) {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
    use signal_hook::iterator::Signals;

    let Ok(mut signals) = Signals::new([SIGTERM, SIGHUP, SIGINT, SIGQUIT]) else {
        return;
    };
    thread::spawn(move || {
        if signals.forever().next().is_some() {
            let _ = events.send(Event::Quit);
        }
    });
}

#[cfg(not(unix))]
pub fn signals(_events: Sender<Event>) {}
