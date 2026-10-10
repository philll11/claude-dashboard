//! `claude-dashboard serve --stdio`, spoken to as the VS Code extension speaks to it: JSON-RPC
//! with `Content-Length` framing over its standard input and output.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(20);

struct Server {
    _dir: tempfile::TempDir,
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
        let mut child = Command::new(env!("CARGO_BIN_EXE_claude-dashboard"))
            .args(["serve", "--stdio"])
            .env("CLAUDE_CONFIG_DIR", &root)
            .env("CLAUDE_DASHBOARD_HISTORY", &history)
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
            _dir: dir,
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

    /// Sends a request and returns its response, keeping the notifications that arrive first.
    fn request(&mut self, method: &str, params: &Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
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
    jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .date()
        .to_string()
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
    server.result("refresh", &Value::Null);
    let changed = server.changed();

    assert_eq!(changed["params"]["days"]["first"], today());
    let status = server.result("status", &Value::Null);
    assert_eq!(status["todayTotals"]["requests"], 3);
}

#[test]
fn shows_the_plan_limits_a_status_line_records() {
    let mut server = Server::start();
    server.changed();
    let resets_at = jiff::Timestamp::now().as_second() + 3_600;
    let mut statusline = Command::new(env!("CARGO_BIN_EXE_claude-dashboard"))
        .arg("statusline")
        .env("CLAUDE_CONFIG_DIR", &server.root)
        .env("CLAUDE_DASHBOARD_HISTORY", &server.history)
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
