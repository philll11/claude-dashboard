//! `claude-dashboard statusline`: records the plan limits that Claude Code passes to a status
//! line command, and prints a compact line, or the person's own status line when it wraps one; and
//! `statusline connect` and `disconnect`, which show the change to Claude Code's settings first.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use claude_dashboard_core::ingest;
use claude_dashboard_core::limits::{FIVE_HOUR, Level, SEVEN_DAY, StatusLineInput};
use claude_dashboard_core::report::{self, Range};
use claude_dashboard_core::transcript;
use claude_dashboard_server::app::App;
use claude_dashboard_server::fmt::money;
use claude_dashboard_server::names::model_name;
use claude_dashboard_server::statusline::{
    Plan, members, plan_connect, plan_disconnect, read_settings, settings_path, wrapped_command,
};
use serde_json::{Map, Value};

use crate::prompt;
use crate::style::Paint;
use crate::tui::{clean, fit};

/// The most status line JSON that is read.
const MAX_INPUT: u64 = 1 << 20;

/// The most cells a session's title takes, so that the plan windows and the cost after it stay in
/// sight in a narrow terminal.
const TITLE_CELLS: i32 = 40;

pub fn run(app: &App, then: &[OsString]) -> ExitCode {
    let mut json = String::new();
    let _ = std::io::stdin().take(MAX_INPUT).read_to_string(&mut json);
    let input = StatusLineInput::parse(&json).unwrap_or_default();
    let today = record(app, &input);
    let line = if then.is_empty() {
        our_line(app, &input, today)
    } else {
        own_line(app, then, &json)
    };
    print!("{line}");
    let _ = std::io::stdout().flush();
    ExitCode::SUCCESS
}

/// Records the plan limits and the session's new requests, and returns today's API-equivalent
/// cost. A status line never fails, so whatever cannot be done is left out.
fn record(app: &App, input: &StatusLineInput) -> Option<f64> {
    let mut history = app.open_history().ok()?;
    let snapshots = input.snapshots(app.now.as_millisecond());
    if !snapshots.is_empty() {
        let _ = history.write(|w| {
            for snapshot in &snapshots {
                w.limit_snapshot(snapshot)?;
            }
            Ok(())
        });
    }
    if let Some(path) = &input.transcript_path {
        let transcripts = transcript::for_session(Path::new(path));
        let _ = ingest::ingest_transcripts(&mut history, &transcripts);
    }
    let day = app.now_zoned().date();
    let models = report::by_model(&history, &app.prices, Range::days(day, day, &app.zone)).ok()?;
    Some(models.iter().map(|model| model.totals.cost.total()).sum())
}

/// claude-dashboard's own status line:
/// `Fix the importer · Opus 5.5 · 5h 42% · week 71% ▲ · $5.05 today`.
fn our_line(app: &App, input: &StatusLineInput, today_usd: Option<f64>) -> String {
    let paint = Paint::for_claude_code();
    let mut parts = Vec::new();
    // The title tells apart the sessions that run side by side, each with its own status line.
    if let Some(title) = &input.title {
        parts.push(paint.bold(&fit(&clean(title), TITLE_CELLS)));
    }
    if let Some(id) = &input.model_id {
        parts.push(model_name(app, id));
    } else if let Some(name) = &input.model_name {
        parts.push(name.clone());
    }
    for (window, used, _) in &input.windows {
        let label = match window.as_str() {
            FIVE_HOUR => "5h",
            SEVEN_DAY => "week",
            "spend_limit" => "spend",
            _ => continue,
        };
        let percent = format!("{used:.0}%");
        let figure = match Level::of(*used) {
            Level::Normal => paint.strong(&percent),
            Level::Warning => paint.paint(&format!("{percent} ▲"), paint.palette().warning, true),
            // Red passes WCAG AA only for marks, so the figure stays in strong ink.
            Level::Critical => format!(
                "{} {}",
                paint.strong(&percent),
                paint.paint("▲", paint.palette().critical, true)
            ),
        };
        parts.push(format!("{label} {figure}"));
    }
    if let Some(usd) = today_usd {
        parts.push(format!(
            "{} today",
            paint.paint(&money(usd), paint.palette().strong, false)
        ));
    }
    parts.join(&format!(" {} ", paint.muted("·")))
}

