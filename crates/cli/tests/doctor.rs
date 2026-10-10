use std::fs;

use assert_cmd::Command;
use predicates::str::contains;

fn doctor() -> Command {
    let mut command = Command::cargo_bin("claude-dashboard").unwrap();
    command.arg("doctor");
    command
}

#[test]
fn reports_the_transcripts_in_config_dir() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects").join("C--code-app");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("4f1c.jsonl"), "{}\n").unwrap();

    doctor()
        .env("CLAUDE_CONFIG_DIR", dir.path())
        .assert()
        .success()
        .stdout(contains("Data folders"))
        .stdout(contains("(CLAUDE_CONFIG_DIR)  1 transcript, 3 B"));
}

#[test]
fn fails_when_no_data_folder_exists() {
    let dir = tempfile::tempdir().unwrap();

    doctor()
        .env("CLAUDE_CONFIG_DIR", dir.path().join("absent"))
        .assert()
        .failure()
        .stdout(contains("not found"));
}
