use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use claude_dashboard_core::history::History;
use claude_dashboard_core::ingest::{self, Report};
use proptest::prelude::*;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};

const SESSION: &str = "4f1c2a3b-0000-4000-8000-000000000001";
const RESUMED: &str = "9a8b7c6d-0000-4000-8000-000000000002";

/// An assistant record shaped like Claude Code's, with made-up content.
fn assistant(record_id: &str, message: &str, request: &str, output: u64) -> Value {
    json!({
        "type": "assistant",
        "uuid": record_id,
        "requestId": request,
        "timestamp": "2026-10-10T06:16:27.123Z",
        "sessionId": SESSION,
        "cwd": "C:\\code\\app",
        "entrypoint": "cli",
        "version": "2.1.296",
        "message": {
            "id": message,
            "model": "claude-opus-5-5",
            "role": "assistant",
            "content": [{"type": "text", "text": "Made-up reply."}],
            "usage": {
                "input_tokens": 3,
                "output_tokens": output,
                "cache_read_input_tokens": 42_000,
                "cache_creation_input_tokens": 1_500,
                "cache_creation": {"ephemeral_5m_input_tokens": 500, "ephemeral_1h_input_tokens": 1_000},
                "server_tool_use": {"web_search_requests": 1, "web_fetch_requests": 0}
            }
        }
    })
}

