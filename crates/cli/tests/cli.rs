use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use serde_json::{Value, json};

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join(".claude")
    }

    fn history(&self) -> PathBuf {
        self.dir.path().join("history.sqlite")
    }

    /// The command, kept to this fixture's data root and history, in UTC and without colour.
    fn cli(&self, args: &[&str]) -> Command {
        let mut command = Command::cargo_bin("claude-dashboard").unwrap();
        command
            .args(args)
            .env("CLAUDE_CONFIG_DIR", self.root())
            .env("CLAUDE_DASHBOARD_HISTORY", self.history())
            .env("TZ", "UTC")
            .env("NO_COLOR", "1")
            .env_remove("COLORTERM")
            .env_remove("WT_SESSION");
        command
    }

    fn transcript(&self, session: &str, records: &[Value]) {
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
    fn cli_without_path(&self, args: &[&str]) -> Command {
        let mut command = self.cli(args);
        command.env("PATH", self.dir.path().join("empty"));
        command
    }

    fn settings(&self) -> Value {
        serde_json::from_str(&fs::read_to_string(self.root().join("settings.json")).unwrap())
            .unwrap()
    }
}

/// An assistant record of `model` made at `timestamp`, costing $0.031712 on Claude Opus 5.5.
fn request(id: &str, model: &str, timestamp: &str) -> Value {
    json!({
        "type": "assistant", "uuid": format!("rec-{id}"), "requestId": format!("req_{id}"),
        "timestamp": timestamp, "cwd": "C:\\code\\app",
        "message": {"id": format!("msg_{id}"), "model": model, "content": [],
            "usage": {"input_tokens": 3, "output_tokens": 640, "cache_read_input_tokens": 42000,
                "cache_creation_input_tokens": 1500,
                "cache_creation": {"ephemeral_5m_input_tokens": 500, "ephemeral_1h_input_tokens": 1000}}}
    })
}

fn today_at(time: &str) -> String {
    format!(
        "{}T{time}Z",
        jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date()
    )
}

#[test]
fn doctor_reports_the_data_folder_history_prices_and_limits() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );

    let output = fixture.cli(&["doctor"]).assert().success();

    let root = fixture.root().display().to_string();
    output
        .stdout(contains(format!("✓ Data folder    {root}   1 transcript")))
        .stdout(contains("(CLAUDE_CONFIG_DIR)"))
        .stdout(contains("✓ History        "))
        .stdout(contains("integrity check passed"))
        .stdout(contains(
            "✓ Prices         table of 10 Oct 2026, all 1 model priced",
        ))
        .stdout(contains("- Limits         status line not connected"))
        .stdout(contains(
            "- Limits         usage endpoint off (experimental, opt-in)",
        ));
}

#[test]
fn doctor_fails_when_no_data_folder_exists() {
    let fixture = Fixture::new();
    fixture
        .cli(&["doctor"])
        .assert()
        .failure()
        .stdout(contains("✗ Data folder"))
        .stdout(contains("not found"));
}

#[test]
fn today_shows_each_model_and_the_total() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[
            request("1", "claude-opus-5-5", &today_at("00:00:01")),
            request("2", "claude-opus-5-5", &today_at("00:00:02")),
            request("3", "claude-sonnet-5-5", &today_at("00:00:03")),
        ],
    );

    fixture
        .cli(&["today"])
        .assert()
        .success()
        .stdout(contains("API-equivalent estimate · prices of 10 Oct 2026"))
        .stdout(contains(
            "Model         Requests    Output   Cache writes   Cache reads     Cost",
        ))
        .stdout(contains(
            "■ Opus 5.5           2        1K             3K           84K    $0.06",
        ))
        .stdout(contains(
            "■ Sonnet 5.5         1       640             2K           42K    $0.02",
        ))
        .stdout(contains(
            "  Total              3        2K             5K          126K    $0.08",
        ))
        .stdout(contains(
            "Plan limits: not recorded yet. To record them, run claude-dashboard statusline connect",
        ));
}

