//! Several processes share one history, as the terminal, the desktop app, VS Code and status line
//! calls do.
//!
//! Each test starts copies of this test binary that run the ignored `helper` test, with the
//! helper's role in an environment variable.

use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use claude_dashboard_core::history::{Attribution, History, Identity, Request};
use claude_dashboard_core::tokens::Tokens;
use rusqlite::{Connection, OpenFlags};

const ROLE: &str = "CLAUDE_DASHBOARD_TEST_ROLE";
const FOLDER: &str = "CLAUDE_DASHBOARD_TEST_FOLDER";
const HELPER_DEADLINE: Duration = Duration::from_secs(120);

const SESSIONS: usize = 6;
const BASE_TIMESTAMP_MS: i64 = 1_791_590_400_000;

/// One version of a request as transcripts hold it: a streamed response leaves several versions,
/// and copies of a request turn up in other sessions. Only the output tokens vary, so they decide
/// which version has the most tokens.
#[derive(Debug, Clone)]
struct Version {
    session: usize,
    message_id: Option<String>,
    request_id: Option<String>,
    record_id: String,
    timestamp_ms: i64,
    output: u32,
}

fn workload() -> Vec<Version> {
    let mut versions = Vec::new();
    let mut timestamp_ms = BASE_TIMESTAMP_MS;
    let mut tick = || {
        timestamp_ms += 1_000;
        timestamp_ms
    };
    for n in 0..120 {
        let session = n % SESSIONS;
        let mut last = None;
        for (step, output) in [10, 20, 30].into_iter().enumerate() {
            let version = Version {
                session,
                message_id: Some(format!("msg_{n}")),
                request_id: Some(format!("req_{n}")),
                record_id: format!("rec-{n}-{step}"),
                timestamp_ms: tick(),
                output: output + u32::try_from(n).unwrap(),
            };
            versions.push(version.clone());
            last = Some(version);
        }
        if n % 5 == 0 {
            let original = last.unwrap();
            versions.push(Version {
                session: (session + 1) % SESSIONS,
                record_id: format!("copy-{n}"),
                ..original
            });
        }
    }
    for n in 0..40 {
        for session in [n % SESSIONS, (n + 3) % SESSIONS] {
            for (step, output) in [5, 15].into_iter().enumerate() {
                versions.push(Version {
                    session,
                    message_id: Some(format!("msg_m{n}")),
                    request_id: None,
                    record_id: format!("rec-m{n}-{session}-{step}"),
                    timestamp_ms: tick(),
                    output,
                });
            }
        }
    }
    for n in 0..20 {
        versions.push(Version {
            session: n % SESSIONS,
            message_id: None,
            request_id: None,
            record_id: format!("solo-{n}"),
            timestamp_ms: tick(),
            output: 7,
        });
    }
    versions
}

fn session_id(session: usize) -> String {
    format!("session-{session}")
}

/// Every request the workload holds once, as (session id, record id, output tokens), worked out
/// without the history.
fn expected() -> BTreeSet<(String, String, u32)> {
    let mut winners: HashMap<String, Version> = HashMap::new();
    for version in workload() {
        let identity = match (&version.message_id, &version.request_id) {
            (Some(message_id), Some(request_id)) => format!("{message_id} {request_id}"),
            (Some(message_id), None) => format!("{} {message_id}", version.session),
            (None, _) => version.record_id.clone(),
        };
        let rank = |v: &Version| (v.output, v.timestamp_ms, v.record_id.clone());
        match winners.get_mut(&identity) {
            Some(winner) if rank(&version) > rank(winner) => *winner = version,
            Some(_) => {}
            None => {
                winners.insert(identity, version);
            }
        }
    }
    winners
        .into_values()
        .map(|v| (session_id(v.session), v.record_id, v.output))
        .collect()
}

/// What the history holds, in the form [`expected`] gives, read through a separate connection.
fn recorded(path: &Path) -> BTreeSet<(String, String, u32)> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    conn.prepare(
        "SELECT sessions.session_id, requests.record_id, requests.output_tokens
         FROM requests JOIN sessions ON sessions.id = requests.session",
    )
    .unwrap()
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

