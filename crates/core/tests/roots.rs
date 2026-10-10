use std::fs;
use std::path::Path;

use claude_dashboard_core::roots::{RootStatus, inspect};

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn a_missing_folder_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(inspect(&dir.path().join("absent")), RootStatus::Missing);
}

#[test]
fn a_file_is_not_a_data_root() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    write(&file, "{}");
    assert!(matches!(inspect(&file), RootStatus::Unreadable { .. }));
}

#[test]
fn a_root_without_projects_holds_no_transcripts() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        inspect(dir.path()),
        RootStatus::Found {
            transcripts: 0,
            bytes: 0
        }
    );
}

#[test]
fn counts_session_and_subagent_transcripts_only() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects").join("C--code-app");
    write(&project.join("4f1c.jsonl"), "{\"type\":\"user\"}\n");
    write(
        &project
            .join("4f1c")
            .join("subagents")
            .join("agent-a1.jsonl"),
        "{}\n{}\n",
    );
    write(
        &project.join("4f1c").join("tool-results").join("out.txt"),
        "not a transcript",
    );
    write(&dir.path().join("history.jsonl"), "outside projects");

    assert_eq!(
        inspect(dir.path()),
        RootStatus::Found {
            transcripts: 2,
            bytes: 16 + 6
        }
    );
}

#[cfg(unix)]
#[test]
fn an_unlistable_projects_folder_is_unreadable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    fs::create_dir(&projects).unwrap();
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = fs::read_dir(&projects).is_ok();

    let status = inspect(dir.path());
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o755)).unwrap();

    // Running as root ignores permissions, so the check only applies when they take effect.
    if !readable_anyway {
        assert!(
            matches!(status, RootStatus::Unreadable { .. }),
            "{status:?}"
        );
    }
}
