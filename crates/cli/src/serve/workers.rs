//! The server's background work, each on a thread with a connection of its own to the history:
//! reading new transcript content, and noticing what other processes change in the history, such
//! as a status line's plan-limit readings.

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use claude_dashboard_core::history::History;
use claude_dashboard_core::ingest;
use claude_dashboard_core::usage_endpoint::{self, Refresh};
use crossbeam_channel::{Receiver, Sender, bounded, select, unbounded};
use jiff::Timestamp;
use lsp_server::{Message, Notification};
use serde::Serialize;

use super::protocol::{Days, HistoryChanged, Progress};
use super::{Shared, lock};
use crate::app::{self, App};

/// How often the transcripts are read again when nothing asks sooner.
const READ_EVERY: Duration = Duration::from_secs(60);
/// How often the history is checked for changes other processes made.
const CHECK_EVERY: Duration = Duration::from_secs(3);
/// How often the usage endpoint gets the chance of a turn, which its own rules may decline.
const ENDPOINT_EVERY: Duration = Duration::from_secs(60);
/// How often progress is reported while transcripts are read.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
/// How far back the check for changed requests looks; requests are only added or completed
/// near the present.
const RECENT_MS: i64 = 2 * 60 * 60 * 1000;

pub struct Workers {
    /// Dropping it stops the workers.
    stop: Option<Sender<()>>,
    trigger: Sender<()>,
    handles: Vec<JoinHandle<()>>,
}

impl Workers {
    /// A way to ask for new transcript content to be read now.
    pub fn trigger(&self) -> Sender<()> {
        self.trigger.clone()
    }

    pub fn stop(mut self) {
        drop(self.stop.take());
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

pub fn start(app: &App, client: &Sender<Message>, shared: &Arc<Mutex<Shared>>) -> Workers {
    let (stop, stopped) = bounded::<()>(0);
    let (trigger, triggered) = unbounded::<()>();
    let reader = {
        let (app, client, shared, stopped) = (
            app.clone(),
            client.clone(),
            Arc::clone(shared),
            stopped.clone(),
        );
        thread::spawn(move || read_transcripts(&app, &client, &shared, &triggered, &stopped))
    };
    let watcher = {
        let (app, client) = (app.clone(), client.clone());
        thread::spawn(move || watch_history(&app, &client, &stopped))
    };
    Workers {
        stop: Some(stop),
        trigger,
        handles: vec![reader, watcher],
    }
}

fn notify(client: &Sender<Message>, method: &str, params: impl Serialize) {
    let _ = client.send(Message::Notification(Notification::new(
        method.to_owned(),
        params,
    )));
}

/// Reads new transcript content now, then whenever asked, and at least every [`READ_EVERY`].
fn read_transcripts(
    app: &App,
    client: &Sender<Message>,
    shared: &Mutex<Shared>,
    triggered: &Receiver<()>,
    stopped: &Receiver<()>,
) {
    let mut history: Option<History> = None;
    loop {
        if history.is_none() {
            match app.open_history() {
                Ok(opened) => history = Some(opened),
                Err(error) => lock(shared).problems = vec![error],
            }
        }
        if let Some(history) = history.as_mut() {
            read_once(&app.at(app::now()), history, client, shared);
        }
        select! {
            recv(stopped) -> _ => return,
            recv(triggered) -> asked => {
                if asked.is_err() {
                    return;
                }
                while triggered.try_recv().is_ok() {}
            }
            default(READ_EVERY) => {}
        }
    }
}

fn read_once(app: &App, history: &mut History, client: &Sender<Message>, shared: &Mutex<Shared>) {
    let mut reported = Instant::now()
        .checked_sub(PROGRESS_EVERY)
        .unwrap_or_else(Instant::now);
    let result = ingest::ingest_with_progress(history, &app.root_paths(), |progress| {
        let progress = Progress {
            done: progress.done as u64,
            total: progress.total as u64,
        };
        lock(shared).reading = Some(progress);
        if progress.done == progress.total || reported.elapsed() >= PROGRESS_EVERY {
            reported = Instant::now();
            notify(client, "ingest/progress", progress);
        }
    });
    let mut state = lock(shared);
    state.reading = None;
    match result {
        Ok(Some(report)) => {
            state.read_ms = Some(app::now().as_millisecond());
            state.problems = report
                .failed
                .iter()
                .map(|(path, reason)| format!("{}: {reason}", path.display()))
                .collect();
            drop(state);
            if let Some((first_ms, last_ms)) = report.requests_between {
                let day = |ms: i64| {
                    Timestamp::from_millisecond(ms)
                        .map(|at| at.to_zoned(app.zone.clone()).date().to_string())
                        .unwrap_or_default()
                };
                notify(
                    client,
                    "history/changed",
                    HistoryChanged {
                        days: Some(Days {
                            first: day(first_ms),
                            last: day(last_ms),
                        }),
                    },
                );
            }
        }
        // Another process is reading; the history watch notices what it adds.
        Ok(None) => {}
        Err(error) => state.problems = vec![error.to_string()],
    }
}

/// What the watch compares to notice a change.
#[derive(Debug, Clone, PartialEq)]
struct Marks {
    limits: Option<i64>,
    endpoint: (Option<i64>, Option<&'static str>),
    requests: (u64, u64, Option<i64>),
}

/// Notices changes other processes make, such as a status line's readings and requests, and
/// gives the usage endpoint its chance of a turn.
fn watch_history(app: &App, client: &Sender<Message>, stopped: &Receiver<()>) {
    let mut history: Option<History> = None;
    let mut version: Option<i64> = None;
    let mut marks: Option<Marks> = None;
    let mut endpoint_turn = Instant::now().checked_sub(ENDPOINT_EVERY);
    loop {
        if history.is_none() {
            history = app.open_history().ok();
        }
        if let Some(history) = history.as_mut() {
            let app = app.at(app::now());
            if endpoint_turn.is_none_or(|turn| turn.elapsed() >= ENDPOINT_EVERY) {
                endpoint_turn = Some(Instant::now());
                // `data_version` leaves out this connection's own changes, so a turn this watch
                // takes is told at once.
                if let Some(login) = app.login_source()
                    && let Ok(Refresh::Asked(_)) =
                        usage_endpoint::refresh(history, &login, app.now.as_millisecond())
                {
                    notify(client, "limits/changed", ());
                }
            }
            if let Ok(current) = history.data_version()
                && version != Some(current)
            {
                version = Some(current);
                if let Some(now_marks) = read_marks(history, app.now.as_millisecond()) {
                    if let Some(before) = &marks {
                        if before.limits != now_marks.limits
                            || before.endpoint != now_marks.endpoint
                        {
                            notify(client, "limits/changed", ());
                        }
                        if before.requests != now_marks.requests {
                            notify(client, "history/changed", HistoryChanged { days: None });
                        }
                    }
                    marks = Some(now_marks);
                }
            }
        }
        select! {
            recv(stopped) -> _ => return,
            default(CHECK_EVERY) => {}
        }
    }
}

fn read_marks(history: &History, now_ms: i64) -> Option<Marks> {
    let endpoint = history.usage_endpoint().ok()?;
    Some(Marks {
        limits: history.latest_limit_ms().ok()?,
        endpoint: (
            endpoint.asked_ms,
            endpoint.outcome.map(usage_endpoint::Outcome::as_str),
        ),
        requests: history.recent_requests(now_ms - RECENT_MS).ok()?,
    })
}
