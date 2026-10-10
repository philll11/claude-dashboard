//! Finding Claude Code's data roots and what they contain.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// The environment variable Claude Code reads to move its configuration folder.
pub const CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";

/// Where a data root came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSource {
    /// Listed in `CLAUDE_CONFIG_DIR`, which takes the place of the default.
    Environment,
    /// Claude Code's default location, `~/.claude`.
    Default,
    /// Added by the person, for example a WSL home.
    Added,
}

/// A data root to read, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataRoot {
    pub path: PathBuf,
    pub source: RootSource,
}

/// What a data root holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootStatus {
    /// The folder exists; `transcripts` and `bytes` cover every transcript under `projects`.
    Found { transcripts: u64, bytes: u64 },
    /// The folder does not exist.
    Missing,
    /// The folder is on a network or WSL location that cannot be reached.
    Unreachable { reason: String },
    /// The folder exists but cannot be read.
    Unreadable { reason: String },
}

/// Lists the data roots to read, in order and without duplicates.
///
/// `config_dir` is the value of `CLAUDE_CONFIG_DIR`: one path or a comma-separated list, where a
/// leading `~` stands for `home`. When it names at least one folder it replaces the default, as it
/// does for Claude Code. `added` roots always come last.
#[must_use]
pub fn data_roots(
    config_dir: Option<&OsStr>,
    home: Option<&Path>,
    added: &[PathBuf],
) -> Vec<DataRoot> {
    let mut roots: Vec<DataRoot> = Vec::new();
    let mut push = |path: PathBuf, source: RootSource| {
        if !roots.iter().any(|root| same_path(&root.path, &path)) {
            roots.push(DataRoot { path, source });
        }
    };

    let from_env: Vec<PathBuf> = config_dir
        .map(|value| split_list(&value.to_string_lossy(), home))
        .unwrap_or_default();
    if from_env.is_empty() {
        if let Some(home) = home {
            push(home.join(".claude"), RootSource::Default);
        }
    } else {
        for path in from_env {
            push(path, RootSource::Environment);
        }
    }
    for path in added {
        push(path.clone(), RootSource::Added);
    }
    roots
}

/// Looks at a data root and counts the transcripts under its `projects` folder.
#[must_use]
pub fn inspect(root: &Path) -> RootStatus {
    match std::fs::metadata(root) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => {
            return RootStatus::Unreadable {
                reason: "it is a file, not a folder".to_owned(),
            };
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return RootStatus::Missing,
        Err(error) if is_wsl_location(root) => {
            // Windows answers with misleading errors here, such as "The user name or password
            // is incorrect" for a distribution that is not installed.
            return RootStatus::Unreachable {
                reason: format!(
                    "the WSL distribution is not installed or not running ({})",
                    os_code(&error)
                ),
            };
        }
        Err(error) if is_network_location(root) => {
            return RootStatus::Unreachable {
                reason: error.to_string(),
            };
        }
        Err(error) => {
            return RootStatus::Unreadable {
                reason: error.to_string(),
            };
        }
    }

    let projects = root.join("projects");
    if !projects.exists() {
        return RootStatus::Found {
            transcripts: 0,
            bytes: 0,
        };
    }

    let mut transcripts = 0;
    let mut bytes = 0;
    for entry in WalkDir::new(&projects) {
        let entry = match entry {
            Ok(entry) => entry,
            // A folder that cannot be listed at the top means the whole root is unreadable;
            // deeper failures only hide part of it, so counting continues.
            Err(error) if error.depth() == 0 => {
                return RootStatus::Unreadable {
                    reason: error.to_string(),
                };
            }
            Err(_) => continue,
        };
        if entry.file_type().is_file() && entry.path().extension() == Some(OsStr::new("jsonl")) {
            transcripts += 1;
            bytes += entry.metadata().map_or(0, |meta| meta.len());
        }
    }
    RootStatus::Found { transcripts, bytes }
}

fn split_list(value: &str, home: Option<&Path>) -> Vec<PathBuf> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| expand_home(entry, home))
        .collect()
}

fn expand_home(entry: &str, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return PathBuf::from(entry);
    };
    if entry == "~" {
        return home.to_path_buf();
    }
    match entry
        .strip_prefix("~/")
        .or_else(|| entry.strip_prefix("~\\"))
    {
        Some(rest) => home.join(rest),
        None => PathBuf::from(entry),
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let normalise = |path: &Path| {
        let text = path.to_string_lossy().replace('\\', "/");
        let trimmed = text.trim_end_matches('/').to_owned();
        if cfg!(windows) {
            trimmed.to_lowercase()
        } else {
            trimmed
        }
    };
    normalise(a) == normalise(b)
}

fn is_network_location(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with(r"\\") || text.starts_with("//")
}

fn is_wsl_location(path: &Path) -> bool {
    let text = path.to_string_lossy().replace('/', "\\").to_lowercase();
    text.starts_with(r"\\wsl.localhost\") || text.starts_with(r"\\wsl$\")
}

fn os_code(error: &io::Error) -> String {
    error.raw_os_error().map_or_else(
        || error.kind().to_string(),
        |code| format!("os error {code}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/home/ada")
    }

    #[test]
    fn defaults_to_dot_claude_in_home() {
        let roots = data_roots(None, Some(&home()), &[]);
        assert_eq!(
            roots,
            vec![DataRoot {
                path: home().join(".claude"),
                source: RootSource::Default
            }]
        );
    }

    #[test]
    fn config_dir_replaces_the_default() {
        let roots = data_roots(Some(OsStr::new("/data/claude")), Some(&home()), &[]);
        assert_eq!(
            roots,
            vec![DataRoot {
                path: PathBuf::from("/data/claude"),
                source: RootSource::Environment
            }]
        );
    }

    #[test]
    fn config_dir_accepts_a_comma_separated_list() {
        let roots = data_roots(
            Some(OsStr::new(" /a , ,~/work/.claude,/b ")),
            Some(&home()),
            &[],
        );
        let paths: Vec<_> = roots.iter().map(|root| root.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/a"),
                home().join("work/.claude"),
                PathBuf::from("/b")
            ]
        );
        assert!(
            roots
                .iter()
                .all(|root| root.source == RootSource::Environment)
        );
    }

    #[test]
    fn blank_config_dir_falls_back_to_the_default() {
        let roots = data_roots(Some(OsStr::new(" , ")), Some(&home()), &[]);
        assert_eq!(roots[0].source, RootSource::Default);
    }

    #[test]
    fn added_roots_come_last_without_duplicates() {
        let added = vec![PathBuf::from("/mnt/wsl/.claude"), home().join(".claude/")];
        let roots = data_roots(None, Some(&home()), &added);
        let sources: Vec<_> = roots.iter().map(|root| root.source).collect();
        assert_eq!(sources, vec![RootSource::Default, RootSource::Added]);
    }

    #[test]
    fn no_home_and_no_config_dir_means_no_default() {
        assert_eq!(data_roots(None, None, &[]), Vec::new());
    }

    #[test]
    fn recognises_wsl_and_network_locations() {
        let wsl = Path::new(r"\\wsl.localhost\Ubuntu\home\ada\.claude");
        let legacy_wsl = Path::new(r"\\WSL$\Ubuntu\home\ada\.claude");
        let share = Path::new(r"\\fileserver\home\ada\.claude");
        let local = Path::new(r"C:\Users\ada\.claude");

        assert!(is_wsl_location(wsl) && is_network_location(wsl));
        assert!(is_wsl_location(legacy_wsl));
        assert!(!is_wsl_location(share) && is_network_location(share));
        assert!(!is_wsl_location(local) && !is_network_location(local));
    }
}
