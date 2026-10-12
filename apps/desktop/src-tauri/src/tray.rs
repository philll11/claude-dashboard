//! The app's item in the notification area, or the menu bar on macOS: the five-hour window's use
//! at a glance, and every plan window, today's cost and where the readings come from in its menu.
//!
//! Tray calls run on the main thread, so the item has a thread of its own, which the server's
//! notifications only nudge. It waits for a burst of them to end, reads the glance through a
//! connection of its own to the history, and applies what changed in one call on the main thread.
//! It never waits on the main thread, so the app can stop it from there at any time.

use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use claude_dashboard_core::history::History;
use claude_dashboard_server::app::{self, App};
use claude_dashboard_server::glance::{self, Glance, NAME};
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};

use crate::icon::{self, Face};
use crate::lifecycle;

/// The item's id.
const ID: &str = "glance";

/// How long the item waits for a burst of changes to end before it reads them.
const SETTLE: Duration = Duration::from_secs(1);

/// How often the item reads again without being nudged: resets, the readings' age and the day
/// move on by themselves.
const EVERY: Duration = Duration::from_secs(60);

/// The menu bar symbol's size on macOS: 18 points, twice over for a Retina display.
#[cfg(target_os = "macos")]
const TEMPLATE_SIZE: u32 = 36;

/// What nudges the item's thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nudge {
    /// The history or the settings changed.
    Changed,
    /// An update waits to be installed, or no longer does.
    Update(Option<String>),
    Stop,
}

/// The item and its thread.
pub struct Tray {
    nudges: Sender<Nudge>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

/// Everything the item shows, to tell when something changed.
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    visible: bool,
    glance: Option<Glance>,
    update: Option<String>,
}

/// The icon the main thread last drew, at its size in pixels.
type Drawn = Arc<Mutex<Option<(Face, u32)>>>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Tray {
    /// Adds the item and starts its thread, which `nudges` reach. Returns `None` where the
    /// desktop has no notification area: on Linux, the tray library loads an `AppIndicator`
    /// library when the item is made, and panics without one.
    pub fn start(
        app: &AppHandle,
        base: App,
        nudges: (Sender<Nudge>, Receiver<Nudge>),
    ) -> Option<Self> {
        match panic::catch_unwind(AssertUnwindSafe(|| build(app))) {
            Ok(Ok(tray)) => {
                // An item the person has hidden never shows, not even while the app starts.
                let visible = base
                    .open_history()
                    .ok()
                    .and_then(|history| history.settings().ok())
                    .is_none_or(|settings| app::shows_tray(&settings));
                if !visible {
                    let _ = tray.set_visible(false);
                }
                log::info!("Added the item to the notification area.");
            }
            Ok(Err(error)) => {
                log::warn!("Cannot add the item to the notification area: {error}");
                return None;
            }
            Err(_) => {
                log::warn!(
                    "This desktop has no notification area for the item, so the app runs without it."
                );
                return None;
            }
        }
        let (sender, receiver) = nudges;
        let app = app.clone();
        let thread = thread::Builder::new()
            .name("tray".to_owned())
            .spawn(move || run(&app, &base, &receiver))
            .ok()?;
        Some(Self {
            nudges: sender,
            thread: Mutex::new(Some(thread)),
        })
    }

    pub fn nudge(&self, nudge: Nudge) {
        let _ = self.nudges.send(nudge);
    }

    /// Stops the thread and removes the item.
    pub fn stop(&self, app: &AppHandle) {
        let _ = self.nudges.send(Nudge::Stop);
        if let Some(thread) = lock(&self.thread).take() {
            let _ = thread.join();
        }
        app.remove_tray_by_id(ID);
    }
}

