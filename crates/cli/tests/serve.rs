//! `claude-dashboard serve --stdio`, spoken to as the VS Code extension speaks to it: JSON-RPC
//! with `Content-Length` framing over its standard input and output.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(20);
/// The moment the server and the commands the tests run see, so that no test depends on when it
/// runs: midday, in UTC, on the price table's date.
const NOW: &str = "2026-10-10T12:00:00Z";
const TODAY: &str = "2026-10-10";

struct Server {
    /// The data root and history, kept until the test ends.
    dir: tempfile::TempDir,
    root: PathBuf,
    history: PathBuf,
    child: Child,
    input: Option<ChildStdin>,
    messages: Receiver<Value>,
    next_id: u64,
}

impl Server {
    /// A server over a data root with one session of two requests today, and a new history.
    fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".claude");
        let history = dir.path().join("history.sqlite");
        write_transcript(
            &root,
            &[
                request("1", "claude-opus-5-5", &today_at("00:00:01")),
                request("2", "claude-sonnet-5-5", &today_at("00:00:02")),
            ],
        );
        Self::start_with(dir, root, history)
    }

    fn start_with(dir: tempfile::TempDir, root: PathBuf, history: PathBuf) -> Self {
        Self::launch(dir, root, history, &[])
    }

    fn launch(dir: tempfile::TempDir, root: PathBuf, history: PathBuf, options: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_claude-dashboard"))
            .args(["serve", "--stdio"])
            .args(options)
            .env("CLAUDE_CONFIG_DIR", &root)
            .env("CLAUDE_DASHBOARD_HISTORY", &history)
            .env("CLAUDE_DASHBOARD_NOW", NOW)
            .env("TZ", "UTC")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = child.stdout.take().unwrap();
        let (sender, messages) = mpsc::channel();
        thread::spawn(move || {
            let mut output = BufReader::new(output);
            while let Some(message) = read_message(&mut output) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        Self {
            dir,
            root,
            history,
            child,
            input,
            messages,
            next_id: 0,
        }
    }

    fn transcript(&self, records: &[Value]) {
        write_transcript(&self.root, records);
    }

    fn send(&mut self, message: &Value) {
        let body = message.to_string();
        let input = self.input.as_mut().unwrap();
        write!(input, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        input.flush().unwrap();
    }

    /// Sends a request, and returns its id without waiting for the response.
    fn ask(&mut self, method: &str, params: &Value) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        id
    }

    /// Sends a request and returns its response, passing over the notifications that arrive first.
    fn request(&mut self, method: &str, params: &Value) -> Value {
        let id = self.ask(method, params);
        self.wait_for(|message| message["id"] == id)
    }

    fn result(&mut self, method: &str, params: &Value) -> Value {
        let response = self.request(method, params);
        assert!(
            response.get("error").is_none(),
            "{method} failed: {response}"
        );
        response["result"].clone()
    }

    /// Waits for the responses to `ids`, which may come in any order, passing over the rest.
    fn responses(&self, ids: &[u64]) -> HashMap<u64, Value> {
        let mut responses = HashMap::new();
        while responses.len() < ids.len() {
            let response =
                self.wait_for(|message| message["id"].as_u64().is_some_and(|id| ids.contains(&id)));
            let id = response["id"]
                .as_u64()
                .expect("a response has its request's id");
            responses.insert(id, response);
        }
        responses
    }

    fn wait_for(&self, wanted: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let message = self
                .messages
                .recv_timeout(left)
                .expect("the server answers in time");
            if wanted(&message) {
                return message;
            }
        }
    }

    /// Waits until the server has read new transcript content.
    fn changed(&self) -> Value {
        self.wait_for(|message| message["method"] == "history/changed")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Adds records to the session transcript in `root`.
fn write_transcript(root: &Path, records: &[Value]) {
    let path = root
        .join("projects")
        .join("C--code-app")
        .join("4f1c2a3b-0000-4000-8000-000000000001.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    for record in records {
        writeln!(file, "{record}").unwrap();
    }
}

fn read_message(output: &mut impl BufRead) -> Option<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if output.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            length = value.parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length?];
    output.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn request(id: &str, model: &str, timestamp: &str) -> Value {
    json!({
        "type": "assistant", "uuid": format!("rec-{id}"), "requestId": format!("req_{id}"),
        "timestamp": timestamp, "cwd": "C:\\code\\app", "version": "2.1.296", "entrypoint": "cli",
        "message": {"id": format!("msg_{id}"), "model": model, "content": [],
            "usage": {"input_tokens": 3, "output_tokens": 640, "cache_read_input_tokens": 42000,
                "cache_creation_input_tokens": 1500}}
    })
}

fn today() -> String {
    TODAY.to_owned()
}

fn today_at(time: &str) -> String {
    format!("{}T{time}Z", today())
}

#[test]
fn answers_with_the_history_it_reads() {
    let mut server = Server::start();
    server.changed();

    let status = server.result("status", &Value::Null);
    assert_eq!(status["today"], today());
    assert_eq!(status["timeZone"], "UTC");
    assert_eq!(status["todayTotals"]["requests"], 2);
    assert_eq!(status["firstDay"], today());
    assert_eq!(status["access"], "readWrite");
    assert!(status["readMs"].is_number());

    let days = json!({"first": today(), "last": today()});
    let overview = server.result("overview", &json!({"days": days}));
    assert_eq!(overview["totals"]["requests"], 2);
    assert_eq!(overview["byDay"][0]["date"], today());
    let models: Vec<&str> = overview["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["model"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(models, ["Opus 5.5", "Sonnet 5.5"]);
    assert_eq!(overview["projects"][0]["project"]["id"], "C--code-app");
    assert_eq!(overview["projects"][0]["project"]["name"], "app");

    let opus_only = server.result(
        "overview",
        &json!({"days": days, "filters": {"model": "claude-opus-5-5"}}),
    );
    assert_eq!(opus_only["totals"]["requests"], 1);

    let sessions = server.result(
        "sessions",
        &json!({"days": days, "search": "APP", "limit": 10}),
    );
    assert_eq!(sessions["total"], 1);
    let id = sessions["rows"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(id, "4f1c2a3b-0000-4000-8000-000000000001");

    let session = server.result("session", &json!({"id": id}));
    assert_eq!(session["totals"]["requests"], 2);
    assert_eq!(session["claudeCodeVersion"], "2.1.296");
    assert_eq!(session["cumulativeCost"].as_array().unwrap().len(), 2);

    let prices = server.result("prices", &json!({"models": ["claude-opus-5-5"]}));
    assert_eq!(prices["models"][0]["model"]["name"], "Opus 5.5");
    assert!(prices["models"][0]["input"].as_f64().unwrap() > 0.0);

    let health = server.result("health", &Value::Null);
    assert_eq!(health["folders"][0]["state"]["state"], "found");
    assert_eq!(health["folders"][0]["state"]["transcripts"], 1);
}

#[test]
fn says_when_it_reads_new_activity() {
    let mut server = Server::start();
    server.changed();

    server.transcript(&[request("3", "claude-opus-5-5", &today_at("00:00:03"))]);
    // The read may end before the answer to the request arrives, and the history watch may say the
    // history changed as well, without the days, so the test waits for the read's own word.
    server.ask("refresh", &Value::Null);
    let changed = server.wait_for(|message| {
        message["method"] == "history/changed" && !message["params"]["days"].is_null()
    });

    assert_eq!(changed["params"]["days"]["first"], today());
    let status = server.result("status", &Value::Null);
    assert_eq!(status["todayTotals"]["requests"], 3);
}

#[test]
fn says_when_a_read_ends_even_with_nothing_new() {
    let mut server = Server::start();
    server.changed();

    // The read may end before the answer to the request arrives, so the test waits for the end of
    // the read alone.
    server.ask("refresh", &Value::Null);
    let ended = server.wait_for(|message| {
        message["method"] == "ingest/progress"
            && message["params"]["done"] == message["params"]["total"]
    });

    assert_eq!(ended["params"]["total"], 1);
    assert_eq!(
        server.result("status", &Value::Null)["reading"],
        Value::Null
    );
}

#[test]
fn shows_the_plan_limits_a_status_line_records() {
    let mut server = Server::start();
    server.changed();
    let resets_at = NOW.parse::<jiff::Timestamp>().unwrap().as_second() + 3_600;
    let mut statusline = Command::new(env!("CARGO_BIN_EXE_claude-dashboard"))
        .arg("statusline")
        .env("CLAUDE_CONFIG_DIR", &server.root)
        .env("CLAUDE_DASHBOARD_HISTORY", &server.history)
        .env("CLAUDE_DASHBOARD_NOW", NOW)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    statusline
        .stdin
        .take()
        .unwrap()
        .write_all(
            json!({"model": {"id": "claude-opus-5-5", "display_name": "Opus"},
                   "rate_limits": {"five_hour": {"used_percentage": 42, "resets_at": resets_at}}})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    assert!(statusline.wait().unwrap().success());

    server.wait_for(|message| message["method"] == "limits/changed");
    let limits = server.result("limits", &Value::Null);

    let window = &limits["windows"][0];
    assert_eq!(window["reading"]["window"], "five_hour");
    assert_eq!(window["reading"]["usedPercentage"], 42.0);
    assert_eq!(window["reading"]["provenance"], "official");
    assert_eq!(window["reading"]["level"], "normal");
    assert_eq!(window["series"].as_array().unwrap().len(), 1);
    assert_eq!(limits["statusLine"]["state"], "notConnected");
    assert_eq!(limits["endpoint"]["enabled"], false);
}

#[test]
fn keeps_the_settings_every_surface_shares() {
    let mut server = Server::start();
    server.changed();

    let settings = server.result("settings", &Value::Null);
    assert_eq!(settings["timeZone"], Value::Null);
    assert_eq!(settings["systemTimeZone"], "UTC");
    assert_eq!(settings["weekStart"], "monday");

    server.result(
        "settings/set",
        &json!({"timeZone": "Pacific/Auckland", "weekStart": "Sunday"}),
    );
    let settings = server.result("settings", &Value::Null);
    assert_eq!(settings["timeZone"], "Pacific/Auckland");
    assert_eq!(settings["weekStart"], "sunday");
    let status = server.result("status", &Value::Null);
    assert_eq!(status["timeZone"], "Pacific/Auckland");
    assert_eq!(status["weekStart"], "sunday");

    let not_a_zone = server.request("settings/set", &json!({"timeZone": "Mars/Olympus"}));
    assert_eq!(not_a_zone["error"]["code"], -32602);
    server.result("settings/set", &json!({"timeZone": ""}));
    assert_eq!(
        server.result("settings", &Value::Null)["timeZone"],
        Value::Null
    );

    let added = server.root.with_file_name("other-claude");
    fs::create_dir_all(added.join("projects")).unwrap();
    let path = added.display().to_string();
    server.result("folders/add", &json!({"path": path}));
    let health = server.result("health", &Value::Null);
    let folders = health["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 2);
    assert_eq!(folders[1]["source"], "added");
    assert_eq!(folders[1]["path"], path);
    let relative = server.request("folders/add", &json!({"path": "somewhere"}));
    assert_eq!(relative["error"]["code"], -32602);
    server.result("folders/remove", &json!({"path": path}));
    let health = server.result("health", &Value::Null);
    assert_eq!(health["folders"].as_array().unwrap().len(), 1);
    assert_eq!(health["history"]["requests"], 2);
    assert_eq!(health["cleanupDays"], 30);
}

#[test]
fn changes_claude_codes_settings_only_as_shown() {
    let mut server = Server::start();
    let settings = server.root.join("settings.json");

    let plan = server.result("statusLine/plan", &json!({"connect": true}));
    assert_eq!(plan["kind"], "change");
    assert_eq!(plan["path"], settings.display().to_string());
    assert_eq!(plan["before"], Value::Null);
    assert!(plan["after"].as_str().unwrap().contains(" statusline"));
    assert_eq!(plan["exists"], false);

    let not_shown = server.request(
        "statusLine/apply",
        &json!({"connect": true, "before": null, "after": "\"type\": \"command\""}),
    );
    assert_eq!(not_shown["error"]["code"], -32602);
    assert!(
        !settings.exists(),
        "nothing changes without the change shown"
    );

    server.result(
        "statusLine/apply",
        &json!({"connect": true, "before": plan["before"], "after": plan["after"]}),
    );
    let saved: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    assert!(
        saved["statusLine"]["command"]
            .as_str()
            .unwrap()
            .ends_with(" statusline")
    );
    assert_eq!(
        server.result("limits", &Value::Null)["statusLine"]["state"],
        "connected"
    );
    assert_eq!(
        server.result("statusLine/plan", &json!({"connect": true}))["kind"],
        "nothing"
    );

    let plan = server.result("statusLine/plan", &json!({"connect": false}));
    assert_eq!(plan["kind"], "change");
    assert_eq!(plan["after"], Value::Null);
    server.result(
        "statusLine/apply",
        &json!({"connect": false, "before": plan["before"], "after": plan["after"]}),
    );
    let saved: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(saved.get("statusLine"), None);
    assert!(
        server
            .root
            .join("settings.claude-dashboard-backup.json")
            .is_file()
    );
}

/// Where `serve --status-line-copy` keeps its copy, for a history in `dir`.
fn copy_in(dir: &Path) -> PathBuf {
    dir.join("bin").join(if cfg!(windows) {
        "claude-dashboard.exe"
    } else {
        "claude-dashboard"
    })
}

/// The version a program reports, or `None` when it does not run.
fn version_of(program: &Path) -> Option<String> {
    let output = Command::new(program).arg("--version").output().ok()?;
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}

#[test]
fn connects_the_status_line_to_a_copy_that_updates_cannot_move() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(".claude");
    let history = dir.path().join("history.sqlite");
    let copy = copy_in(dir.path());
    fs::create_dir_all(&root).unwrap();
    let mut server = Server::launch(dir, root, history, &["--status-line-copy"]);

    let plan = server.result("statusLine/plan", &json!({"connect": true}));
    assert!(!copy.exists(), "nothing is copied until the change is made");
    server.result(
        "statusLine/apply",
        &json!({"connect": true, "before": plan["before"], "after": plan["after"]}),
    );

    let saved: Value =
        serde_json::from_str(&fs::read_to_string(server.root.join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        saved["statusLine"]["command"],
        format!(
            "{} statusline",
            copy.display().to_string().replace('\\', "/")
        )
    );
    assert_eq!(
        version_of(&copy).as_deref(),
        Some(concat!("claude-dashboard ", env!("CARGO_PKG_VERSION")))
    );
}

#[test]
fn brings_the_status_line_copy_up_to_date_when_it_starts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(".claude");
    let history = dir.path().join("history.sqlite");
    let copy = copy_in(dir.path());
    fs::create_dir_all(copy.parent().unwrap()).unwrap();
    fs::write(&copy, "not a program").unwrap();
    let _server = Server::launch(dir, root, history, &["--status-line-copy"]);

    let deadline = Instant::now() + WAIT;
    while version_of(&copy).is_none() {
        assert!(Instant::now() < deadline, "the copy is replaced in time");
        thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        version_of(&copy).as_deref(),
        Some(concat!("claude-dashboard ", env!("CARGO_PKG_VERSION")))
    );
}

/// The cost Claude Code writes when a process of the session ends, counting what `request` counts.
fn cost_state(cost_usd: f64, requests: u64) -> Value {
    json!({
        "type": "cost-state", "sessionId": "4f1c2a3b-0000-4000-8000-000000000001",
        "startTime": 1_791_000_000_000_i64, "timestamp": today_at("00:00:03"),
        "modelUsage": {"claude-opus-5-5": {"inputTokens": 3 * requests,
            "outputTokens": 640 * requests, "cacheReadInputTokens": 42_000 * requests,
            "cacheCreationInputTokens": 1_500 * requests, "costUSD": cost_usd}}
    })
}

#[test]
fn compares_a_session_with_claude_codes_figure_for_the_same_requests() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(".claude");
    let history = dir.path().join("history.sqlite");
    write_transcript(
        &root,
        &[
            request("1", "claude-opus-5-5", &today_at("00:00:01")),
            cost_state(0.05, 1),
        ],
    );
    let mut server = Server::start_with(dir, root, history);
    server.changed();
    let row = |server: &mut Server| {
        server.result(
            "sessions",
            &json!({"days": {"first": today(), "last": today()}, "filters": {}, "limit": 10, "offset": 0}),
        )["rows"][0]
            .clone()
    };

    let compared = row(&mut server);
    let ours = compared["totals"]["cost"]["total"].as_f64().unwrap();
    assert_eq!(compared["reportedUsd"], 0.05);
    let drift = compared["drift"].as_f64().unwrap();
    assert!((drift - (ours - 0.05) / 0.05).abs() < 1e-9, "{compared}");

    // The session goes on in a process that has not written its cost yet.
    server.transcript(&[request("2", "claude-opus-5-5", &today_at("00:00:04"))]);
    // The read may end before the answer to the request arrives.
    server.ask("refresh", &Value::Null);
    server.changed();
    let running = row(&mut server);
    assert_eq!(running["reportedUsd"], 0.05);
    assert_eq!(running["drift"], Value::Null, "{running}");
}

#[test]
fn turns_the_usage_endpoint_on_and_off() {
    let mut server = Server::start();

    let endpoint = &server.result("limits", &Value::Null)["endpoint"];
    assert_eq!(endpoint["enabled"], false);
    assert_eq!(endpoint["consent"].as_array().unwrap().len(), 5);

    server.result("endpoint/set", &json!({"enabled": true}));
    assert_eq!(
        server.result("limits", &Value::Null)["endpoint"]["enabled"],
        true
    );
    server.result("endpoint/set", &json!({"enabled": false}));
    assert_eq!(
        server.result("limits", &Value::Null)["endpoint"]["enabled"],
        false
    );
}

#[test]
fn offers_the_claude_usage_dashboards_history_once() {
    let mut server = Server::start();
    server.changed();
    assert_eq!(server.result("import/offer", &Value::Null), Value::Null);

    let database = rusqlite::Connection::open(server.root.join("usage.db")).unwrap();
    database
        .execute_batch(
            "CREATE TABLE turns (
                 id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, timestamp TEXT,
                 model TEXT, input_tokens INTEGER, output_tokens INTEGER,
                 cache_read_tokens INTEGER, cache_creation_tokens INTEGER, cwd TEXT,
                 message_id TEXT
             );
             INSERT INTO turns (session_id, timestamp, model, input_tokens, output_tokens,
                     cache_read_tokens, cache_creation_tokens, cwd, message_id)
                 VALUES ('old', '2026-08-01T10:00:00.000Z', 'claude-opus-5', 4, 50, 30000, 900,
                     '/code/old', 'msg_old');",
        )
        .unwrap();
    drop(database);

    let offer = server.result("import/offer", &Value::Null);
    assert_eq!(offer["requests"], 1);
    assert_eq!(offer["firstDay"], "2026-08-01");
    assert_eq!(offer["dismissed"], false);

    server.result("import/dismiss", &Value::Null);
    assert_eq!(
        server.result("import/offer", &Value::Null)["dismissed"],
        true
    );

    let done = server.result("import/run", &Value::Null);
    assert_eq!(done["imported"], 1);
    assert_eq!(server.result("import/offer", &Value::Null)["requests"], 0);
    assert_eq!(
        server.result("health", &Value::Null)["history"]["imported"]["requests"],
        1
    );
}

/// How many sessions, and requests in each, make a first read that writes several batches, so
/// that it can be stopped or watched midway.
const MANY: usize = 1_000;
const EACH: usize = 24;

/// Writes `MANY` sessions of `EACH` requests, today, into a new data root.
fn many_sessions() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(".claude");
    let history = dir.path().join("history.sqlite");
    let folder = root.join("projects").join("C--code-app");
    fs::create_dir_all(&folder).unwrap();
    for session in 0..MANY {
        let lines: Vec<String> = (0..EACH)
            .map(|n| {
                request(
                    &format!("{session}-{n}"),
                    "claude-opus-5-5",
                    &today_at(&format!(
                        "{:02}:{:02}:{n:02}",
                        session / 60 % 12,
                        session % 60
                    )),
                )
                .to_string()
            })
            .collect();
        fs::write(
            folder.join(format!("4f1c2a3b-0000-4000-8000-{session:012}.jsonl")),
            lines.join("\n") + "\n",
        )
        .unwrap();
    }
    (dir, root, history)
}

fn requests_held(history: &Path) -> i64 {
    rusqlite::Connection::open(history)
        .unwrap()
        .query_row("SELECT count(*) FROM requests", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn a_client_that_goes_away_during_a_read_stops_it_and_the_next_read_carries_on() {
    let (dir, root, history) = many_sessions();
    let mut server = Server::start_with(dir, root, history);
    server.wait_for(|message| {
        message["method"] == "ingest/progress"
            && message["params"]["done"].as_u64() < message["params"]["total"].as_u64()
    });

    drop(server.input.take());
    let deadline = Instant::now() + WAIT;
    while server.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "the server keeps reading");
        thread::sleep(Duration::from_millis(20));
    }
    assert!(server.child.try_wait().unwrap().unwrap().success());
    let kept = requests_held(&server.history);
    assert!(
        kept < i64::try_from(MANY * EACH).unwrap(),
        "the read stopped midway: {kept} requests"
    );

    let dir = std::mem::replace(&mut server.dir, tempfile::tempdir().unwrap());
    let mut next = Server::start_with(dir, server.root.clone(), server.history.clone());
    next.wait_for(|message| {
        message["method"] == "ingest/progress"
            && message["params"]["done"] == message["params"]["total"]
    });
    assert_eq!(
        next.result("status", &Value::Null)["todayTotals"]["requests"],
        MANY * EACH
    );
}

#[test]
fn says_the_history_changed_while_a_long_read_goes_on() {
    let (dir, root, history) = many_sessions();
    let server = Server::start_with(dir, root, history);

    let changed = server.wait_for(|message| {
        message["method"] == "history/changed"
            || (message["method"] == "ingest/progress"
                && message["params"]["done"] == message["params"]["total"])
    });

    assert_eq!(changed["method"], "history/changed", "{changed}");
    assert_eq!(changed["params"]["days"]["last"], today());
}

#[test]
fn answers_a_mistake_with_an_error() {
    let mut server = Server::start();

    let unknown = server.request("exit-now", &Value::Null);
    assert_eq!(unknown["error"]["code"], -32601);
    let bad_days = server.request(
        "overview",
        &json!({"days": {"first": "soon", "last": "later"}}),
    );
    assert_eq!(bad_days["error"]["code"], -32602);
    let no_session = server.request("session", &json!({"id": "missing"}));
    assert_eq!(no_session["error"]["code"], -32602);
}

#[test]
fn ends_when_its_input_closes() {
    let mut server = Server::start();
    server.result("status", &Value::Null);

    drop(server.input.take());

    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(status) = server.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "the server keeps running");
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn an_export_is_saved_where_the_extension_says_and_named_as_the_dashboard_suggests() {
    let mut server = Server::start();
    server.changed();
    let export = json!({
        "dataset": "requests",
        "format": "csv",
        "days": {"first": TODAY, "last": TODAY},
    });

    let name = server.result("export/name", &export);
    assert_eq!(
        name["name"],
        "claude-dashboard-history-2026-10-10-to-2026-10-10.csv"
    );
    let path = server.dir.path().join("saved.csv");
    let saved = server.result(
        "export/save",
        &json!({"export": export, "path": path.display().to_string()}),
    );
    assert_eq!(saved["rows"], 2);
    assert_eq!(saved["what"], "2 requests");
    let written = fs::read(&path).unwrap();
    assert!(
        written.starts_with(b"\xef\xbb\xbf"),
        "a saved file starts with a byte order mark"
    );

    let content = server.result("export/content", &export);
    assert_eq!(
        content["content"].as_str().unwrap().as_bytes(),
        written.as_slice()
    );
}

#[test]
fn an_export_names_its_file_by_its_whole_path() {
    let mut server = Server::start();
    let response = server.request(
        "export/save",
        &json!({
            "export": {"dataset": "days", "format": "json"},
            "path": "relative.json",
        }),
    );
    assert_eq!(response["error"]["code"], -32602, "{response}");
    assert!(!server.dir.path().join("relative.json").exists());
}

#[test]
fn the_server_keeps_answering_while_an_export_writes() {
    let mut server = Server::start();
    server.changed();
    let path = server.dir.path().join("history.csv");
    let export = server.ask(
        "export/save",
        &json!({
            "export": {"dataset": "requests", "format": "csv"},
            "path": path.display().to_string(),
        }),
    );
    let status = server.ask("status", &Value::Null);
    // The export answers from a thread of its own, so either answer can come first.
    let responses = server.responses(&[export, status]);
    assert_eq!(responses[&status]["result"]["today"], TODAY);
    let saved = &responses[&export];
    assert_eq!(saved["result"]["rows"], 2, "{saved}");
}
