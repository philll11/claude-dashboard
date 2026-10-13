//! Starting the app: the server, the window, and the `claude-dashboard` program the app brings,
//! which Claude Code's status line runs a copy of.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use claude_dashboard_server::app::{App, DESKTOP_APP_SETTING};
use claude_dashboard_server::{program, statusline};
use tauri::{AppHandle, Manager, Url, WebviewWindow, WebviewWindowBuilder};

use crate::bridge::{self, Dashboard, MAIN};
use crate::export::Exports;
use crate::lifecycle::{self, Life};
use crate::tray::Tray;

/// How long the window waits for the dashboard to draw before it shows anyway.
const SHOW_ANYWAY_AFTER: Duration = Duration::from_secs(5);

/// Starts the server, adds the item to the notification area and opens the window, which a start
/// at login keeps hidden.
///
/// # Errors
///
/// Why the app cannot start, such as a window its configuration does not describe.
pub fn app(tauri: &mut tauri::App) -> Result<(), Box<dyn Error>> {
    let app = App {
        status_line_copy: Some(bundled_program()?),
        ..App::new().with_shared_settings()
    };
    let args: Vec<String> = std::env::args().collect();
    let show = bridge::requested(&args);
    let hidden = args.iter().any(|arg| arg == lifecycle::HIDDEN);
    let (nudge, nudges) = mpsc::channel();
    tauri.manage(Dashboard::start(
        tauri.handle(),
        app.clone(),
        show,
        nudge.clone(),
    ));
    let tray = Tray::start(tauri.handle(), app.clone(), (nudge, nudges));
    let stays_hidden = hidden && tray.is_some();
    tauri.manage(Life::new(app.clone(), tray, stays_hidden));
    tauri.manage(Exports::new(app.clone()));
    let window = window(tauri.handle())?;
    if !stays_hidden {
        show_anyway(tauri.handle().clone(), window);
    }
    let product = tauri.package_info().name.clone();
    thread::spawn(move || {
        if let Err(reason) = statusline::refresh_copy(&app) {
            log::warn!("Cannot update the status line's copy of claude-dashboard: {reason}.");
        }
        record_location(&app);
        if cfg!(windows) {
            let removed = crate::updater::remove_downloads(&std::env::temp_dir(), &product);
            if removed > 0 {
                log::info!(
                    "Removed {removed} installers the updater left in the temporary folder."
                );
            }
        }
    });
    Ok(())
}

/// The `claude-dashboard` program the app brings, next to its own (Tauri's `externalBin`).
fn bundled_program() -> Result<PathBuf, Box<dyn Error>> {
    let this = std::env::current_exe()?;
    let folder = this.parent().ok_or("the app's program has no folder")?;
    Ok(folder.join(program::FILE_NAME))
}

/// Opens the main window, hidden until the dashboard has drawn in the system's theme, so that it
/// never shows the wrong colours first.
///
/// # Errors
///
/// Why the window cannot open, such as a configuration that describes no main window.
pub fn window(app: &AppHandle) -> Result<WebviewWindow, Box<dyn Error>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN)
        .ok_or("the configuration describes no main window")?;
    Ok(WebviewWindowBuilder::from_config(app, config)?
        .on_navigation(own_page)
        .build()?)
}

/// Shows the window if the dashboard has not drawn in it in time. A window hidden after the
/// dashboard drew stays hidden, as when it closes to the notification area or an update about to
/// install hides it.
fn show_anyway(app: AppHandle, window: WebviewWindow) {
    thread::spawn(move || {
        thread::sleep(SHOW_ANYWAY_AFTER);
        let drawn = app.try_state::<Life>().is_some_and(|life| life.drawn());
        if !drawn && !window.is_visible().unwrap_or(true) {
            log::warn!("The dashboard did not draw in time; showing the window anyway.");
            let _ = window.show();
        }
    });
}

/// Whether a navigation stays on the app's own page, which is all the window ever shows.
fn own_page(url: &Url) -> bool {
    match url.scheme() {
        "tauri" => true,
        "http" | "https" => match url.host_str() {
            Some("tauri.localhost") => true,
            // The web app's development server, while the app runs with `tauri dev`.
            Some("localhost") => cfg!(dev),
            _ => false,
        },
        _ => false,
    }
}

/// Records where the app is, for `claude-dashboard app`. A history that this version cannot
/// change is left as it is.
fn record_location(app: &App) {
    let Some(location) = location() else {
        return;
    };
    let location = location.display().to_string();
    let Ok(mut history) = app.open_history() else {
        return;
    };
    let recorded = history
        .settings()
        .ok()
        .and_then(|settings| settings.get(DESKTOP_APP_SETTING).cloned());
    if recorded.as_deref() == Some(location.as_str()) {
        return;
    }
    if let Err(error) = history.write(|w| w.set_setting(DESKTOP_APP_SETTING, Some(&location))) {
        log::info!("Cannot record where the desktop app is: {error}");
    }
}

/// Where the app is, as a person would open it: the `AppImage` on Linux, the app bundle on macOS,
/// and otherwise its program.
fn location() -> Option<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Some(PathBuf::from(appimage));
    }
    let this = std::env::current_exe().ok()?;
    if cfg!(target_os = "macos")
        && let Some(bundle) = this.ancestors().find(|folder| is_bundle(folder))
    {
        return Some(bundle.to_path_buf());
    }
    Some(this)
}

fn is_bundle(folder: &Path) -> bool {
    folder
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_stays_on_the_apps_own_page() {
        for own in ["tauri://localhost/", "http://tauri.localhost/index.html"] {
            assert!(own_page(&Url::parse(own).unwrap()), "{own}");
        }
        for elsewhere in [
            "https://example.com/",
            "http://tauri.localhost.example.com/",
            "file:///C:/Windows/",
        ] {
            assert!(!own_page(&Url::parse(elsewhere).unwrap()), "{elsewhere}");
        }
    }
}