#[test]
fn today_without_requests_says_so() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root().join("projects")).unwrap();
    fixture
        .cli(&["today"])
        .assert()
        .success()
        .stdout(contains("No requests today."));
}

#[test]
fn daily_writes_json_and_csv() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[
            request("1", "claude-opus-5-5", "2026-10-05T10:00:00Z"),
            request("2", "claude-sonnet-5-5", "2026-10-06T10:00:00Z"),
        ],
    );

    let output = fixture
        .cli(&[
            "daily",
            "--since",
            "2026-10-05",
            "--until",
            "2026-10-06",
            "--json",
        ])
        .assert()
        .success();
    let days: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(days.as_array().unwrap().len(), 2);
    assert_eq!(days[0]["date"], "2026-10-05");
    assert_eq!(days[0]["requests"], 1);
    assert_eq!(days[0]["tokens"]["cache_write_1h"], 1000);
    assert_eq!(days[0]["cost"]["usd"], 0.03);
    assert_eq!(days[0]["cost"]["provenance"], "estimate");
    assert_eq!(days[0]["cost"]["price_table"], "2026-10-10");
    assert_eq!(days[0]["models"], json!(["claude-opus-5-5"]));

    fixture
        .cli(&[
            "daily",
            "--since",
            "2026-10-05",
            "--until",
            "2026-10-06",
            "--csv",
        ])
        .assert()
        .success()
        .stdout(contains("date,requests,input_tokens,output_tokens"))
        .stdout(contains("2026-10-06,1,3,640,500,1000,42000,"));
}

fn status_line_input(fixture: &Fixture) -> String {
    let transcript = fixture
        .root()
        .join("projects")
        .join("C--code-app")
        .join("s1.jsonl");
    json!({
        "session_id": "s1",
        "transcript_path": transcript,
        "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
        "rate_limits": {
            "five_hour": {"used_percentage": 42.0, "resets_at": 4_102_444_800_i64},
            "seven_day": {"used_percentage": 71.4, "resets_at": 4_103_049_600_i64}
        }
    })
    .to_string()
}

#[test]
fn the_status_line_records_the_limits_and_shows_them() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );

    fixture
        .cli(&["statusline"])
        .write_stdin(status_line_input(&fixture))
        .assert()
        .success()
        .stdout("Opus 5.5 · 5h 42% · week 71% ▲ · $0.03 today");

    fixture
        .cli(&["today"])
        .assert()
        .success()
        .stdout(contains(
            "Plan limits, official, just now: five-hour 42% resets",
        ))
        .stdout(contains("weekly 71% ▲ resets"));
}

#[test]
fn the_status_line_runs_your_own_after_recording() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();

    fixture
        .cli(&["statusline", "--then", "echo", "your own line"])
        .write_stdin(status_line_input(&fixture))
        .assert()
        .success()
        .stdout(contains("your own line"));

    fixture
        .cli(&["today"])
        .assert()
        .stdout(contains("five-hour 42%"));
}

fn program() -> String {
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_claude-dashboard"));
    let path = fs::canonicalize(exe).unwrap().display().to_string();
    path.strip_prefix(r"\\?\")
        .unwrap_or(&path)
        .replace('\\', "/")
}

#[test]
fn connecting_adds_the_status_line_and_disconnecting_removes_it() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();
    fs::write(
        fixture.root().join("settings.json"),
        "{\n  \"model\": \"opus\",\n  \"theme\": \"dark\"\n}\n",
    )
    .unwrap();

    fixture
        .cli_without_path(&["statusline", "connect", "--yes"])
        .assert()
        .success()
        .stdout(contains("This adds one setting to"))
        .stdout(contains(format!(
            "+ \"statusLine\": {{ \"type\": \"command\", \"command\": \"{} statusline\" }}",
            program()
        )))
        .stdout(contains("Connected."));
    let settings = fixture.settings();
    let keys: Vec<&str> = settings
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["model", "theme", "statusLine"]);
    assert!(
        fixture
            .root()
            .join("settings.claude-dashboard-backup.json")
            .is_file()
    );

    fixture
        .cli_without_path(&["statusline", "connect", "--yes"])
        .assert()
        .success()
        .stdout(contains("already connected"));

    fixture
        .cli_without_path(&["statusline", "disconnect", "--yes"])
        .assert()
        .success()
        .stdout(contains("This removes one setting from"))
        .stdout(contains("Disconnected."));
    assert_eq!(
        fixture.settings(),
        json!({"model": "opus", "theme": "dark"})
    );
}

