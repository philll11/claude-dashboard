//! The program's short name, `cld`, as the installers leave it beside `claude-dashboard`: a
//! hardlink on Windows and a symlink elsewhere.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use predicates::str::contains;
use serde_json::Value;

use common::Fixture;

/// `claude-dashboard` and `cld` side by side in a folder of their own. It sits beside the built
/// program, because a hardlink cannot reach another drive, and Windows runners keep Temp on one.
struct Installed {
    folder: tempfile::TempDir,
}

impl Installed {
    fn new() -> Self {
        let built = PathBuf::from(env!("CARGO_BIN_EXE_claude-dashboard"));
        let folder = tempfile::Builder::new()
            .prefix("installed-")
            .tempdir_in(built.parent().unwrap())
            .unwrap();
        let installed = Self { folder };
        link(&built, &installed.program("claude-dashboard"));
        link(
            &installed.program("claude-dashboard"),
            &installed.program("cld"),
        );
        installed
    }

    fn program(&self, name: &str) -> PathBuf {
        self.folder
            .path()
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    }
}

#[cfg(windows)]
fn link(target: &Path, link: &Path) {
    fs::hard_link(target, link).unwrap();
}

#[cfg(not(windows))]
fn link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

/// The command a status line setting holds.
fn status_line(fixture: &Fixture) -> String {
    match &fixture.settings()["statusLine"]["command"] {
        Value::String(command) => command.clone(),
        other => panic!("the status line command is {other}"),
    }
}

#[test]
fn the_short_name_runs_the_same_program() {
    let installed = Installed::new();
    let fixture = Fixture::new();
    let cld = installed.program("cld");

    fixture
        .cli_at(&cld, &["--version"])
        .assert()
        .success()
        .stdout(format!("claude-dashboard {}\n", env!("CARGO_PKG_VERSION")));
    fixture
        .cli_at(&cld, &["--help"])
        .assert()
        .success()
        .stdout(contains("Usage: cld"));
    fs::create_dir_all(fixture.root().join("projects")).unwrap();
    fixture
        .cli_at(&cld, &["today"])
        .assert()
        .success()
        .stdout(contains("No requests today."));
}

#[test]
fn connecting_through_the_short_name_writes_the_full_one() {
    let installed = Installed::new();
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();
    let cld = installed.program("cld");
    let connect = || {
        let mut command = fixture.cli_at(&cld, &["statusline", "connect", "--yes"]);
        command.env("PATH", fixture.dir.path().join("empty"));
        command
    };

    connect().assert().success().stdout(contains("Connected."));
    let command = status_line(&fixture);
    let program = command
        .strip_suffix(" statusline")
        .unwrap_or_else(|| panic!("the status line runs {command}"));
    assert!(
        program.ends_with(&format!(
            "/claude-dashboard{}",
            std::env::consts::EXE_SUFFIX
        )),
        "the status line names {program}"
    );

    connect()
        .assert()
        .success()
        .stdout(contains("already connected"));
    assert_eq!(status_line(&fixture), command);
}

#[test]
fn on_the_path_the_short_name_connects_as_claude_dashboard() {
    let installed = Installed::new();
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();
    let cld = installed.program("cld");
    let connect = || {
        let mut command = fixture.cli_at(&cld, &["statusline", "connect", "--yes"]);
        command.env("PATH", installed.folder.path());
        command
    };

    connect().assert().success().stdout(contains("Connected."));
    assert_eq!(status_line(&fixture), "claude-dashboard statusline");
    connect()
        .assert()
        .success()
        .stdout(contains("already connected"));
}
