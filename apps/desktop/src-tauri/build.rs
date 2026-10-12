// Tauri allows every command the app registers to every window unless the app lists them, so it
// lists them; capabilities/main.json grants them to the main window alone.
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "request",
            "take_show",
            "ready",
            "desktop",
            "close_choice",
            "update_waiting",
            "autostart",
            "set_autostart",
            "export",
            "export_cancel",
        ]),
    ))
    .expect("the desktop app's Tauri configuration is valid");
}