#[test]
fn connecting_wraps_your_own_status_line_and_disconnecting_restores_it() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();
    let own = r#"jq -r '"[\(.model.display_name)]"'"#;
    fs::write(
        fixture.root().join("settings.json"),
        json!({"statusLine": {"type": "command", "command": own, "padding": 2}}).to_string(),
    )
    .unwrap();

    fixture
        .cli_without_path(&["statusline", "connect", "--yes"])
        .assert()
        .success()
        .stdout(contains("This changes one setting in"))
        .stdout(contains("Your status line keeps showing"));
    assert_eq!(
        fixture.settings()["statusLine"],
        json!({"type": "command", "command": format!("{} statusline --then {own}", program()), "padding": 2})
    );

    fixture
        .cli_without_path(&["statusline", "disconnect", "--yes"])
        .assert()
        .success();
    assert_eq!(
        fixture.settings()["statusLine"],
        json!({"type": "command", "command": own, "padding": 2})
    );
}

#[test]
fn connecting_refuses_a_command_with_shell_operators() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();
    let settings = json!({"statusLine": {"type": "command", "command": "cat | jq ."}}).to_string();
    fs::write(fixture.root().join("settings.json"), &settings).unwrap();

    fixture
        .cli_without_path(&["statusline", "connect", "--yes"])
        .assert()
        .failure()
        .stderr(contains("cannot wrap safely"));
    assert_eq!(
        fs::read_to_string(fixture.root().join("settings.json")).unwrap(),
        settings
    );
}

#[test]
fn connecting_asks_before_changing_anything() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();

    fixture
        .cli(&["statusline", "connect"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains("Nothing was changed"));
    assert!(!fixture.root().join("settings.json").exists());
}

fn claude_usage(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (session_id TEXT PRIMARY KEY, project_name TEXT, topic TEXT);
         CREATE TABLE turns (
             id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, timestamp TEXT, model TEXT,
             input_tokens INTEGER, output_tokens INTEGER, cache_read_tokens INTEGER,
             cache_creation_tokens INTEGER, cwd TEXT, message_id TEXT, is_subagent INTEGER,
             agent_id TEXT
         );
         INSERT INTO turns (session_id, timestamp, model, input_tokens, output_tokens,
             cache_read_tokens, cache_creation_tokens, cwd, message_id, is_subagent)
         VALUES ('old', '2026-08-01T10:00:00.000Z', 'claude-opus-5', 4, 60, 3000, 90,
             'C:\\code\\app', 'msg_old', 0);",
    )
    .unwrap();
}

#[test]
fn importing_claude_usage_adds_what_the_transcripts_no_longer_hold() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root().join("projects")).unwrap();
    claude_usage(&fixture.root().join("usage.db"));

    fixture
        .cli(&["doctor"])
        .assert()
        .stdout(contains("! Old history    claude-usage's database at"));

    fixture
        .cli(&["import", "claude-usage"])
        .assert()
        .success()
        .stdout(contains("Imported 1 request from"))
        .stdout(contains("the history already held 0."));

    fixture.cli(&["doctor"]).assert().stdout(contains(
        "✓ Old history    1 requests imported from claude-usage on",
    ));
}

#[test]
fn json_is_written_without_colour_when_piped() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root().join("projects")).unwrap();
    fixture
        .cli(&["today", "--json"])
        .env_remove("NO_COLOR")
        .assert()
        .success()
        .stdout(predicate::str::contains('\u{1b}').not());
}
