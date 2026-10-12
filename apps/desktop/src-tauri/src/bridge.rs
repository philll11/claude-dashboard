//! What the dashboard in the window asks of the server, and what the server tells it: one command
//! carries every request, and one event every notification, as the protocol in
//! `web/protocol` describes them.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use claude_dashboard_server::app::App;
use claude_dashboard_server::{Fault, Notify, Server};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::lifecycle;
use crate::tray::Nudge;

/// The window the dashboard is in.
pub const MAIN: &str = "main";

/// The sections a start of the app may ask for (`--section`), as the dashboard names them.
const SECTIONS: [&str; 6] = [
    "overview", "sessions", "projects", "models", "limits", "settings",
];

/// The server, until the app ends, and a section asked for before the dashboard could show it.
pub struct Dashboard {
    server: Arc<Mutex<Option<Server>>>,
    show: Mutex<Option<Show>>,
}

/// A section for the dashboard to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Show {
    pub section: &'static str,
}

/// A notification from the server, as the `notification` event carries it.
#[derive(Debug, Clone, Serialize)]
struct Notification {
    method: String,
    params: Value,
}

/// Any lock here, whichever thread last held it; none of them leaves its value half-changed.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Dashboard {
    /// Starts the server, whose notifications go to the main window and nudge the item in the
    /// notification area.
    pub fn start(handle: &AppHandle, app: App, show: Option<Show>, tray: Sender<Nudge>) -> Self {
        let handle = handle.clone();
        let tray = Mutex::new(tray);
        let notify: Notify = Arc::new(move |method, params| {
            if matches!(
                method,
                "limits/changed" | "history/changed" | "settings/changed"
            ) {
                let _ = lock(&tray).send(Nudge::Changed);
            }
            let notification = Notification {
                method: method.to_owned(),
                params,
            };
            if let Err(error) = handle.emit_to(MAIN, "notification", notification) {
                log::warn!("Cannot tell the dashboard about {method}: {error}");
            }
        });
        Self {
            server: Arc::new(Mutex::new(Some(Server::start(app, notify)))),
            show: Mutex::new(show),
        }
    }
}

/// The section that `--section <name>` asks for among a start's arguments.
pub fn requested<S: AsRef<str>>(args: &[S]) -> Option<Show> {
    let at = args.iter().position(|arg| arg.as_ref() == "--section")?;
    let name = args.get(at + 1)?.as_ref();
    let section = SECTIONS.into_iter().find(|section| *section == name)?;
    Some(Show { section })
}

/// Answers one request from the dashboard, off the main thread and one at a time, as the server
/// answers them.
#[tauri::command]
pub async fn request(
    dashboard: State<'_, Dashboard>,
    method: String,
    params: Value,
) -> Result<Value, Fault> {
    let server = Arc::clone(&dashboard.server);
    tauri::async_runtime::spawn_blocking(move || {
        let mut server = lock(&server);
        let server = server.as_mut().ok_or_else(|| Fault {
            code: Fault::INTERNAL_ERROR,
            message: "The desktop app is closing.".to_owned(),
        })?;
        server.answer(&method, params)
    })
    .await
    .map_err(|error| Fault {
        code: Fault::INTERNAL_ERROR,
        message: error.to_string(),
    })?
}

/// The section the app was started to show, once.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its state by value"
)]
pub fn take_show(dashboard: State<'_, Dashboard>) -> Option<Show> {
    lock(&dashboard.show).take()
}

/// What a second start of the app does instead of opening another window: it brings this one
/// forward, at the section it asks for. A start at login, which the app's own already had,
/// changes nothing.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the single instance plugin hands over the arguments by value"
)]
pub fn second_start(app: &AppHandle, args: Vec<String>, _folder: String) {
    if args.iter().any(|arg| arg == lifecycle::HIDDEN) {
        return;
    }
    lifecycle::open(app, requested(&args).map(|show| show.section));
}

/// Stops the server's background work as the app ends.
pub fn stop(app: &AppHandle) {
    let Some(dashboard) = app.try_state::<Dashboard>() else {
        return;
    };
    let server = lock(&dashboard.server).take();
    if let Some(server) = server {
        server.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_asks_for_a_section_by_name() {
        assert_eq!(
            requested(&["claude-dashboard-desktop", "--section", "limits"]),
            Some(Show { section: "limits" })
        );
        assert_eq!(requested(&["claude-dashboard-desktop"]), None);
        assert_eq!(requested(&["claude-dashboard-desktop", "--section"]), None);
        assert_eq!(
            requested(&["claude-dashboard-desktop", "--section", "elsewhere"]),
            None
        );
    }
}