fn build(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let builder = TrayIconBuilder::with_id(ID)
        .tooltip(NAME)
        .menu(&menu(
            app,
            &Shown {
                visible: true,
                glance: None,
                update: None,
            },
        )?)
        // On Windows a left click opens the dashboard, as Microsoft asks of an item, and a right
        // click opens the menu; elsewhere a click opens the menu.
        .show_menu_on_left_click(!cfg!(windows))
        .on_menu_event(|app, event| chosen(app, &event))
        .on_tray_icon_event(|tray, event| clicked(tray, &event));
    #[cfg(target_os = "macos")]
    let builder = {
        let template = icon::template(TEMPLATE_SIZE);
        builder
            .icon(Image::new_owned(
                template.pixels,
                TEMPLATE_SIZE,
                TEMPLATE_SIZE,
            ))
            .icon_as_template(true)
    };
    #[cfg(not(target_os = "macos"))]
    let builder = {
        let size = icon_size(app);
        let gauge = icon::draw(Face::Gauge, size);
        builder.icon(Image::new_owned(gauge.pixels, size, size))
    };
    builder.build(app)
}

fn clicked(tray: &TrayIcon, event: &TrayIconEvent) {
    if cfg!(windows)
        && let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
    {
        lifecycle::open(tray.app_handle(), None);
    }
}

fn chosen(app: &AppHandle, event: &MenuEvent) {
    match event.id().as_ref() {
        "open" => lifecycle::open(app, None),
        "limits" => lifecycle::open(app, Some("limits")),
        "today" => lifecycle::open(app, Some("overview")),
        "update" => lifecycle::install_update(app),
        "quit" => lifecycle::quit(app),
        _ => {}
    }
}

/// The item's menu: a line for each plan window and today's cost, where the readings come from,
/// then opening the dashboard, a waiting update and quitting.
fn menu(app: &AppHandle, shown: &Shown) -> tauri::Result<Menu<Wry>> {
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    if let Some(glance) = &shown.glance {
        for line in &glance.lines {
            let id = if line.label == "Today" {
                "today"
            } else {
                "limits"
            };
            items.push(Box::new(MenuItem::with_id(
                app,
                id,
                line.text(),
                true,
                None::<&str>,
            )?));
        }
        if let Some(source) = &glance.source {
            items.push(Box::new(PredefinedMenuItem::separator(app)?));
            items.push(Box::new(MenuItem::with_id(
                app,
                "source",
                source,
                false,
                None::<&str>,
            )?));
        }
        items.push(Box::new(PredefinedMenuItem::separator(app)?));
    }
    items.push(Box::new(MenuItem::with_id(
        app,
        "open",
        "Open dashboard",
        true,
        None::<&str>,
    )?));
    if let Some(version) = &shown.update {
        items.push(Box::new(MenuItem::with_id(
            app,
            "update",
            format!("Restart to update to {version}"),
            true,
            None::<&str>,
        )?));
    }
    items.push(Box::new(MenuItem::with_id(
        app,
        "quit",
        format!("Quit {NAME}"),
        true,
        None::<&str>,
    )?));
    let items: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(AsRef::as_ref).collect();
    Menu::with_items(app, &items)
}

/// The icon's size in pixels: 16 at 100% scaling, as the primary display scales it, up to 48.
#[cfg(not(target_os = "macos"))]
fn icon_size(app: &AppHandle) -> u32 {
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |monitor| monitor.scale_factor());
    pixels(scale)
}

#[cfg(not(target_os = "macos"))]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the size is rounded and kept between 16 and 48 first"
)]
fn pixels(scale: f64) -> u32 {
    (16.0 * scale).round().clamp(16.0, 48.0) as u32
}

/// What the icon shows for a glance.
fn face(glance: Option<&Glance>) -> Face {
    match glance.and_then(|glance| glance.five_hour) {
        Some(five_hour) => Face::Number {
            used: five_hour.used,
            level: five_hour.level,
        },
        None => Face::Gauge,
    }
}

