//! `claude-dashboard serve --stdio`: carries the dashboard server's requests, answers and
//! notifications between a client, such as the VS Code extension, and the server, in JSON-RPC
//! over standard input and output (ADR 0006). It ends when its standard input closes, so it never
//! outlives its client.

use std::process::ExitCode;
use std::sync::Arc;
use std::thread;

use claude_dashboard_server::app::App;
use claude_dashboard_server::{Server, statusline};
use lsp_server::{Connection, Message, Notification, Response};

pub fn run(app: App) -> ExitCode {
    update_status_line_copy(&app);
    let (connection, io_threads) = Connection::stdio();
    let client = connection.sender.clone();
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
            let response = match server.answer(&request.method, request.params) {
                Ok(value) => Response::new_ok(request.id, value),
                Err(fault) => Response::new_err(request.id, fault.code, fault.message),
            };
            if connection.sender.send(Message::Response(response)).is_err() {
                break;
            }
        }
    }
    // The writer thread ends once every sender is gone: the server's first, then ours.
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

/// Brings the status line's copy of this program up to date after an update, when there is one.
fn update_status_line_copy(app: &App) {
    let app = app.clone();
    thread::spawn(move || {
        if let Err(reason) = statusline::refresh_copy(&app) {
            eprintln!("Cannot update the status line's copy of claude-dashboard: {reason}.");
        }
    });
}
