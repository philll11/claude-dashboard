//! The app outlives its window (ADR 0010). While its item is in the notification area or the menu
//! bar and the person keeps the app running, closing the window hides it, and the item, a second
//! start or the Dock brings it back; Quit in the item's menu ends the app. The first time the
//! window closes on Windows, the dashboard asks which the person wants. A start at login stays
//! hidden in the item until the person opens it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use claude_dashboard_server::Fault;
use claude_dashboard_server::app::{self, App, KEEP_RUNNING_SETTING};
use serde::Serialize;
use tauri::{AppHandle, CloseRequestApi, Emitter, Manager, State};
use tauri_plugin_window_state::{AppHandleExt as _, StateFlags};

use crate::bridge::{MAIN, Show};
use crate::tray::{Nudge, Tray};

/// The argument a start at login passes, which keeps the window hidden.
pub const HIDDEN: &str = "--hidden";

/// What the app's life depends on.
pub struct Life {
    base: App,
    /// The item, where the desktop has a notification area.
    pub tray: Option<Tray>,
    /// Whether this start keeps the window hidden until the person opens it.
    hidden: AtomicBool,
    /// Whether the dashboard has drawn, and so can ask the person a question.
    drawn: AtomicBool,
}

impl Life {
    pub fn new(base: App, tray: Option<Tray>, hidden: bool) -> Self {
        Self {
            base,
            tray,
            hidden: AtomicBool::new(hidden),
            drawn: AtomicBool::new(false),
        }
    }

    fn settings(&self) -> HashMap<String, String> {
        self.base
            .open_history()
            .ok()
            .and_then(|history| history.settings().ok())
            .unwrap_or_default()
    }

    /// Whether the item is there to come back from.
    fn item_shows(&self, settings: &HashMap<String, String>) -> bool {
        self.tray.is_some() && app::shows_tray(settings)
    }

    /// Whether the dashboard has drawn in the window.
    pub fn drawn(&self) -> bool {
        self.drawn.load(Ordering::Relaxed)
    }

    /// Whether closing the window leaves the app running.
    fn keeps_running(&self) -> bool {
        let settings = self.settings();
        self.item_shows(&settings) && app::keeps_running(&settings)
    }
}

/// Brings the window forward, at `section` when given, making it again if it has gone.
pub fn open(app: &AppHandle, section: Option<&'static str>) {
    if let Some(life) = app.try_state::<Life>() {
        life.hidden.store(false, Ordering::Relaxed);
    }
    let window = match app.get_webview_window(MAIN) {
        Some(window) => window,
        None => match crate::start::window(app) {
            Ok(window) => window,
            Err(error) => {
                log::warn!("Cannot open the window again: {error}");
                return;
            }
        },
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    if let Some(section) = section
        && let Err(error) = app.emit_to(MAIN, "show", Show { section })
    {
        log::warn!("Cannot show the section asked for: {error}");
    }
}

/// Hides the window, keeping its size and place for when it comes back.
pub fn hide(app: &AppHandle) {
    if let Err(error) = app.save_window_state(StateFlags::all() - StateFlags::VISIBLE) {
        log::warn!("Cannot keep the window's size and place: {error}");
    }
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.hide();
    }
}

/// Ends the app: hides the window, removes the item and exits.
pub fn quit(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.hide();
    }
    if let Some(life) = app.try_state::<Life>()
        && let Some(tray) = &life.tray
    {
        tray.stop(app);
    }
    app.exit(0);
}

/// Asks the dashboard to install the update that waits, which restarts the app.
pub fn install_update(app: &AppHandle) {
    open(app, None);
    if let Err(error) = app.emit_to(MAIN, "installUpdate", ()) {
        log::warn!("Cannot start the update: {error}");
    }
}

/// What closing the window does: it hides the window while the app keeps running, asking first
/// the first time on Windows; otherwise the window closes and the app ends.
pub fn close_requested(app: &AppHandle, api: &CloseRequestApi) {
    let Some(life) = app.try_state::<Life>() else {
        return;
    };
    let settings = life.settings();
    if !life.item_shows(&settings) || !app::keeps_running(&settings) {
        return;
    }
    api.prevent_close();
    if cfg!(windows)
        && !settings.contains_key(KEEP_RUNNING_SETTING)
        && life.drawn()
        && app.emit_to(MAIN, "closeAsked", ()).is_ok()
    {
        return;
    }
    hide(app);
}

/// Whether to stop the app from ending when its last window has gone, which it should not while
/// it keeps running in its item.
pub fn keeps_running(app: &AppHandle) -> bool {
    app.try_state::<Life>()
        .is_some_and(|life| life.keeps_running())
}

/// What the dashboard needs to know about the desktop app.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Desktop {
    /// Whether the app has its item.
    tray: bool,
    /// Where the item is, in words.
    tray_place: &'static str,
    /// Whether this start keeps the window hidden.
    hidden: bool,
}

/// The dashboard has drawn: the window shows, unless this start keeps it hidden in the item.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its state by value"
)]
pub fn ready(app: AppHandle, life: State<'_, Life>) {
    life.drawn.store(true, Ordering::Relaxed);
    let hidden = life.hidden.load(Ordering::Relaxed) && life.item_shows(&life.settings());
    if !hidden && let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.show();
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its state by value"
)]
pub fn desktop(life: State<'_, Life>) -> Desktop {
    Desktop {
        tray: life.tray.is_some(),
        tray_place: if cfg!(target_os = "macos") {
            "menu bar"
        } else {
            "notification area"
        },
        hidden: life.hidden.load(Ordering::Relaxed),
    }
}

/// The person's answer the first time the window closed: keep the app running in its item, or
/// quit, now and whenever the window closes.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its state by value"
)]
pub fn close_choice(
    app: AppHandle,
    life: State<'_, Life>,
    keep_running: bool,
) -> Result<(), Fault> {
    let mut history = life.base.open_history().map_err(|message| Fault {
        code: Fault::INTERNAL_ERROR,
        message,
    })?;
    history
        .write(|w| {
            w.set_setting(
                KEEP_RUNNING_SETTING,
                Some(if keep_running { "on" } else { "off" }),
            )
        })
        .map_err(Fault::from)?;
    if keep_running {
        hide(&app);
    } else {
        quit(&app);
    }
    Ok(())
}

/// An update is downloaded and waits for a restart, which the item's menu offers; `None` once it
/// no longer waits.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its state by value"
)]
pub fn update_waiting(life: State<'_, Life>, version: Option<String>) {
    if let Some(tray) = &life.tray {
        tray.nudge(Nudge::Update(version));
    }
}
