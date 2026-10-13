//! Every surface on one history at once, as when the desktop app, VS Code, the terminal and Claude
//! Code's status line all run while Claude Code writes: two servers, report commands and status
//! line calls share the history while transcripts grow. The history stays whole, and holds exactly
//! what one read of the same transcripts gives.

mod common;

use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use common::{Fixture, NOW, request, today_at};
use serde_json::{Value, json};

/// The sessions Claude Code writes to at once, the rounds of writing, and the requests each session
/// gains in a round.
const SESSIONS: usize = 6;
const ROUNDS: usize = 12;
const PER_ROUND: usize = 5;

/// A `serve --stdio` server, as the desktop app and VS Code each run one, whose answers are read
/// and dropped so that it never waits to write them.
struct Server {
    child: Child,
    input: ChildStdin,
    next_id: u64,
}

impl Server {
    fn start(fixture: &Fixture) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_claude-dashboard"))
            .args(["serve", "--stdio"])
            .env("CLAUDE_CONFIG_DIR", fixture.root())
            .env("CLAUDE_DASHBOARD_HISTORY", fixture.history())
            .env("CLAUDE_DASHBOARD_NOW", NOW)
            .env("TZ", "UTC")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();
        thread::spawn(move || io::copy(&mut output, &mut io::sink()));
        Self {
            child,
            input,
            next_id: 0,
        }
    }

    /// Asks the server to read what is new, as a dashboard's refresh does.
    fn refresh(&mut self) {
        self.next_id += 1;
        let body =
            json!({"jsonrpc": "2.0", "id": self.next_id, "method": "refresh", "params": null})
                .to_string();
        write!(self.input, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.input.flush().unwrap();
    }

    /// Ends the server as a dashboard that closes does, by closing its input.
    fn stop(self) {
        let Self {
            mut child, input, ..
        } = self;
        drop(input);
        let deadline = Instant::now() + Duration::from_secs(30);
        while child.try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "the server ends once its input closes"
            );
            thread::sleep(Duration::from_millis(50));
        }
    }
}

/// The transcript of session `session`, in the project `C:\code\app`.
fn transcript(root: &Path, session: usize) -> PathBuf {
    root.join("projects")
        .join("C--code-app")
        .join(format!("4f1c2a3b-0000-4000-8000-{session:012}.jsonl"))
}

/// Claude Code writing to every session: each round adds requests to each transcript, and stops
/// midway through its last line, which the next round finishes, so that every read between rounds
/// meets a line still being written.
struct Writer {
    root: PathBuf,
    /// What each session's half-written line still lacks.
    unfinished: Vec<String>,
}

impl Writer {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            unfinished: vec![String::new(); SESSIONS],
        }
    }

    fn write_round(&mut self, round: usize) {
        const MODELS: [&str; 3] = ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"];
        for session in 0..SESSIONS {
            let path = transcript(&self.root, session);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
            let mut text = std::mem::take(&mut self.unfinished[session]);
            for at in 0..PER_ROUND {
                let second = round * PER_ROUND + at;
                let record = request(
                    &format!("{session}-{round}-{at}"),
                    MODELS[(session + at) % MODELS.len()],
                    &today_at(&format!(
                        "{:02}:{:02}:{:02}",
                        session,
                        second / 60,
                        second % 60
                    )),
                );
                writeln!(text, "{record}").unwrap();
            }
            if round + 1 < ROUNDS {
                self.unfinished[session] = text.split_off(text.len() - 40);
            }
            file.write_all(text.as_bytes()).unwrap();
        }
    }
}

/// A report's JSON, from the history at `history` after it has read what is new.
fn report(fixture: &Fixture, history: &Path, name: &str) -> Value {
    let output = fixture
        .cli(&[name, "--json"])
        .env("CLAUDE_DASHBOARD_HISTORY", history)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

/// A status line call that records how much of the five-hour window is used.
fn status_line(fixture: &Fixture, used: usize) {
    let reading = json!({
        "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
        "rate_limits": {"five_hour": {"used_percentage": used, "resets_at": 1_999_999_999}}
    });
    let output = fixture
        .cli(&["statusline"])
        .write_stdin(reading.to_string())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        String::from_utf8_lossy(&output).contains(&format!("5h {used}%")),
        "the status line shows the reading it was given"
    );
}

#[test]
fn every_surface_shares_one_history_while_transcripts_grow() {
    let fixture = Fixture::new();
    let mut writer = Writer::new(fixture.root());
    writer.write_round(0);
    let mut servers = [Server::start(&fixture), Server::start(&fixture)];

    thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..ROUNDS {
                report(&fixture, &fixture.history(), "daily");
            }
        });
        scope.spawn(|| {
            for round in 0..ROUNDS {
                status_line(&fixture, 40 + round);
            }
        });
        for round in 1..ROUNDS {
            writer.write_round(round);
            for server in &mut servers {
                server.refresh();
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
    for server in servers {
        server.stop();
    }

    let shared = report(&fixture, &fixture.history(), "daily");
    let alone_history = fixture.dir.path().join("read-alone.sqlite");
    let alone = report(&fixture, &alone_history, "daily");
    assert_eq!(
        shared, alone,
        "the shared history holds what one read gives"
    );
    assert_eq!(
        shared[0]["requests"],
        SESSIONS * ROUNDS * PER_ROUND,
        "every request, once"
    );
    assert_eq!(
        report(&fixture, &fixture.history(), "sessions"),
        report(&fixture, &alone_history, "sessions"),
    );

    let history = rusqlite::Connection::open(fixture.history()).unwrap();
    let integrity: String = history
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    // Every call comes at the same moment, so the history keeps one reading of the window.
    let readings: Vec<f64> = history
        .prepare("SELECT used_percentage FROM limit_snapshots WHERE window = 'five_hour'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(readings.len(), 1, "{readings:?}");
    let sent = (0..ROUNDS).map(|round| f64::from(u32::try_from(40 + round).unwrap()));
    assert!(
        sent.into_iter()
            .any(|used| (used - readings[0]).abs() < f64::EPSILON),
        "one of the readings sent: {readings:?}"
    );
}
