//! Where claude-dashboard's program is, the copy of it that a status line can run, and how a
//! command that Claude Code runs names it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use semver::Version;

use crate::app::App;

/// The program's file name.
pub const FILE_NAME: &str = if cfg!(windows) {
    "claude-dashboard.exe"
} else {
    "claude-dashboard"
};

/// The short name that the installers give the program beside its own: a link to the same file.
pub const ALIAS: &str = "cld";

/// How old a half-made copy must be before it counts as abandoned rather than under way.
const ABANDONED: Duration = Duration::from_hours(1);

/// This program, with links resolved.
///
/// # Errors
///
/// Why the program's own path cannot be told.
pub fn this() -> Result<PathBuf, String> {
    std::env::current_exe()
        .and_then(fs::canonicalize)
        .map(in_full)
        .map_err(|error| format!("cannot tell where claude-dashboard is: {error}"))
}

/// `program`, or the `claude-dashboard` beside it when `program` is its short name. On Windows the
/// short name is a hardlink, which keeps the name it was started by where a symlink resolves to
/// the program's own.
fn in_full(program: PathBuf) -> PathBuf {
    if program.file_stem().is_none_or(|stem| stem != ALIAS) {
        return program;
    }
    fs::canonicalize(program.with_file_name(FILE_NAME)).unwrap_or(program)
}

/// Where the copy lives that a status line runs for a surface whose program an update moves,
/// such as the VS Code extension and the desktop app: `bin` in the folder that holds the history.
#[must_use]
pub fn copy_path(app: &App) -> Option<PathBuf> {
    Some(
        app.history_path
            .as_ref()?
            .parent()?
            .join("bin")
            .join(FILE_NAME),
    )
}

/// Puts `program` at `copy`, unless a copy of the same or a newer version is there already, so
/// that surfaces of different versions never take the copy back to an older one.
///
/// # Errors
///
/// Why the copy could not be made.
pub fn keep_copy(program: &Path, copy: &Path) -> Result<(), String> {
    let program = fs::canonicalize(program)
        .map_err(|error| format!("cannot find {}: {error}", program.display()))?;
    if fs::canonicalize(copy).is_ok_and(|found| found == program) {
        return Ok(());
    }
    let ours = version_at(&program)
        .ok_or_else(|| format!("cannot tell the version of {}", program.display()))?;
    if version_at(copy).is_some_and(|theirs| theirs >= ours) {
        return Ok(());
    }
    let folder = copy
        .parent()
        .ok_or("the copy of claude-dashboard has no folder")?;
    fs::create_dir_all(folder)
        .map_err(|error| format!("cannot create {}: {error}", folder.display()))?;
    remove_leftovers(folder);
    let id = std::process::id();
    let new = folder.join(format!("{FILE_NAME}.{id}.new"));
    fs::copy(&program, &new).map_err(|error| {
        format!(
            "cannot copy claude-dashboard to {}: {error}",
            folder.display()
        )
    })?;
    replace(&new, copy).inspect_err(|_| {
        let _ = fs::remove_file(&new);
    })
}

/// Moves `new` to `copy`. Windows cannot replace a program while it runs, as Claude Code may be
/// running the copy, but it can rename one, so the old copy moves aside first.
fn replace(new: &Path, copy: &Path) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("cannot replace {}: {error}", copy.display());
    if cfg!(windows) && copy.exists() {
        let old = copy.with_file_name(format!("{FILE_NAME}.{}.old", std::process::id()));
        fs::rename(copy, &old).map_err(failed)?;
        if let Err(error) = fs::rename(new, copy) {
            let _ = fs::rename(&old, copy);
            return Err(failed(error));
        }
        let _ = fs::remove_file(&old);
        Ok(())
    } else {
        fs::rename(new, copy).map_err(failed)
    }
}