/// The person's own status line, run as Claude Code would run it, with the same input.
fn own_line(app: &App, then: &[OsString], json: &str) -> String {
    let command = settings_path(app)
        .and_then(|path| read_settings(&path).ok())
        .and_then(|settings| wrapped_command(&settings))
        .unwrap_or_else(|| {
            then.iter()
                .map(|part| part.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ")
        });
    let Ok(mut child) = shell(&command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return String::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(json.as_bytes());
    }
    child
        .wait_with_output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

/// The shell that Claude Code runs status line commands in: `sh` on macOS and Linux, and on
/// Windows Git Bash when it is installed, or else PowerShell.
fn shell(command: &str) -> Command {
    if cfg!(windows) {
        if let Some(bash) = git_bash() {
            let mut shell = Command::new(bash);
            shell.args(["-c", command]);
            return shell;
        }
        let mut shell = Command::new("powershell");
        shell.args(["-NoProfile", "-Command", command]);
        shell
    } else {
        let mut shell = Command::new("sh");
        shell.args(["-c", command]);
        shell
    }
}

/// Git Bash, where Claude Code finds it: the path it is told, next to the Git on the `PATH`, or
/// where Git installs by default. A `bash` on the `PATH` is not taken, as on Windows it can be
/// WSL's.
fn git_bash() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|folder| folder.join("git.exe"))
        .filter(|git| git.is_file())
        .find_map(|git| {
            let bash = git.parent()?.parent()?.join("bin").join("bash.exe");
            bash.is_file().then_some(bash)
        })
        .or_else(|| {
            let bash = PathBuf::from(r"C:\Program Files\Git\bin\bash.exe");
            bash.is_file().then_some(bash)
        })
}

/// The change to the `statusLine` setting, as the design shows it: the old and new fields on
/// their own lines, in the removed and added colours.
fn describe_change(
    paint: Paint,
    path: &Path,
    old: Option<&Map<String, Value>>,
    new: Option<&Map<String, Value>>,
) -> String {
    let removed = |text: String| paint.paint(&text, paint.palette().removed, false);
    let added = |text: String| paint.paint(&text, paint.palette().added, false);
    let path = path.display();
    match (old, new) {
        (Some(old), Some(new)) => format!(
            "This changes one setting in {path}:\n\n  \"statusLine\": {{\n{}\n{}\n  }}\n",
            removed(format!("-   {}", members(old))),
            added(format!("+   {}", members(new)))
        ),
        (None, Some(new)) => format!(
            "{}\n\n{}\n",
            if Path::new(&path.to_string()).is_file() {
                format!("This adds one setting to {path}:")
            } else {
                format!("This creates {path} with one setting:")
            },
            added(format!("+ \"statusLine\": {{ {} }}", members(new)))
        ),
        (Some(old), None) => format!(
            "This removes one setting from {path}:\n\n{}\n",
            removed(format!("- \"statusLine\": {{ {} }}", members(old)))
        ),
        (None, None) => String::new(),
    }
}

/// Prints a plan's change and asks before making it; `undo` names how to take it back.
fn confirm_and_make(plan: Plan, yes: bool, undo: Option<&str>, done: &str) -> ExitCode {
    let change = match plan {
        Plan::Change(change) => change,
        Plan::Nothing(reason) => {
            println!("{reason}");
            return ExitCode::SUCCESS;
        }
        Plan::Refused(reason) => {
            eprintln!("{reason}");
            return ExitCode::FAILURE;
        }
    };
    let paint = Paint::for_stdout();
    let mut out = describe_change(
        paint,
        &change.path,
        change.old.as_ref(),
        change.new.as_ref(),
    );
    out.push('\n');
    out.push_str(change.effect);
    out.push('\n');
    match (undo, change.backed_up()) {
        (Some(undo), true) => {
            let _ = writeln!(
                out,
                "A backup is saved next to settings.json. Undo it with {}.",
                paint.strong(undo)
            );
        }
        (Some(undo), false) => {
            let _ = writeln!(out, "Undo it with {}.", paint.strong(undo));
        }
        (None, _) => out.push_str("A backup is saved next to settings.json.\n"),
    }
    print!("{out}");
    if !prompt::confirmed(paint, yes, "Make this change?") {
        return ExitCode::FAILURE;
    }
    match change.make() {
        Ok(()) => {
            println!("{done}");
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!("Cannot save the change: {reason}.");
            ExitCode::FAILURE
        }
    }
}

pub fn connect(app: &App, yes: bool) -> ExitCode {
    confirm_and_make(
        plan_connect(app),
        yes,
        Some("claude-dashboard statusline disconnect"),
        "Connected. Claude Code runs it after its next response.",
    )
}

pub fn disconnect(app: &App, yes: bool) -> ExitCode {
    confirm_and_make(plan_disconnect(app), yes, None, "Disconnected.")
}
