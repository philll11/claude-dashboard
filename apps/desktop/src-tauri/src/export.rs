//! Exports from the dashboard. The app opens the system's save dialog itself, so that the
//! dashboard never names a file and needs no access to the file system, then writes the export
//! on a thread of its own while the dashboard keeps answering. Quitting cancels an export under
//! way and waits for it, so that it never leaves half a file.

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use claude_dashboard_server::Fault;
use claude_dashboard_server::app::App;
use claude_dashboard_server::export::{self, ExportError};
use claude_dashboard_server::protocol::{ExportFormat, ExportParams, ExportProgress, Exported};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt as _;

use crate::bridge::MAIN;

/// The environment variable that names a folder to save exports in without asking, as the
/// end-to-end tests do, since they cannot drive the system's save dialog.
const EXPORT_TO_ENV: &str = "CLAUDE_DASHBOARD_EXPORT_TO";

/// How often an export reports its progress.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// How long quitting waits for a cancelled export to clean up.
const STOP_WAIT: Duration = Duration::from_secs(10);

/// The export under way, one at a time, by the flag that cancels it.
pub struct Exports {
    base: App,
    running: Arc<(Mutex<Option<Arc<AtomicBool>>>, Condvar)>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A notification for the dashboard, as the server's travel.
#[derive(Debug, Clone, Serialize)]
struct Notification {
    method: &'static str,
    params: ExportProgress,
}

impl Exports {
    pub fn new(base: App) -> Self {
        Self {
            base,
            running: Arc::new((Mutex::new(None), Condvar::new())),
        }
    }

    /// Cancels the export under way and waits until it has cleaned up.
    pub fn stop(&self) {
        let (running, ended) = &*self.running;
        let mut current = lock(running);
        if let Some(cancelled) = current.as_ref() {
            cancelled.store(true, Ordering::Relaxed);
        }
        let deadline = Instant::now() + STOP_WAIT;
        while current.is_some() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            current = ended
                .wait_timeout(current, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

fn fault(message: impl Into<String>) -> Fault {
    Fault {
        code: Fault::INTERNAL_ERROR,
        message: message.into(),
    }
}

/// Saves an export where the person chooses in the system's save dialog; `None` when they choose
/// no file.
#[tauri::command]
pub async fn export(
    app: AppHandle,
    exports: State<'_, Exports>,
    params: ExportParams,
) -> Result<Option<Exported>, Fault> {
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut running = lock(&exports.running.0);
        if running.is_some() {
            return Err(fault(
                "An export is under way; wait for it to finish, or cancel it.",
            ));
        }
        *running = Some(Arc::clone(&cancelled));
    }
    let (base, running) = (exports.base.clone(), Arc::clone(&exports.running));
    tauri::async_runtime::spawn_blocking(move || {
        let result = save(&app, &base, &params, &cancelled);
        let (current, ended) = &*running;
        *lock(current) = None;
        ended.notify_all();
        result
    })
    .await
    .map_err(|error| fault(error.to_string()))?
}

/// Cancels the export under way.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its state by value"
)]
pub fn export_cancel(exports: State<'_, Exports>) {
    if let Some(cancelled) = lock(&exports.running.0).as_ref() {
        cancelled.store(true, Ordering::Relaxed);
    }
}

fn save(
    app: &AppHandle,
    base: &App,
    params: &ExportParams,
    cancelled: &AtomicBool,
) -> Result<Option<Exported>, Fault> {
    let name = export::name(base, params).map_err(fault)?;
    let Some(path) = destination(app, params, &name)? else {
        return Ok(None);
    };
    let mut reported = Instant::now().checked_sub(PROGRESS_EVERY);
    let mut progress = |written: u64, total: u64| {
        if cancelled.load(Ordering::Relaxed) {
            return ControlFlow::Break(());
        }
        if reported.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY) {
            reported = Some(Instant::now());
            let notification = Notification {
                method: "export/progress",
                params: ExportProgress {
                    written,
                    total,
                    path: Some(path.display().to_string()),
                },
            };
            let _ = app.emit_to(MAIN, "notification", notification);
        }
        ControlFlow::Continue(())
    };
    match export::save(base, params, &path, &mut progress) {
        Ok(exported) => Ok(Some(exported)),
        Err(ExportError::Cancelled) => Err(Fault {
            code: Fault::REQUEST_CANCELLED,
            message: ExportError::Cancelled.to_string(),
        }),
        Err(ExportError::Failed(reason)) => Err(fault(reason)),
    }
}

/// Where the person wants the export saved, in the system's save dialog, starting in their
/// Downloads folder with the suggested name.
fn destination(
    app: &AppHandle,
    params: &ExportParams,
    name: &str,
) -> Result<Option<PathBuf>, Fault> {
    if let Some(folder) = std::env::var_os(EXPORT_TO_ENV).filter(|folder| !folder.is_empty()) {
        return Ok(Some(PathBuf::from(folder).join(name)));
    }
    let (kind, extension) = match params.format {
        ExportFormat::Csv => ("CSV", "csv"),
        ExportFormat::Json => ("JSON", "json"),
    };
    let mut dialog = app
        .dialog()
        .file()
        .set_title(format!("Export as {kind}"))
        .set_file_name(name)
        .add_filter(kind, &[extension]);
    if let Ok(downloads) = app.path().download_dir() {
        dialog = dialog.set_directory(downloads);
    }
    if let Some(window) = app.get_webview_window(MAIN) {
        dialog = dialog.set_parent(&window);
    }
    match dialog.blocking_save_file() {
        None => Ok(None),
        Some(path) => path
            .into_path()
            .map(Some)
            .map_err(|error| fault(format!("Cannot save there: {error}"))),
    }
}