/// Removes old copies that were moved aside while running, and copies abandoned half made.
fn remove_leftovers(folder: &Path) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let ours = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&format!("{FILE_NAME}.")));
        let abandoned = || {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|modified| now.duration_since(modified).unwrap_or_default() > ABANDONED)
        };
        let leftover = match path.extension().and_then(|extension| extension.to_str()) {
            Some("old") => true,
            Some("new") => abandoned(),
            _ => false,
        };
        if ours && leftover {
            let _ = fs::remove_file(&path);
        }
    }
}

/// The version of the program at `path`, as it reports it.
fn version_at(path: &Path) -> Option<Version> {
    if !path.is_file() {
        return None;
    }
    let output = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    Version::parse(text.trim().strip_prefix("claude-dashboard ")?).ok()
}

/// How a command that Claude Code runs names `program`: `claude-dashboard` when that name on the
/// `PATH` is `program`, and otherwise its path.
///
/// # Errors
///
/// Why no command could name the program.
pub fn command_name(program: &Path) -> Result<String, String> {
    let on_path = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|folder| {
            fs::canonicalize(folder.join(FILE_NAME)).is_ok_and(|found| found == program)
        })
    });
    if on_path {
        return Ok("claude-dashboard".to_owned());
    }
    let home = shell_home().map(|home| slashed(&home));
    spelled(&slashed(program), home.as_deref(), cfg!(windows))
}

/// A path with forward slashes and without Windows' `\\?\` prefix, as shells take it.
fn slashed(path: &Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .replace('\\', "/")
}

/// The folder that `~` stands for in every shell Claude Code might run a command in. On Windows
/// that is Git Bash, whose `~` is `HOME` when it is set, or else PowerShell, whose `~` is
/// `USERPROFILE`, so there is one only while the two agree.
fn shell_home() -> Option<PathBuf> {
    if !cfg!(windows) {
        return std::env::var_os("HOME").map(PathBuf::from);
    }
    let profile = PathBuf::from(std::env::var_os("USERPROFILE")?);
    match std::env::var_os("HOME") {
        None => Some(profile),
        Some(home) if same_folder(&slashed(Path::new(&home)), &slashed(&profile)) => Some(profile),
        Some(_) => None,
    }
}

fn same_folder(a: &str, b: &str) -> bool {
    a.trim_end_matches('/')
        .eq_ignore_ascii_case(b.trim_end_matches('/'))
}

/// `path` as the first word of a shell command. A path that needs no quoting stands as it is. On
/// macOS and Linux one that does is quoted. On Windows, Git Bash and PowerShell quote differently,
/// so a path inside the home folder starts from `~`, which both expand as one word, and any other
/// path that needs quoting cannot be written.
fn spelled(path: &str, home: Option<&str>, windows: bool) -> Result<String, String> {
    if plain(path, windows) {
        return Ok(path.to_owned());
    }
    if !windows {
        return Ok(format!("'{}'", path.replace('\'', r"'\''")));
    }
    let within_home = home.and_then(|home| {
        let home = home.trim_end_matches('/');
        let (start, rest) = path.split_at_checked(home.len())?;
        let rest = rest.strip_prefix('/')?;
        start.eq_ignore_ascii_case(home).then_some(rest)
    });
    match within_home {
        Some(rest) if plain(rest, false) => Ok(format!("~/{rest}")),
        _ => Err(format!(
            "claude-dashboard is at {path}, and a status line command cannot name a program whose path has spaces or other special characters outside your home folder; put claude-dashboard on your PATH and try again"
        )),
    }
}