/// Reads the glance whenever it is nudged, after a burst of nudges ends, and at least every
/// [`EVERY`], until it is told to stop.
fn run(app: &AppHandle, base: &App, nudges: &Receiver<Nudge>) {
    let drawn: Drawn = Arc::new(Mutex::new(None));
    let mut history: Option<History> = None;
    let mut shown: Option<Shown> = None;
    let mut update: Option<String> = None;
    loop {
        let wanted = read(base, &mut history, update.clone());
        if shown.as_ref() == Some(&wanted) {
            // The display's scale may have changed, which needs the icon drawn again.
            redraw(app, face(wanted.glance.as_ref()), &drawn);
        } else {
            apply(app, &wanted, &drawn);
            shown = Some(wanted);
        }
        match nudges.recv_timeout(EVERY) {
            Ok(Nudge::Changed) => {
                let until = Instant::now() + SETTLE;
                loop {
                    match nudges.recv_timeout(until.saturating_duration_since(Instant::now())) {
                        Ok(Nudge::Changed) => {}
                        Ok(Nudge::Update(version)) => update = version,
                        Ok(Nudge::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                        Err(RecvTimeoutError::Timeout) => break,
                    }
                }
            }
            Ok(Nudge::Update(version)) => update = version,
            Ok(Nudge::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// What the item should show now. Without a history to read, it shows only its own menu.
fn read(base: &App, history: &mut Option<History>, update: Option<String>) -> Shown {
    if history.is_none() {
        *history = base.open_history().ok();
    }
    let Some(history) = history.as_ref() else {
        return Shown {
            visible: true,
            glance: None,
            update,
        };
    };
    let app = base.with_shared(history).at(app::now());
    let settings = history.settings().unwrap_or_default();
    Shown {
        visible: app::shows_tray(&settings),
        glance: glance::read(&app, history).ok(),
        update,
    }
}

/// Applies `shown` in one call on the main thread.
fn apply(app: &AppHandle, shown: &Shown, drawn: &Drawn) {
    let (handle, shown, drawn) = (app.clone(), shown.clone(), Arc::clone(drawn));
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(ID) else {
            return;
        };
        let _ = tray.set_visible(shown.visible);
        draw(&handle, &tray, face(shown.glance.as_ref()), &drawn);
        let tooltip = shown
            .glance
            .as_ref()
            .map_or(NAME, |glance| glance.tooltip.as_str());
        let _ = tray.set_tooltip(Some(tooltip));
        #[cfg(target_os = "macos")]
        let _ = tray.set_title(shown.glance.as_ref().map(|glance| glance.title.as_str()));
        match menu(&handle, &shown) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
            }
            Err(error) => log::warn!("Cannot update the item's menu: {error}"),
        }
    });
}

/// Draws the icon again on the main thread, if the display's scale changed since it was drawn.
fn redraw(app: &AppHandle, face: Face, drawn: &Drawn) {
    let (handle, drawn) = (app.clone(), Arc::clone(drawn));
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = handle.tray_by_id(ID) {
            draw(&handle, &tray, face, &drawn);
        }
    });
}

/// Draws `face` at the display's scale, unless it is drawn so already. On macOS the icon is the
/// template image the item was made with, and the title carries the figure.
#[cfg_attr(
    target_os = "macos",
    expect(unused_variables, reason = "the icon never changes")
)]
fn draw(app: &AppHandle, tray: &TrayIcon, face: Face, drawn: &Mutex<Option<(Face, u32)>>) {
    #[cfg(not(target_os = "macos"))]
    {
        let size = icon_size(app);
        let mut drawn = lock(drawn);
        if *drawn == Some((face, size)) {
            return;
        }
        let image = icon::draw(face, size);
        match tray.set_icon(Some(Image::new_owned(image.pixels, size, size))) {
            Ok(()) => *drawn = Some((face, size)),
            Err(error) => log::warn!("Cannot draw the item's icon: {error}"),
        }
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::pixels;

    #[test]
    fn the_icon_is_16_pixels_at_100_percent_and_scales_with_the_display() {
        assert_eq!(pixels(1.0), 16);
        assert_eq!(pixels(1.25), 20);
        assert_eq!(pixels(1.5), 24);
        assert_eq!(pixels(2.0), 32);
        assert_eq!(pixels(2.5), 40);
        assert_eq!(pixels(3.0), 48);
        assert_eq!(pixels(4.0), 48);
        assert_eq!(pixels(0.5), 16);
    }
}
