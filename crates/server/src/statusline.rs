//! Claude Code's status line setting: whether claude-dashboard's status line is connected, what
//! connecting or disconnecting it would change, and making that change.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::app::App;
use crate::program;

/// What follows claude-dashboard's own status line command when it wraps the person's.
const THEN: &str = " --then ";

/// Characters that would split a wrapped command into several, or redirect it, in the shell
/// that Claude Code runs status lines in.
const STRUCTURAL: [char; 10] = ['|', ';', '&', '<', '>', '(', ')', '`', '\n', '\r'];

/// The backup kept next to `settings.json` before a change.
const BACKUP: &str = "settings.claude-dashboard-backup.json";

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
#[must_use]
pub fn settings_path(app: &App) -> Option<PathBuf> {
    app.roots
        .first()
        .map(|root| root.path.join("settings.json"))
}

/// Claude Code's settings in the file at `path`: none when the file does not exist or is empty.
///
/// # Errors
///
/// Why the file cannot be read, or that it does not hold a JSON object.
pub fn read_settings(path: &Path) -> Result<Map<String, Value>, String> {
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

/// How many days Claude Code keeps transcripts for: its `cleanupPeriodDays` setting, or 30.
#[must_use]
pub fn cleanup_period_days(app: &App) -> u32 {
    settings_path(app)
        .and_then(|path| read_settings(&path).ok())
        .and_then(|settings| settings.get("cleanupPeriodDays")?.as_u64())
        .and_then(|days| u32::try_from(days).ok())
        .unwrap_or(30)
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
#[must_use]
pub fn wrapped_command(settings: &Map<String, Value>) -> Option<String> {
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

/// The program a status line command runs, and, when that is a copy (`App::status_line_copy`), the
/// program it is copied from and the copy.
fn status_line_program(app: &App) -> Result<(PathBuf, Option<(PathBuf, PathBuf)>), String> {
    let Some(program) = &app.status_line_copy else {
        return Ok((program::this()?, None));
    };
    let copy = program::copy_path(app).ok_or(
        "there is no folder for local app data on this system to keep claude-dashboard in",
    )?;
    Ok((copy.clone(), Some((program.clone(), copy))))
}

/// The fields of a status line, as one line of JSON members.
#[must_use]
pub fn members(status_line: &Map<String, Value>) -> String {
    status_line
        .iter()
        .map(|(key, value)| format!("{}: {value}", Value::String(key.clone())))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Claude Code's user settings and where they are, or why they cannot be read, as a sentence that
/// says what could not be done.
fn load(app: &App, verb: &str) -> Result<(PathBuf, Map<String, Value>), String> {
    let Some(path) = settings_path(app) else {
        return Err("There is no data folder to find Claude Code's settings in.".to_owned());
    };
    match read_settings(&path) {
        Ok(settings) => Ok((path, settings)),
        Err(reason) => Err(format!("Cannot {verb}: {reason}.")),
    }
}

/// A change to the `statusLine` setting in Claude Code's settings, worked out but not made.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub path: PathBuf,
    /// The setting as it is; `None` when there is none.
    pub old: Option<Map<String, Value>>,
    /// The setting the change leaves; `None` when the change removes it.
    pub new: Option<Map<String, Value>>,
    /// What changes for the person once the change is made.
    pub effect: &'static str,
    /// The settings as the change leaves them.
    settings: Map<String, Value>,
    /// The program to copy, and where the copy goes, for the new status line to run.
    copy: Option<(PathBuf, PathBuf)>,
}

impl Change {
    /// The setting as it is, as one line of JSON members.
    pub fn old_members(&self) -> Option<String> {
        self.old.as_ref().map(members)
    }

    /// The setting the change leaves, as one line of JSON members.
    pub fn new_members(&self) -> Option<String> {
        self.new.as_ref().map(members)
    }

    /// Whether settings.json exists, so that a backup of it is kept.
    #[must_use]
    pub fn backed_up(&self) -> bool {
        self.path.is_file()
    }

    /// Makes the change, keeping a backup of the settings as they were.
    ///
    /// # Errors
    ///
    /// Why the program could not be copied or the settings saved.
    pub fn make(&self) -> Result<(), String> {
        if let Some((program, copy)) = &self.copy {
            program::keep_copy(program, copy)?;
        }
        save(&self.path, &self.settings)
    }
}

/// What connecting or disconnecting would do.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    Change(Box<Change>),
    /// Nothing needs changing, for the reason given.
    Nothing(String),
    /// The change cannot be made, for the reason given.
    Refused(String),
}

/// What connecting claude-dashboard's status line would change.
pub fn plan_connect(app: &App) -> Plan {
    let (path, mut settings) = match load(app, "connect") {
        Ok(loaded) => loaded,
        Err(reason) => return Plan::Refused(reason),
    };
    let current = match status_line_command(&settings) {
        Ok(current) => current.map(str::to_owned),
        Err(reason) => return Plan::Refused(format!("Cannot connect: {reason}.")),
    };
    let (program, copy) = match status_line_program(app)
        .and_then(|(path, copy)| Ok((program::command_name(&path)?, copy)))
    {
        Ok(named) => named,
        Err(reason) => return Plan::Refused(format!("Cannot connect: {reason}.")),
    };
    let new_command = match &current {
        Some(command) if is_ours(command) => {
            return Plan::Nothing(format!(
                "claude-dashboard's status line is already connected in {}.",
                path.display()
            ));
        }
        Some(command) if has_shell_operators(command) => {
            return Plan::Refused(format!(
                "Your status line command uses shell operators such as pipes, which claude-dashboard cannot wrap safely:\n  {command}\nPut it in a script, point statusLine at the script, and connect again."
            ));
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
    settings.insert("statusLine".to_owned(), Value::Object(status_line.clone()));
    let effect = if old.is_some() {
        "Your status line keeps showing; claude-dashboard records the plan limits Claude Code passes to it."
    } else {
        "Claude Code shows claude-dashboard's status line, with the model, the plan limits and today's API-equivalent cost, and claude-dashboard records the plan limits Claude Code passes to it."
    };
    Plan::Change(Box::new(Change {
        path,
        old,
        new: Some(status_line),
        effect,
        settings,
        copy,
    }))
}

/// What disconnecting claude-dashboard's status line would change: the person's own status line
/// comes back, or none.
pub fn plan_disconnect(app: &App) -> Plan {
    let (path, mut settings) = match load(app, "disconnect") {
        Ok(loaded) => loaded,
        Err(reason) => return Plan::Refused(reason),
    };
    let Some(command) = status_line_command(&settings)
        .ok()
        .flatten()
        .filter(|command| is_ours(command))
        .map(str::to_owned)
    else {
        return Plan::Nothing(format!(
            "claude-dashboard's status line is not connected in {}.",
            path.display()
        ));
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
    let effect = if let Some(status_line) = &restored {
        settings.insert("statusLine".to_owned(), Value::Object(status_line.clone()));
        "Claude Code shows your own status line again, and claude-dashboard stops recording plan limits from it."
    } else {
        settings.remove("statusLine");
        "Claude Code shows no status line, and claude-dashboard stops recording plan limits from it."
    };
    Plan::Change(Box::new(Change {
        path,
        old: Some(old),
        new: restored,
        effect,
        settings,
        copy: None,
    }))
}

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

/// Brings the status line's copy of claude-dashboard up to date with the program it is copied from,
/// when a status line runs such a copy: the program a surface brings may be newer, after an
/// update.
///
/// # Errors
///
/// Why the copy could not be brought up to date.
pub fn refresh_copy(app: &App) -> Result<(), String> {
    let (Some(program), Some(copy)) = (&app.status_line_copy, program::copy_path(app)) else {
        return Ok(());
    };
    if copy.is_file() {
        program::keep_copy(program, &copy)
    } else {
        Ok(())
    }
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
