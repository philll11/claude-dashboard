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
