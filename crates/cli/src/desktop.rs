//! `claude-dashboard app`: opens the desktop app where it last ran, as the history records it.

use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use claude_dashboard_core::history::History;
use claude_dashboard_server::app::{App, DESKTOP_APP_SETTING};

/// Where the desktop app's installers are.
const RELEASES: &str = "https://github.com/philll11/claude-dashboard/releases/latest";

pub fn open(app: &App, section: Option<&str>) -> ExitCode {
    let Some(location) = recorded(app).filter(|location| location.exists()) else {
        eprintln!(
            "The desktop app has not run on this computer yet. Install it from {RELEASES} and open it once."
        );
        return ExitCode::FAILURE;
    };
    let mut command = if cfg!(target_os = "macos")
        && location
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    {
        let mut open = Command::new("open");
        open.arg("-a").arg(&location);
        if let Some(section) = section {
            open.args(["--args", "--section", section]);
        }
        open
    } else {
        let mut start = Command::new(&location);
        if let Some(section) = section {
            start.args(["--section", section]);
        }
        start
    };
    match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "Cannot open the desktop app at {}: {error}",
                location.display()
            );
            ExitCode::FAILURE
        }
    }
}

/// Where the desktop app last ran, which it records in the history each time it starts.
fn recorded(app: &App) -> Option<PathBuf> {
    let path = app.history_path.as_ref().filter(|path| path.is_file())?;
    let history = History::open(path).ok()?;
    let settings = history.settings().ok()?;
    settings.get(DESKTOP_APP_SETTING).map(PathBuf::from)
}
