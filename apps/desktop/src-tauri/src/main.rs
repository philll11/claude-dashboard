//! The desktop app: the dashboard in a window of its own, answered by the dashboard server running
//! in this process (ADR 0008).

// A release build is a Windows app, which opens no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod start;

use tauri::RunEvent;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};
use tauri_plugin_window_state::StateFlags;

/// The most a log file holds before it starts again.
const LOG_SIZE: u128 = 1_000_000;

fn main() {
    let app = tauri::Builder::default()
        // The single instance plugin comes first, so that a second start hands over its section
        // and ends before anything else runs.
        .plugin(tauri_plugin_single_instance::init(bridge::second_start))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                // The window shows itself once the dashboard has drawn (`start::window`).
                .with_state_flags(StateFlags::all() - StateFlags::VISIBLE)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .target(Target::new(TargetKind::LogDir { file_name: None }))
                .target(Target::new(TargetKind::Stderr))
                .level(log::LevelFilter::Info)
                // In the person's own time, with its offset from UTC.
                .format(|out, message, record| {
                    out.finish(format_args!(
                        "{} {} {}: {message}",
                        jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S %:z"),
                        record.level(),
                        record.target(),
                    ));
                })
                .max_file_size(LOG_SIZE)
                .rotation_strategy(RotationStrategy::KeepOne)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![bridge::request, bridge::take_show])
        .setup(start::app)
        .build(tauri::generate_context!())
        .expect("the desktop app's Tauri context is valid");
    app.run(|app, event| {
        if let RunEvent::Exit = event {
            bridge::stop(app);
        }
    });
}
