//! `claude-dashboard serve --stdio`: carries the dashboard server's requests, answers and
//! notifications between a client, such as the VS Code extension, and the server, in JSON-RPC
//! over standard input and output (ADR 0006). It ends when its standard input closes, so it never
//! outlives its client.
//!
//! An export is answered from a thread of its own, out of order as JSON-RPC allows, so that the
//! dashboard keeps answering while it writes.

use std::ops::ControlFlow;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use claude_dashboard_server::app::App;
use claude_dashboard_server::export::{self, ExportError};
use claude_dashboard_server::protocol::{ExportParams, ExportProgress, ExportSave, Exported};
use claude_dashboard_server::{Fault, Server, statusline};
use crossbeam_channel::Sender;
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use serde::de::DeserializeOwned;

/// How often an export reports its progress.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

pub fn run(app: App) -> ExitCode {
    update_status_line_copy(&app);
    let (connection, io_threads) = Connection::stdio();
    let client = connection.sender.clone();
    let mut exports = Exports {
        app: app.clone(),
        client: connection.sender.clone(),
        running: None,
    };
    let mut server = Server::start(
        app,
        Arc::new(move |method, params| {
            let _ = client.send(Message::Notification(Notification::new(
                method.to_owned(),
                params,
            )));
        }),
    );
    for message in &connection.receiver {
        if let Message::Request(request) = message {
            let response = match request.method.as_str() {
                "export/save" | "export/content" => {
                    exports.start(request);
                    continue;
                }
                "export/cancel" => {
                    exports.cancel();
                    Response::new_ok(request.id, serde_json::Value::Null)
                }
                _ => match server.answer(&request.method, request.params) {
                    Ok(value) => Response::new_ok(request.id, value),
                    Err(fault) => Response::new_err(request.id, fault.code, fault.message),
                },
            };
            if connection.sender.send(Message::Response(response)).is_err() {
                break;
            }
        }
    }
    // The writer thread ends once every sender is gone: an export's and the server's first, then
    // ours.
    exports.stop();
    server.stop();
    drop(connection);
    match io_threads.join() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("The connection to the client failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The export under way, one at a time.
struct Exports {
    app: App,
    client: Sender<Message>,
    running: Option<Running>,
}

/// An export's thread, and what it shares with the server.
struct Running {
    cancelled: Arc<AtomicBool>,
    /// Set just before the export answers, so that a request the client sends on that answer can
    /// start another.
    answered: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Exports {
    fn start(&mut self, request: Request) {
        if self
            .running
            .as_ref()
            .is_some_and(|running| !running.answered.load(Ordering::Acquire))
        {
            let _ = self.client.send(Message::Response(Response::new_err(
                request.id,
                Fault::INVALID_PARAMS,
                "An export is under way; wait for it to finish, or cancel it.".to_owned(),
            )));
            return;
        }
        // An export that has answered has nothing left to do but end.
        if let Some(finished) = self.running.take() {
            let _ = finished.handle.join();
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let answered = Arc::new(AtomicBool::new(false));
        let (app, client, stop, done) = (
            self.app.clone(),
            self.client.clone(),
            Arc::clone(&cancelled),
            Arc::clone(&answered),
        );
        let handle = thread::spawn(move || {
            let id = request.id.clone();
            let result = export(&app, &client, &stop, request);
            done.store(true, Ordering::Release);
            let _ = client.send(Message::Response(response(id, result)));
        });
        self.running = Some(Running {
            cancelled,
            answered,
            handle,
        });
    }

    fn cancel(&self) {
        if let Some(running) = &self.running {
            running.cancelled.store(true, Ordering::Relaxed);
        }
    }

    /// Cancels the export under way and waits for it, so that it never leaves a file behind.
    fn stop(mut self) {
        self.cancel();
        if let Some(running) = self.running.take() {
            let _ = running.handle.join();
        }
    }
}

fn parse<T: DeserializeOwned>(params: serde_json::Value) -> Result<T, Fault> {
    serde_json::from_value(params).map_err(|error| Fault {
        code: Fault::INVALID_PARAMS,
        message: error.to_string(),
    })
}

/// Runs the export `request` asks for, telling the client how far it has got.
fn export(
    app: &App,
    client: &Sender<Message>,
    cancelled: &AtomicBool,
    request: Request,
) -> Result<Exported, Fault> {
    let save: Option<ExportSave> = if request.method == "export/save" {
        Some(parse(request.params.clone())?)
    } else {
        None
    };
    let path = save.as_ref().map(|save| save.path.clone());
    let mut reported = Instant::now().checked_sub(PROGRESS_EVERY);
    let mut progress = |written: u64, total: u64| {
        if cancelled.load(Ordering::Relaxed) {
            return ControlFlow::Break(());
        }
        if reported.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY) {
            reported = Some(Instant::now());
            let progress = ExportProgress {
                written,
                total,
                path: path.clone(),
            };
            if let Ok(params) = serde_json::to_value(progress) {
                let _ = client.send(Message::Notification(Notification::new(
                    "export/progress".to_owned(),
                    params,
                )));
            }
        }
        ControlFlow::Continue(())
    };
    let result = if let Some(save) = &save {
        let path = Path::new(&save.path);
        if !path.is_absolute() {
            return Err(Fault {
                code: Fault::INVALID_PARAMS,
                message: format!("{} is not a full path to a file", path.display()),
            });
        }
        export::save(app, &save.export, path, &mut progress)
    } else {
        let params: ExportParams = parse(request.params)?;
        export::content(app, &params, &mut progress)
    };
    result.map_err(|error| Fault {
        code: match error {
            ExportError::Cancelled => Fault::REQUEST_CANCELLED,
            ExportError::Failed(_) => Fault::INTERNAL_ERROR,
        },
        message: error.to_string(),
    })
}

fn response(id: RequestId, result: Result<Exported, Fault>) -> Response {
    match result.and_then(|exported| {
        serde_json::to_value(exported).map_err(|error| Fault {
            code: Fault::INTERNAL_ERROR,
            message: error.to_string(),
        })
    }) {
        Ok(value) => Response::new_ok(id, value),
        Err(fault) => Response::new_err(id, fault.code, fault.message),
    }
}

/// Brings the status line's copy of this program up to date after an update, when there is one.
fn update_status_line_copy(app: &App) {
    let app = app.clone();
    thread::spawn(move || {
        if let Err(reason) = statusline::refresh_copy(&app) {
            eprintln!("Cannot update the status line's copy of claude-dashboard: {reason}.");
        }
    });
}
