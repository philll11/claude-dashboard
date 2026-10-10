//! `claude-dashboard statusline`: records the plan limits that Claude Code passes to a status
//! line command, and prints a compact line, or the person's own status line when it wraps one.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use claude_dashboard_core::ingest;
use claude_dashboard_core::limits::{FIVE_HOUR, Level, SEVEN_DAY, StatusLineInput};
use claude_dashboard_core::report::{self, Range};
use claude_dashboard_core::transcript;
use serde_json::{Map, Value};

use crate::app::App;
use crate::fmt::money;
use crate::report::model_name;
use crate::style::{Paint, palette};

/// The most status line JSON that is read.
const MAX_INPUT: u64 = 1 << 20;
/// What follows claude-dashboard's own status line command when it wraps the person's.
const THEN: &str = " --then ";
/// Characters that would split a wrapped command into several, or redirect it, in the shell
/// that Claude Code runs status lines in.
const STRUCTURAL: [char; 10] = ['|', ';', '&', '<', '>', '(', ')', '`', '\n', '\r'];
/// The backup kept next to `settings.json` before a change.
const BACKUP: &str = "settings.claude-dashboard-backup.json";

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
    let models = report::by_model(&history, app.prices, Range::days(day, day, &app.zone)).ok()?;
    Some(models.iter().map(|model| model.totals.cost.total()).sum())
}

/// Whether a command uses shell operators outside quotes, so that it would not stay one command
/// once claude-dashboard's command is written in front of it.
fn has_shell_operators(command: &str) -> bool {
    let mut chars = command.chars();
    let mut quote = None;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None | Some('"'), '\\') => {
                chars.next();
            }
            (None, '\'' | '"') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, _) if STRUCTURAL.contains(&c) => return true,
            _ => {}
        }
    }
    false
}

/// claude-dashboard's own status line: `Opus 5.5 · 5h 42% · week 71% ▲ · $5.05 today`.
fn our_line(app: &App, input: &StatusLineInput, today_usd: Option<f64>) -> String {
    let paint = Paint::for_claude_code();
    let mut parts = Vec::new();
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
            Level::Warning => paint.paint(&format!("{percent} ▲"), palette::WARNING, true),
            // Red passes WCAG AA only for marks, so the figure stays in strong ink.
            Level::Critical => format!(
                "{} {}",
                paint.strong(&percent),
                paint.paint("▲", palette::CRITICAL, true)
            ),
        };
        parts.push(format!("{label} {figure}"));
    }
    if let Some(usd) = today_usd {
        parts.push(format!(
            "{} today",
            paint.paint(&money(usd), palette::STRONG, false)
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

/// Git Bash, where Claude Code would find it: the path it is told, `bash` on the `PATH`, or next
/// to Git.
fn git_bash() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    let on_path = |name: &str| {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|folder| folder.join(name))
            .find(|path| path.is_file())
    };
    on_path("bash.exe")
        .or_else(|| {
            let git = on_path("git.exe")?;
            let bash = git.parent()?.parent()?.join("bin").join("bash.exe");
            bash.is_file().then_some(bash)
        })
        .or_else(|| {
            let bash = PathBuf::from(r"C:\Program Files\Git\bin\bash.exe");
            bash.is_file().then_some(bash)
        })
}

/// Whether claude-dashboard is Claude Code's status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    /// It is; `wrapping` is the person's own status line, run after it.
    Connected { wrapping: Option<String> },
    /// It is not; `current` is the status line command Claude Code runs instead.
    NotConnected { current: Option<String> },
    /// The settings cannot tell.
    Unknown(String),
}

/// Claude Code's user settings, in the first data root.
fn settings_path(app: &App) -> Option<PathBuf> {
    app.roots
        .first()
        .map(|root| root.path.join("settings.json"))
}

fn read_settings(path: &Path) -> Result<Map<String, Value>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(settings)) => Ok(settings),
        Ok(_) => Err(format!("{} does not hold a JSON object", path.display())),
        Err(error) => Err(format!("{} is not valid JSON: {error}", path.display())),
    }
}

