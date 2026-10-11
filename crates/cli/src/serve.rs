//! `claude-dashboard serve --stdio`: answers a client, such as the VS Code extension, in JSON-RPC
//! over standard input and output (ADR 0006), and tells it when the history or the plan limits
//! change. It ends when its standard input closes, so it never outlives its client.

mod protocol;
mod requests;
mod workers;

use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use lsp_server::{Connection, Message};

use crate::app::App;
use crate::program;

/// What the background work knows that answers need.
#[derive(Debug, Default)]
struct Shared {
    /// Set while new transcript content is read.
    reading: Option<protocol::Progress>,
    /// When reading new transcript content last finished.
    read_ms: Option<i64>,
    /// What could not be read the last time, with the reason.
    problems: Vec<String>,
}

/// The shared state, whichever thread last held it; none of them leaves it half-changed.
fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn run(app: &App) -> ExitCode {
    update_status_line_copy(app);
    let (connection, io_threads) = Connection::stdio();
    let shared = Arc::new(Mutex::new(Shared::default()));
    let workers = workers::start(app, &connection.sender, &shared);
    let mut server = requests::Server::new(app.clone(), Arc::clone(&shared), workers.triggers());
    for message in &connection.receiver {
        if let Message::Request(request) = message {
            let response = server.answer(request);
            if connection.sender.send(Message::Response(response)).is_err() {
                break;
            }
        }
    }
    // The writer thread ends once every sender is gone: the workers' first, then ours.
    workers.stop();
    drop(server);
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
    let Some(copy) = program::copy_path(app).filter(|copy| app.status_line_copy && copy.is_file())
    else {
        return;
    };
    thread::spawn(move || {
        if let Err(reason) = program::keep_copy(&copy) {
            eprintln!("Cannot update the status line's copy of claude-dashboard: {reason}.");
        }
    });
}