/// Whether `path` reads the same in every shell without quoting. A colon may only follow a
/// Windows drive letter.
fn plain(path: &str, windows: bool) -> bool {
    path.char_indices().all(|(index, c)| {
        c.is_alphanumeric()
            || matches!(c, '/' | '.' | '_' | '-' | '+')
            || (c == '~' && index > 0)
            || (c == ':' && windows && index == 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_path_stands_as_it_is() {
        assert_eq!(
            spelled(
                "C:/Users/ada/.cargo/bin/claude-dashboard.exe",
                Some("C:/Users/ada"),
                true
            ),
            Ok("C:/Users/ada/.cargo/bin/claude-dashboard.exe".to_owned())
        );
        assert_eq!(
            spelled(
                "/home/ada/.local/bin/claude-dashboard",
                Some("/home/ada"),
                false
            ),
            Ok("/home/ada/.local/bin/claude-dashboard".to_owned())
        );
        assert_eq!(
            spelled(
                "C:/Users/ADA~1/AppData/Local/claude-dashboard.exe",
                None,
                true
            ),
            Ok("C:/Users/ADA~1/AppData/Local/claude-dashboard.exe".to_owned())
        );
    }

    #[test]
    fn on_windows_a_home_folder_with_spaces_is_written_as_a_tilde() {
        assert_eq!(
            spelled(
                "C:/Users/Ada Lovelace/AppData/Local/claude-dashboard/bin/claude-dashboard.exe",
                Some("c:/users/ada lovelace/"),
                true
            ),
            Ok("~/AppData/Local/claude-dashboard/bin/claude-dashboard.exe".to_owned())
        );
    }

    #[test]
    fn on_windows_other_paths_that_need_quoting_cannot_be_written() {
        for (path, home) in [
            (
                "C:/Program Files/claude-dashboard/claude-dashboard.exe",
                Some("C:/Users/ada"),
            ),
            (
                "C:/Users/Ada Lovelace/My Tools/claude-dashboard.exe",
                Some("C:/Users/Ada Lovelace"),
            ),
            (
                "C:/Users/Ada Lovelace/AppData/Local/claude-dashboard.exe",
                None,
            ),
            (
                "C:/Users/Ada Lovelace2/claude-dashboard.exe",
                Some("C:/Users/Ada Lovelace"),
            ),
            ("C:/tools/a,b/claude-dashboard.exe", Some("C:/Users/ada")),
            ("C:/tools/a:b/claude-dashboard.exe", Some("C:/Users/ada")),
        ] {
            assert!(spelled(path, home, true).is_err(), "{path}");
        }
    }

    #[test]
    fn on_macos_and_linux_a_path_that_needs_quoting_is_quoted() {
        assert_eq!(
            spelled(
                "/Users/ada/Library/Application Support/claude-dashboard/bin/claude-dashboard",
                Some("/Users/ada"),
                false
            ),
            Ok(
                "'/Users/ada/Library/Application Support/claude-dashboard/bin/claude-dashboard'"
                    .to_owned()
            )
        );
        assert_eq!(
            spelled("/opt/ada's tools/claude-dashboard", None, false),
            Ok(r"'/opt/ada'\''s tools/claude-dashboard'".to_owned())
        );
    }

    #[test]
    fn its_short_name_is_the_program_beside_it() {
        let folder = tempfile::tempdir().unwrap();
        let alias = folder
            .path()
            .join(format!("{ALIAS}{}", std::env::consts::EXE_SUFFIX));
        let other = folder.path().join("helper");
        fs::write(&alias, "").unwrap();
        fs::write(&other, "").unwrap();
        assert_eq!(in_full(alias.clone()), alias, "with nothing beside it");

        let full = folder.path().join(FILE_NAME);
        fs::write(&full, "").unwrap();
        assert_eq!(in_full(alias), fs::canonicalize(&full).unwrap());
        assert_eq!(in_full(full.clone()), full);
        assert_eq!(in_full(other.clone()), other);
    }

    #[test]
    fn the_copy_lives_next_to_the_history() {
        let app = App {
            history_path: Some(PathBuf::from("/data/claude-dashboard/history.sqlite")),
            ..App::new()
        };
        assert_eq!(
            copy_path(&app),
            Some(PathBuf::from("/data/claude-dashboard/bin").join(FILE_NAME))
        );
    }
}