fn status_line_command(settings: &Map<String, Value>) -> Result<Option<&str>, String> {
    let Some(status_line) = settings.get("statusLine") else {
        return Ok(None);
    };
    match (
        status_line.get("type").and_then(Value::as_str),
        status_line.get("command").and_then(Value::as_str),
    ) {
        (Some("command"), Some(command)) => Ok(Some(command)),
        _ => Err("the status line in settings.json is not a command".to_owned()),
    }
}

/// Whether a status line command is claude-dashboard's: its program is `claude-dashboard`,
/// however it is reached, and its first argument is `statusline`.
fn is_ours(command: &str) -> bool {
    let command = command.trim_start();
    let (program, rest) = match command.chars().next() {
        Some(quote @ ('"' | '\'')) => command[1..]
            .split_once(quote)
            .unwrap_or((&command[1..], "")),
        _ => command
            .split_once(char::is_whitespace)
            .unwrap_or((command, "")),
    };
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim_end_matches(".exe");
    name == "claude-dashboard" && rest.split_whitespace().next() == Some("statusline")
}

/// The person's own status line command that claude-dashboard's wraps, exactly as it stands in
/// the settings.
fn wrapped_command(settings: &Map<String, Value>) -> Option<String> {
    let command = status_line_command(settings).ok()??;
    if !is_ours(command) {
        return None;
    }
    command
        .split_once(THEN)
        .map(|(_, wrapped)| wrapped.to_owned())
}

pub fn connection(app: &App) -> Connection {
    let Some(path) = settings_path(app) else {
        return Connection::Unknown("there is no data folder".to_owned());
    };
    let settings = match read_settings(&path) {
        Ok(settings) => settings,
        Err(reason) => return Connection::Unknown(reason),
    };
    match status_line_command(&settings) {
        Ok(Some(command)) if is_ours(command) => Connection::Connected {
            wrapping: command
                .split_once(THEN)
                .map(|(_, wrapped)| wrapped.to_owned()),
        },
        Ok(current) => Connection::NotConnected {
            current: current.map(str::to_owned),
        },
        Err(reason) => Connection::Unknown(reason),
    }
}

/// How settings refer to this program: by name when that name on the `PATH` is this program,
/// and otherwise by its full path.
fn our_program() -> Result<String, String> {
    let exe = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|error| format!("cannot tell where claude-dashboard is: {error}"))?;
    let name = if cfg!(windows) {
        "claude-dashboard.exe"
    } else {
        "claude-dashboard"
    };
    let on_path = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path)
            .any(|folder| fs::canonicalize(folder.join(name)).is_ok_and(|found| found == exe))
    });
    if on_path {
        return Ok("claude-dashboard".to_owned());
    }
    let path = exe.display().to_string();
    let path = path
        .strip_prefix(r"\\?\")
        .unwrap_or(&path)
        .replace('\\', "/");
    if path.contains(char::is_whitespace) {
        return Err(format!(
            "claude-dashboard is at {path}, and a path with spaces cannot be written into a status line command; put claude-dashboard on your PATH and try again"
        ));
    }
    Ok(path)
}