fn spawn(role: &str, folder: &Path) -> Child {
    Command::new(env::current_exe().unwrap())
        .args(["helper", "--exact", "--ignored", "--nocapture"])
        .env(ROLE, role)
        .env(FOLDER, folder)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn assert_succeeded(outputs: &[Output]) {
    for output in outputs {
        assert!(
            output.status.success(),
            "a helper process failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn writers_and_readers_share_the_history_without_double_counting() {
    let dir = tempfile::tempdir().unwrap();
    let readers: Vec<Child> = (0..2)
        .map(|index| spawn(&format!("reader:{index}"), dir.path()))
        .collect();
    let writers: Vec<Child> = (0..4)
        .map(|index| spawn(&format!("writer:{index}"), dir.path()))
        .collect();

    let mut outputs: Vec<Output> = writers
        .into_iter()
        .map(|writer| writer.wait_with_output().unwrap())
        .collect();
    fs::write(dir.path().join("stop"), "").unwrap();
    outputs.extend(
        readers
            .into_iter()
            .map(|reader| reader.wait_with_output().unwrap()),
    );
    assert_succeeded(&outputs);

    let path = dir.path().join("history.sqlite");
    let history = History::open(&path).unwrap();
    assert_eq!(history.integrity_check().unwrap(), Vec::<String>::new());
    assert_eq!(recorded(&path), expected());
}

#[test]
fn processes_that_create_the_history_at_once_all_open_it() {
    for _ in 0..10 {
        let dir = tempfile::tempdir().unwrap();
        let openers: Vec<Child> = (0..6)
            .map(|index| spawn(&format!("opener:{index}"), dir.path()))
            .collect();

        let deadline = Instant::now() + HELPER_DEADLINE;
        while (0..6).any(|index| !dir.path().join(format!("ready-{index}")).exists()) {
            assert!(Instant::now() < deadline, "the openers did not start");
            thread::sleep(Duration::from_millis(1));
        }
        fs::write(dir.path().join("go"), "").unwrap();

        let outputs: Vec<Output> = openers
            .into_iter()
            .map(|opener| opener.wait_with_output().unwrap())
            .collect();
        assert_succeeded(&outputs);
    }
}

#[test]
fn the_ingest_lock_admits_one_process_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("holder"), "nobody").unwrap();

    let lockers: Vec<Child> = (0..4)
        .map(|index| spawn(&format!("locker:{index}"), dir.path()))
        .collect();

    let outputs: Vec<Output> = lockers
        .into_iter()
        .map(|locker| locker.wait_with_output().unwrap())
        .collect();
    assert_succeeded(&outputs);
}

#[test]
#[ignore = "a helper process that the other tests in this file start"]
fn helper() {
    let (Ok(role), Some(folder)) = (env::var(ROLE), env::var_os(FOLDER)) else {
        return;
    };
    let folder = PathBuf::from(folder);
    let (kind, index) = role.split_once(':').unwrap();
    let index: u64 = index.parse().unwrap();
    match kind {
        "writer" => write_the_workload(&folder, index),
        "reader" => read_until_stopped(&folder),
        "locker" => take_turns_with_the_ingest_lock(&folder, index),
        "opener" => open_on_signal(&folder, index),
        _ => panic!("unknown role {role}"),
    }
}

/// Waits until every opener is ready and the test says go, then opens the history and records a
/// session in it, so that the openers create the history at the same moment.
fn open_on_signal(folder: &Path, index: u64) {
    fs::write(folder.join(format!("ready-{index}")), "").unwrap();
    let deadline = Instant::now() + HELPER_DEADLINE;
    while !folder.join("go").exists() {
        assert!(Instant::now() < deadline, "never told to go");
        thread::yield_now();
    }

    let mut history = History::open(&folder.join("history.sqlite")).unwrap();
    history
        .write(|w| {
            let project = w.project("project", None)?;
            w.session(&format!("session-{index}"), project)?;
            Ok(())
        })
        .unwrap();
}

/// Records every version of the workload in an order and in batches of its own, without taking
/// the ingest lock, so that writes from several processes interleave.
fn write_the_workload(folder: &Path, index: u64) {
    let mut history = History::open(&folder.join("history.sqlite")).unwrap();
    let mut random = Random::new(index);
    let mut versions = workload();
    random.shuffle(&mut versions);

    let mut rest = versions.as_slice();
    while !rest.is_empty() {
        let (batch, tail) = rest.split_at(rest.len().min(1 + random.below(16)));
        history
            .write(|w| {
                for version in batch {
                    let project = w.project(&format!("project-{}", version.session / 3), None)?;
                    let session = w.session(&session_id(version.session), project)?;
                    w.request(&Request {
                        session,
                        identity: Identity::from_ids(
                            version.message_id.as_deref(),
                            version.request_id.as_deref(),
                        ),
                        record_id: &version.record_id,
                        subagent_id: None,
                        timestamp_ms: version.timestamp_ms,
                        model: "claude-opus-5-5",
                        tokens: Tokens {
                            input: 3,
                            output: u64::from(version.output),
                            cache_read: 2_000,
                            cache_write_5m: 0,
                            cache_write_1h: 100,
                        },
                        web_search_requests: 0,
                        web_fetch_requests: 0,
                        speed: None,
                        service_tier: None,
                        inference_geo: None,
                        attribution: Attribution::default(),
                        entrypoint: None,
                        claude_code_version: None,
                    })?;
                }
                Ok(())
            })
            .unwrap();
        rest = tail;
    }
}

/// Reads while the writers write, and checks that no read ever sees a request counted twice.
fn read_until_stopped(folder: &Path) {
    let path = folder.join("history.sqlite");
    let expected = expected();
    let most_requests = u64::try_from(expected.len()).unwrap();
    let most_output: u64 = expected
        .iter()
        .map(|(_, _, output)| u64::from(*output))
        .sum();
    let deadline = Instant::now() + HELPER_DEADLINE;

    let mut history = History::open(&path).unwrap();
    for reads in 1.. {
        let stopped = folder.join("stop").exists();
        assert!(Instant::now() < deadline, "the writers did not finish");
        let counts = history.counts().unwrap();
        assert!(counts.requests <= most_requests, "{counts:?}");
        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        conn.busy_timeout(HELPER_DEADLINE).unwrap();
        let output: u64 = conn
            .query_row(
                "SELECT coalesce(sum(output_tokens), 0) FROM requests",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(output <= most_output, "{output} output tokens");
        if stopped {
            break;
        }
        if reads % 20 == 0 {
            history = History::open(&path).unwrap();
        }
        thread::sleep(Duration::from_millis(1));
    }
}

/// Takes the ingest lock five times, and checks each time that no other process holds it.
fn take_turns_with_the_ingest_lock(folder: &Path, index: u64) {
    let history = History::open(&folder.join("history.sqlite")).unwrap();
    let holder = folder.join("holder");
    let me = index.to_string();
    let deadline = Instant::now() + HELPER_DEADLINE;

    let mut turns = 0;
    while turns < 5 {
        assert!(Instant::now() < deadline, "never got the ingest lock");
        let Some(lock) = history.try_lock_ingest().unwrap() else {
            thread::sleep(Duration::from_millis(1));
            continue;
        };
        assert_eq!(fs::read_to_string(&holder).unwrap(), "nobody");
        fs::write(&holder, &me).unwrap();
        thread::sleep(Duration::from_millis(5));
        assert_eq!(fs::read_to_string(&holder).unwrap(), me);
        fs::write(&holder, "nobody").unwrap();
        drop(lock);
        turns += 1;
        thread::sleep(Duration::from_millis(1));
    }
}

/// A small deterministic random number generator (xorshift), so each writer has its own order.
struct Random(u64);

impl Random {
    fn new(seed: u64) -> Self {
        Self(0x9E37_79B9_7F4A_7C15 ^ (seed + 1))
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number from 0 up to, but not including, `bound`.
    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound).unwrap()).unwrap()
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}
