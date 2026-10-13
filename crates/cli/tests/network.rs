//! Reports, exports and the status line never fetch anything; only `prices update` reaches for the
//! network, and then only claude-dashboard's repository, through the proxy the environment names.

mod common;

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::json;

use common::{Fixture, request, today_at};

/// How long the proxy listens at most, so that a test can never wait on it for ever.
const LISTENS_FOR: Duration = Duration::from_secs(120);

/// How long the proxy waits for a connection to say something.
const WAITS_FOR_A_LINE: Duration = Duration::from_secs(5);

/// How long a command may run.
const RUNS_FOR: Duration = Duration::from_secs(60);

/// What the proxy notes for a connection that says nothing.
const SILENCE: &str = "(no request line)";

/// A stand-in for a proxy that notes the first line each connection sends and hangs up.
struct Proxy {
    port: u16,
    lines: Arc<Mutex<Vec<String>>>,
    done: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Proxy {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let thread = {
            let (lines, done) = (Arc::clone(&lines), Arc::clone(&done));
            thread::spawn(move || {
                let deadline = Instant::now() + LISTENS_FOR;
                while Instant::now() < deadline {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let line = first_line(&stream);
                            lines.lock().unwrap().push(line);
                        }
                        Err(error) if error.kind() == ErrorKind::WouldBlock => {
                            if done.load(Ordering::SeqCst) {
                                return;
                            }
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => return,
                    }
                }
            })
        };
        Self {
            port,
            lines,
            done,
            thread: Some(thread),
        }
    }

    /// Where a client finds it, such as `http://127.0.0.1:50000`.
    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The first lines noted so far, once every connection made before this call has been noted.
    fn lines(&self) -> Vec<String> {
        let mut marker = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        marker.write_all(b"MARKER\r\n").unwrap();
        let deadline = Instant::now() + WAITS_FOR_A_LINE * 2;
        loop {
            {
                let mut lines = self.lines.lock().unwrap();
                if let Some(at) = lines.iter().position(|line| line == "MARKER") {
                    lines.remove(at);
                    return lines.clone();
                }
            }
            assert!(
                Instant::now() < deadline,
                "the proxy did not note the marker"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The first line a connection sends, or [`SILENCE`] when it sends none.
fn first_line(stream: &TcpStream) -> String {
    // A connection the client has already closed may refuse these, which must not lose it.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(WAITS_FOR_A_LINE));
    let mut line = String::new();
    match BufReader::new(stream).read_line(&mut line) {
        Ok(_) if !line.trim_end().is_empty() => line.trim_end().to_owned(),
        _ => SILENCE.to_owned(),
    }
}

/// The input Claude Code gives a status line.
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
fn reports_never_fetch_and_prices_update_asks_the_repository_through_the_proxy() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );
    let proxy = Proxy::start();
    let through_the_proxy = |args: &[&str]| -> Command {
        let mut command = fixture.cli(args);
        command
            .env("HTTPS_PROXY", proxy.url())
            .env("https_proxy", proxy.url())
            .env_remove("NO_PROXY")
            .env_remove("no_proxy")
            .env_remove("ALL_PROXY")
            .env_remove("all_proxy")
            .timeout(RUNS_FOR);
        command
    };

    through_the_proxy(&["prices", "refresh", "on"])
        .assert()
        .success()
        .stdout(contains("The daily check for a newer price table is on."));
    let offline: &[&[&str]] = &[
        &["prices"],
        &["today"],
        &["daily", "--json"],
        &["weekly", "--csv"],
        &["monthly", "--json"],
        &["models", "--csv"],
        &["sessions", "--json"],
        &["projects", "--csv"],
        &["limits"],
        &["export", "--csv"],
        &["export", "--json"],
        &["doctor"],
    ];
    for args in offline {
        through_the_proxy(args).assert().success();
    }
    through_the_proxy(&["statusline"])
        .write_stdin(status_line_input(&fixture))
        .assert()
        .success();
    assert_eq!(
        proxy.lines(),
        Vec::<String>::new(),
        "something fetched although it should not"
    );

    through_the_proxy(&["prices", "update"])
        .assert()
        .failure()
        .stderr(contains("Cannot reach claude-dashboard's repository"));
    let lines = proxy.lines();
    assert!(!lines.is_empty(), "prices update did not use the proxy");
    for line in &lines {
        assert!(
            line.starts_with("CONNECT raw.githubusercontent.com:443"),
            "{line}"
        );
    }
}
