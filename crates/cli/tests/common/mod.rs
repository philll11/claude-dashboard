//! What the CLI's tests share: a data root of made-up transcripts and a history of their own, in
//! a folder that goes away after each test, and a fixed moment for every command to see.

#![allow(dead_code, reason = "each test file uses a part of these")]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};

/// The moment every command the tests run sees, so that no test depends on when it runs: midday,
/// in UTC, on the price table's date.
pub const NOW: &str = "2026-10-10T12:00:00Z";
pub const TODAY: &str = "2026-10-10";

pub struct Fixture {
    pub dir: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn root(&self) -> PathBuf {
        self.dir.path().join(".claude")
    }

    pub fn history(&self) -> PathBuf {
        self.dir.path().join("history.sqlite")
    }

    /// The command, kept to this fixture's data root and history, in UTC and without colour.
    pub fn cli(&self, args: &[&str]) -> Command {
        self.cli_at(Path::new(env!("CARGO_BIN_EXE_claude-dashboard")), args)
    }

    /// The command as `cli` gives it, run from `program`.
    pub fn cli_at(&self, program: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command
            .args(args)
            .env("CLAUDE_CONFIG_DIR", self.root())
            .env("CLAUDE_DASHBOARD_HISTORY", self.history())
            .env("CLAUDE_DASHBOARD_NOW", NOW)
            .env("TZ", "UTC")
            .env("NO_COLOR", "1")
            .env_remove("COLORTERM")
            .env_remove("WT_SESSION");
        command
    }

    /// Writes a session's transcript in the project `C:\code\app`.
    pub fn transcript(&self, session: &str, records: &[Value]) {
        let path = self
            .root()
            .join("projects")
            .join("C--code-app")
            .join(format!("{session}.jsonl"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let lines: Vec<String> = records.iter().map(Value::to_string).collect();
        fs::write(path, lines.join("\n") + "\n").unwrap();
    }

    /// The command with nothing on the `PATH`, so that `connect` names claude-dashboard by its
    /// full path on every platform.
    pub fn cli_without_path(&self, args: &[&str]) -> Command {
        let mut command = self.cli(args);
        command.env("PATH", self.dir.path().join("empty"));
        command
    }

    pub fn settings(&self) -> Value {
        serde_json::from_str(&fs::read_to_string(self.root().join("settings.json")).unwrap())
            .unwrap()
    }
}

/// An assistant record of `model` made at `timestamp`, costing $0.031712 on Claude Opus 5.5.
pub fn request(id: &str, model: &str, timestamp: &str) -> Value {
    json!({
        "type": "assistant", "uuid": format!("rec-{id}"), "requestId": format!("req_{id}"),
        "timestamp": timestamp, "cwd": "C:\\code\\app",
        "message": {"id": format!("msg_{id}"), "model": model, "content": [],
            "usage": {"input_tokens": 3, "output_tokens": 640, "cache_read_input_tokens": 42000,
                "cache_creation_input_tokens": 1500,
                "cache_creation": {"ephemeral_5m_input_tokens": 500, "ephemeral_1h_input_tokens": 1000}}}
    })
}

pub fn today_at(time: &str) -> String {
    format!("{TODAY}T{time}Z")
}
