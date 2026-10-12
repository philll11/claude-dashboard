//! Opening the app at login, hidden in its item: a value in the Run key on Windows, a launch agent
//! on macOS and an autostart entry on Linux, as `auto-launch` makes them.

use auto_launch::{AutoLaunch, AutoLaunchBuilder};
use claude_dashboard_server::Fault;
use tauri::AppHandle;

use crate::lifecycle::HIDDEN;

/// Whether the app opens at login, as the operating system has it.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command the app by value"
)]
pub fn autostart(app: AppHandle) -> Result<bool, Fault> {
    item(&app)
        .and_then(|item| item.is_enabled().map_err(|error| error.to_string()))
        .map_err(|error| Fault {
            code: Fault::INTERNAL_ERROR,
            message: format!("Cannot tell whether the app opens at login: {error}"),
        })
}

/// Makes the app open at login, in its item without the window, or stop doing so.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command the app by value"
)]
pub fn set_autostart(app: AppHandle, on: bool) -> Result<(), Fault> {
    item(&app)
        .and_then(|item| {
            if on { item.enable() } else { item.disable() }.map_err(|error| error.to_string())
        })
        .map_err(|error| Fault {
            code: Fault::INTERNAL_ERROR,
            message: format!("Cannot change whether the app opens at login: {error}"),
        })
}

/// The login item of this copy of the app.
fn item(app: &AppHandle) -> Result<AutoLaunch, String> {
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut builder = AutoLaunchBuilder::new();
    // The Windows uninstaller removes the Run key's value that has the product's name.
    builder
        .set_app_name(&app.package_info().name)
        .set_args(&[HIDDEN]);
    #[cfg(windows)]
    builder
        .set_app_path(&command_line_word(&program.display().to_string()))
        .set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser);
    #[cfg(target_os = "macos")]
    builder
        .set_app_path(
            &program
                .canonicalize()
                .map_err(|error| error.to_string())?
                .display()
                .to_string(),
        )
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent);
    // An AppImage opens through the image, not through the program it unpacks for this run.
    #[cfg(target_os = "linux")]
    builder
        .set_app_path(&exec_word(
            &tauri::Manager::env(app)
                .appimage
                .map_or(program, std::path::PathBuf::from)
                .display()
                .to_string(),
        ))
        .set_linux_launch_mode(auto_launch::LinuxLaunchMode::XdgAutostart);
    builder.build().map_err(|error| error.to_string())
}

/// A program's path as the first word of a Windows command line, such as a Run key's value:
/// quoted, since Windows reads an unquoted path as ending at its first space. No Windows file name
/// holds a quote.
#[cfg_attr(not(any(windows, test)), expect(dead_code))]
fn command_line_word(path: &str) -> String {
    format!("\"{path}\"")
}

/// A program's path as the first word of a desktop entry's `Exec` key, quoted where the Desktop
/// Entry Specification requires it, then escaped as a string value, then with `%` doubled.
#[cfg_attr(not(any(target_os = "linux", test)), expect(dead_code))]
fn exec_word(path: &str) -> String {
    const RESERVED: &[char] = &[
        ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(',
        ')', '`',
    ];
    let word = if path.contains(RESERVED) {
        let mut quoted = String::from('"');
        for character in path.chars() {
            if matches!(character, '"' | '`' | '$' | '\\') {
                quoted.push('\\');
            }
            quoted.push(character);
        }
        quoted.push('"');
        quoted
    } else {
        path.to_owned()
    };
    let mut value = String::with_capacity(word.len());
    for character in word.chars() {
        match character {
            '\\' => value.push_str(r"\\"),
            '\n' => value.push_str(r"\n"),
            '\t' => value.push_str(r"\t"),
            '\r' => value.push_str(r"\r"),
            '%' => value.push_str("%%"),
            other => value.push(other),
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::{command_line_word, exec_word};

    #[test]
    fn a_run_key_value_quotes_the_path_so_its_arguments_reach_the_app() {
        assert_eq!(
            command_line_word(
                r"C:\Users\you\AppData\Local\Dashboard for Claude Code\claude-dashboard-desktop.exe"
            ),
            r#""C:\Users\you\AppData\Local\Dashboard for Claude Code\claude-dashboard-desktop.exe""#
        );
    }

    #[test]
    fn a_plain_path_is_its_own_exec_word() {
        assert_eq!(
            exec_word("/usr/bin/claude-dashboard-desktop"),
            "/usr/bin/claude-dashboard-desktop"
        );
    }

    #[test]
    fn a_path_with_a_space_is_quoted_in_the_exec_key() {
        assert_eq!(
            exec_word("/home/you/Apps/Dashboard for Claude Code.AppImage"),
            r#""/home/you/Apps/Dashboard for Claude Code.AppImage""#
        );
    }

    #[test]
    fn characters_with_a_meaning_in_the_exec_key_are_escaped() {
        assert_eq!(
            exec_word(r#"/home/you/"$Apps"\`x`/100%/app"#),
            r#""/home/you/\\"\\$Apps\\"\\\\\\`x\\`/100%%/app""#
        );
        assert_eq!(exec_word("/opt/100%/app"), "/opt/100%%/app");
    }
}