fn lines(records: &[Value]) -> Vec<u8> {
    records
        .iter()
        .flat_map(|record| {
            let mut line = serde_json::to_vec(record).unwrap();
            line.push(b'\n');
            line
        })
        .collect()
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn append(path: &Path, bytes: &[u8]) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    history: History,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".claude");
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();
        Self {
            _dir: dir,
            root,
            history,
        }
    }

    fn project(&self) -> PathBuf {
        self.root.join("projects").join("C--code-app")
    }

    fn ingest(&mut self) -> Report {
        ingest::ingest(&mut self.history, std::slice::from_ref(&self.root))
            .unwrap()
            .expect("no other process holds the ingest lock")
    }

    fn query<T: rusqlite::types::FromSql>(&self, sql: &str) -> Vec<T> {
        let conn =
            Connection::open_with_flags(self.history.path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        conn.prepare(sql)
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// Every request as one line of text, in a stable order, for comparing two histories.
    fn requests(&self) -> Vec<String> {
        self.query(
            "SELECT sessions.session_id || ' ' || coalesce(message_id, '-') || ' '
                 || coalesce(request_id, '-') || ' ' || record_id || ' '
                 || coalesce(subagent_id, '-') || ' ' || model || ' ' || output_tokens || ' '
                 || cache_read_tokens || ' ' || web_search_requests
             FROM requests JOIN sessions ON sessions.id = requests.session
             ORDER BY 1",
        )
    }
}

#[test]
fn ingests_a_session_its_subagents_and_its_titles() {
    let mut fixture = Fixture::new();
    let session = fixture.project();
    write(
        &session.join(format!("{SESSION}.jsonl")),
        &lines(&[
            json!({"type": "user", "message": {"role": "user", "content": "Made-up prompt."}}),
            assistant("rec-1", "msg_1", "req_1", 10),
            json!({"type": "ai-title", "aiTitle": "First title", "sessionId": SESSION}),
            assistant("rec-2", "msg_2", "req_2", 20),
            json!({"type": "ai-title", "aiTitle": "Better title", "sessionId": SESSION}),
            json!({"type": "custom-title", "customTitle": "Release prep", "sessionId": SESSION}),
        ]),
    );
    write(
        &session
            .join(SESSION)
            .join("subagents")
            .join("agent-a1.jsonl"),
        &lines(&[assistant("rec-3", "msg_3", "req_3", 30)]),
    );
    write(
        &session
            .join(SESSION)
            .join("subagents")
            .join("workflows")
            .join("wf_9")
            .join("agent-b2.jsonl"),
        &lines(&[assistant("rec-4", "msg_4", "req_4", 40)]),
    );

    let report = fixture.ingest();

    assert_eq!(report.transcripts_read, 3);
    assert_eq!(report.request_records, 4);
    assert_eq!(report.failed, []);
    assert_eq!(
        fixture.requests(),
        [
            format!("{SESSION} msg_1 req_1 rec-1 - claude-opus-5-5 10 42000 1"),
            format!("{SESSION} msg_2 req_2 rec-2 - claude-opus-5-5 20 42000 1"),
            format!("{SESSION} msg_3 req_3 rec-3 a1 claude-opus-5-5 30 42000 1"),
            format!("{SESSION} msg_4 req_4 rec-4 b2 claude-opus-5-5 40 42000 1"),
        ]
    );
    assert_eq!(
        fixture.query::<String>("SELECT ai_title || ' / ' || custom_title FROM sessions"),
        ["Better title / Release prep"]
    );
    assert_eq!(
        fixture.query::<String>("SELECT transcript_folder || ' ' || path FROM projects"),
        [r"C--code-app C:\code\app"]
    );
}

#[test]
fn a_second_ingest_reads_only_what_was_added() {
    let mut fixture = Fixture::new();
    let main = fixture.project().join(format!("{SESSION}.jsonl"));
    let other = fixture.project().join(format!("{RESUMED}.jsonl"));
    write(&main, &lines(&[assistant("rec-1", "msg_1", "req_1", 10)]));
    write(&other, &lines(&[assistant("rec-9", "msg_9", "req_9", 90)]));
    fixture.ingest();

    append(&main, &lines(&[assistant("rec-2", "msg_2", "req_2", 20)]));
    let report = fixture.ingest();

    assert_eq!(report.transcripts_read, 1);
    assert_eq!(report.transcripts_unchanged, 1);
    assert_eq!(report.request_records, 1);
    assert_eq!(fixture.requests().len(), 3);

    let again = fixture.ingest();
    assert_eq!(
        (again.transcripts_read, again.transcripts_unchanged),
        (0, 2)
    );
}

#[test]
fn streamed_records_and_a_resumed_sessions_copies_count_once() {
    let mut fixture = Fixture::new();
    write(
        &fixture.project().join(format!("{SESSION}.jsonl")),
        &lines(&[
            assistant("rec-1a", "msg_1", "req_1", 5),
            assistant("rec-1b", "msg_1", "req_1", 50),
            assistant("rec-1c", "msg_1", "req_1", 50),
        ]),
    );
    let mut copy = assistant("rec-1c", "msg_1", "req_1", 50);
    copy["sessionId"] = json!(RESUMED);
    write(
        &fixture.project().join(format!("{RESUMED}.jsonl")),
        &lines(&[copy]),
    );

    fixture.ingest();

    let requests = fixture.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(
        requests[0].contains(" rec-1c - claude-opus-5-5 50 "),
        "{requests:?}"
    );
}

#[test]
fn an_attempt_on_another_model_before_a_fallback_is_a_request_of_its_own() {
    let mut fixture = Fixture::new();
    let mut record = assistant("rec-1", "msg_1", "req_1", 5_978);
    record["message"]["model"] = json!("claude-opus-4-8");
    record["message"]["usage"]["iterations"] = json!([
        {"type": "message", "model": "claude-fable-5-1", "input_tokens": 32, "output_tokens": 484,
         "cache_read_input_tokens": 424_503, "cache_creation_input_tokens": 0},
        {"type": "fallback_message", "model": "claude-opus-4-8", "input_tokens": 3, "output_tokens": 5_978,
         "cache_read_input_tokens": 42_000, "cache_creation_input_tokens": 1_500}
    ]);
    write(
        &fixture.project().join(format!("{SESSION}.jsonl")),
        &lines(&[record.clone(), record]),
    );

    fixture.ingest();

    assert_eq!(
        fixture.requests(),
        [
            format!("{SESSION} msg_1 req_1 rec-1 - claude-opus-4-8 5978 42000 1"),
            format!(
                "{SESSION} msg_1 req_1@claude-fable-5-1 rec-1@claude-fable-5-1 - claude-fable-5-1 484 424503 0"
            ),
        ]
    );
}

#[test]
fn a_half_written_line_is_ingested_once_claude_code_finishes_it() {
    let mut fixture = Fixture::new();
    let path = fixture.project().join(format!("{SESSION}.jsonl"));
    let bytes = lines(&[
        assistant("rec-1", "msg_1", "req_1", 10),
        assistant("rec-2", "msg_2", "req_2", 20),
    ]);
    let cut = bytes.len() - 7;
    write(&path, &bytes[..cut]);

    fixture.ingest();
    assert_eq!(fixture.requests().len(), 1);

    append(&path, &bytes[cut..]);
    fixture.ingest();
    assert_eq!(fixture.requests().len(), 2);
}

#[test]
fn a_shorter_transcript_is_read_again_from_the_start() {
    let mut fixture = Fixture::new();
    let path = fixture.project().join(format!("{SESSION}.jsonl"));
    write(
        &path,
        &lines(&[
            assistant("rec-1", "msg_1", "req_1", 10),
            assistant("rec-2", "msg_2", "req_2", 20),
        ]),
    );
    fixture.ingest();

    write(&path, &lines(&[assistant("rec-3", "msg_3", "req_3", 30)]));
    fixture.ingest();

    assert_eq!(fixture.requests().len(), 3);
}

#[test]
fn only_one_process_ingests_at_a_time() {
    let mut fixture = Fixture::new();
    write(
        &fixture.project().join(format!("{SESSION}.jsonl")),
        &lines(&[assistant("rec-1", "msg_1", "req_1", 10)]),
    );
    let other = History::open(fixture.history.path()).unwrap();
    let lock = other.try_lock_ingest().unwrap();

    let skipped =
        ingest::ingest(&mut fixture.history, std::slice::from_ref(&fixture.root)).unwrap();

    assert_eq!(skipped, None);
    assert_eq!(fixture.requests().len(), 0);
    drop(lock);
    assert_eq!(fixture.ingest().request_records, 1);
}

#[test]
fn missing_data_roots_hold_nothing_to_ingest() {
    let mut fixture = Fixture::new();
    let report = fixture.ingest();
    assert_eq!(report, Report::default());
}

/// A transcript of five requests, three streamed in several records, with lines the history
/// ignores in between.
fn growing_transcript() -> Vec<u8> {
    let mut records = Vec::new();
    for n in 0..5 {
        records.push(
            json!({"type": "user", "message": {"role": "user", "content": "Made-up prompt."}}),
        );
        for (step, output) in
            [10, 20, 30]
                .into_iter()
                .enumerate()
                .take(if n % 2 == 0 { 3 } else { 1 })
        {
            records.push(assistant(
                &format!("rec-{n}-{step}"),
                &format!("msg_{n}"),
                &format!("req_{n}"),
                output + n,
            ));
        }
    }
    records.push(json!({"type": "ai-title", "aiTitle": "Made-up title"}));
    lines(&records)
}

proptest! {
    // Each case builds two histories on disk.
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn ingesting_as_a_transcript_grows_gives_the_history_one_ingest_gives(
        mut cuts in proptest::collection::vec(0..=growing_transcript().len(), 0..8),
    ) {
        let bytes = growing_transcript();
        cuts.push(bytes.len());
        cuts.sort_unstable();

        let mut growing = Fixture::new();
        let path = growing.project().join(format!("{SESSION}.jsonl"));
        write(&path, b"");
        let mut written = 0;
        for cut in cuts {
            append(&path, &bytes[written..cut]);
            written = cut;
            growing.ingest();
        }

        let mut whole = Fixture::new();
        write(&whole.project().join(format!("{SESSION}.jsonl")), &bytes);
        whole.ingest();

        prop_assert_eq!(growing.requests(), whole.requests());
        prop_assert_eq!(growing.requests().len(), 5);
    }
}