/// The fields of a status line, as one line of JSON members.
fn members(status_line: &Map<String, Value>) -> String {
    status_line
        .iter()
        .map(|(key, value)| format!("{}: {value}", Value::String(key.clone())))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Claude Code's user settings and where they are, or why they cannot be read.
fn load(app: &App, verb: &str) -> Result<(PathBuf, Map<String, Value>), ExitCode> {
    let Some(path) = settings_path(app) else {
        eprintln!("There is no data folder to find Claude Code's settings in.");
        return Err(ExitCode::FAILURE);
    };
    match read_settings(&path) {
        Ok(settings) => Ok((path, settings)),
        Err(reason) => {
            eprintln!("Cannot {verb}: {reason}.");
            Err(ExitCode::FAILURE)
        }
    }
}

/// The change to the `statusLine` setting, as the design shows it: the old and new fields on
/// their own lines, in the removed and added colours.
fn describe_change(
    paint: Paint,
    path: &Path,
    old: Option<&Map<String, Value>>,
    new: Option<&Map<String, Value>>,
) -> String {
    let removed = |text: String| paint.paint(&text, palette::REMOVED, false);
    let added = |text: String| paint.paint(&text, palette::ADDED, false);
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

pub fn connect(app: &App, yes: bool) -> ExitCode {
    let (path, mut settings) = match load(app, "connect") {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let current = match status_line_command(&settings) {
        Ok(current) => current.map(str::to_owned),
        Err(reason) => {
            eprintln!("Cannot connect: {reason}.");
            return ExitCode::FAILURE;
        }
    };
    let program = match our_program() {
        Ok(program) => program,
        Err(reason) => {
            eprintln!("Cannot connect: {reason}.");
            return ExitCode::FAILURE;
        }
    };
    let new_command = match &current {
        Some(command) if is_ours(command) => {
            println!(
                "claude-dashboard's status line is already connected in {}.",
                path.display()
            );
            return ExitCode::SUCCESS;
        }
        Some(command) if has_shell_operators(command) => {
            eprintln!(
                "Your status line command uses shell operators such as pipes, which claude-dashboard cannot wrap safely:\n  {command}\nPut it in a script, point statusLine at the script, and run claude-dashboard statusline connect again."
            );
            return ExitCode::FAILURE;
        }
        Some(command) => format!("{program} statusline{THEN}{command}"),
        None => format!("{program} statusline"),
    };
    let old = settings
        .get("statusLine")
        .and_then(Value::as_object)
        .cloned();
    let mut status_line = old.clone().unwrap_or_default();
    status_line.insert("type".to_owned(), Value::String("command".to_owned()));
    status_line.insert("command".to_owned(), Value::String(new_command));

    let paint = Paint::for_stdout();
    let undo = paint.strong("claude-dashboard statusline disconnect");
    let mut out = describe_change(paint, &path, old.as_ref(), Some(&status_line));
    out.push('\n');
    out.push_str(if old.is_some() {
        "Your status line keeps showing; claude-dashboard records the plan limits Claude Code passes to it.\n"
    } else {
        "Claude Code shows claude-dashboard's status line, with the model, the plan limits and today's API-equivalent cost, and claude-dashboard records the plan limits Claude Code passes to it.\n"
    });
    if path.is_file() {
        let _ = writeln!(
            out,
            "A backup is saved next to settings.json. Undo it with {undo}."
        );
    } else {
        let _ = writeln!(out, "Undo it with {undo}.");
    }
    print!("{out}");
    if !confirmed(paint, yes) {
        return ExitCode::FAILURE;
    }
    settings.insert("statusLine".to_owned(), Value::Object(status_line));
    finish(
        &path,
        &settings,
        "Connected. Claude Code runs it after its next response.",
    )
}

pub fn disconnect(app: &App, yes: bool) -> ExitCode {
    let (path, mut settings) = match load(app, "disconnect") {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let Some(command) = status_line_command(&settings)
        .ok()
        .flatten()
        .filter(|command| is_ours(command))
        .map(str::to_owned)
    else {
        println!(
            "claude-dashboard's status line is not connected in {}.",
            path.display()
        );
        return ExitCode::SUCCESS;
    };
    let old = settings
        .get("statusLine")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let restored = command.split_once(THEN).map(|(_, wrapped)| {
        let mut status_line = old.clone();
        status_line.insert("command".to_owned(), Value::String(wrapped.to_owned()));
        status_line
    });

    let paint = Paint::for_stdout();
    let mut out = describe_change(paint, &path, Some(&old), restored.as_ref());
    out.push('\n');
    out.push_str(if restored.is_some() {
        "Claude Code shows your own status line again, and claude-dashboard stops recording plan limits from it.\n"
    } else {
        "Claude Code shows no status line, and claude-dashboard stops recording plan limits from it.\n"
    });
    out.push_str("A backup is saved next to settings.json.\n");
    print!("{out}");
    if !confirmed(paint, yes) {
        return ExitCode::FAILURE;
    }
    if let Some(status_line) = restored {
        settings.insert("statusLine".to_owned(), Value::Object(status_line));
    } else {
        settings.remove("statusLine");
    }
    finish(&path, &settings, "Disconnected.")
}

fn finish(path: &Path, settings: &Map<String, Value>, done: &str) -> ExitCode {
    match save(path, settings) {
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

/// Asks before a change, unless `yes` already confirmed it.
fn confirmed(paint: Paint, yes: bool) -> bool {
    if yes {
        return true;
    }
    if !std::io::stdin().is_terminal() {
        eprintln!(
            "\nNothing was changed. Run the command again with --yes to make the change without a question."
        );
        return false;
    }
    print!("\nMake this change? {} ", paint.strong("[y/N]"));
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    let yes = matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes");
    if !yes {
        println!("Nothing was changed.");
    }
    yes
}

/// Saves settings, keeping a backup of the file they replace, through a file beside it so that
/// a failure never leaves half a file.
fn save(path: &Path, settings: &Map<String, Value>) -> Result<(), String> {
    let folder = path.parent().ok_or("the settings file has no folder")?;
    fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    if path.is_file() {
        fs::copy(path, folder.join(BACKUP))
            .map_err(|error| format!("cannot keep a backup: {error}"))?;
    }
    let mut text = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    text.push('\n');
    let temporary = folder.join("settings.json.claude-dashboard-saving");
    fs::write(&temporary, text).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_its_own_status_line_however_it_is_reached() {
        for command in [
            "claude-dashboard statusline",
            "claude-dashboard statusline --then ~/.claude/my-statusline.sh",
            "C:/Users/ada/.cargo/bin/claude-dashboard.exe statusline",
            "\"/opt/claude dashboard/claude-dashboard\" statusline",
        ] {
            assert!(is_ours(command), "{command}");
        }
        for command in [
            "~/.claude/statusline.sh",
            "claude-dashboard today",
            "my-claude-dashboard statusline",
        ] {
            assert!(!is_ours(command), "{command}");
        }
    }

    #[test]
    fn shell_operators_count_outside_quotes_only() {
        for command in ["cat | jq .", "a; b", "a && b", "a > out", "(a)", "a `b`"] {
            assert!(has_shell_operators(command), "{command}");
        }
        for command in [
            "~/.claude/statusline.sh",
            "node ~/.claude/line.js --short",
            r#"jq -r '"[\(.model.display_name)] \(.cost | tostring)"'"#,
            r#"echo "a | b; c""#,
            r"echo a\|b",
        ] {
            assert!(!has_shell_operators(command), "{command}");
        }
    }

    #[test]
    fn the_wrapped_command_is_read_back_exactly() {
        let settings: Map<String, Value> = serde_json::from_str(
            r#"{"statusLine": {"type": "command", "command": "claude-dashboard statusline --then jq -r '\"[\\(.model.display_name)]\"'", "padding": 2}}"#,
        )
        .unwrap();
        assert_eq!(
            wrapped_command(&settings).as_deref(),
            Some(r#"jq -r '"[\(.model.display_name)]"'"#)
        );
    }

    #[test]
    fn status_line_fields_read_as_one_line_of_json() {
        let status_line: Map<String, Value> = serde_json::from_str(
            r#"{"type": "command", "command": "~/.claude/my-statusline.sh", "padding": 2}"#,
        )
        .unwrap();
        assert_eq!(
            members(&status_line),
            r#""type": "command", "command": "~/.claude/my-statusline.sh", "padding": 2"#
        );
    }
}
