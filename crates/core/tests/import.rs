use std::fs;
use std::path::{Path, PathBuf};

use claude_dashboard_core::history::History;
use claude_dashboard_core::import::{self, CLAUDE_USAGE, ImportError, ImportReport};
use claude_dashboard_core::ingest;
use rusqlite::{Connection, OpenFlags, params};
use serde_json::json;

const SESSION: &str = "4f1c2a3b-0000-4000-8000-000000000001";
const OLD_SESSION: &str = "1d2e3f40-0000-4000-8000-000000000003";
const API_SESSION: &str = "7a6b5c4d-0000-4000-8000-000000000004";
const NOW_MS: i64 = 1_791_612_987_123;

/// A claude-usage database with its real schema.
fn claude_usage(path: &Path, turns: &[(&str, &str, &str, i64, Option<&str>)]) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (
             session_id TEXT PRIMARY KEY, project_name TEXT, first_timestamp TEXT,
             last_timestamp TEXT, git_branch TEXT, total_input_tokens INTEGER DEFAULT 0,
             total_output_tokens INTEGER DEFAULT 0, total_cache_read INTEGER DEFAULT 0,
             total_cache_creation INTEGER DEFAULT 0, model TEXT, turn_count INTEGER DEFAULT 0,
             topic TEXT
         );
         CREATE TABLE turns (
             id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, timestamp TEXT, model TEXT,
             input_tokens INTEGER DEFAULT 0, output_tokens INTEGER DEFAULT 0,
             cache_read_tokens INTEGER DEFAULT 0, cache_creation_tokens INTEGER DEFAULT 0,
             tool_name TEXT, cwd TEXT, message_id TEXT, is_subagent INTEGER DEFAULT 0,
             agent_id TEXT
         );
         INSERT INTO sessions (session_id, project_name, topic)
             VALUES ('1d2e3f40-0000-4000-8000-000000000003', 'app', 'Old importer work');",
    )
    .unwrap();
    for (session, message, cwd, output, agent) in turns {
        conn.execute(
            "INSERT INTO turns (session_id, timestamp, model, input_tokens, output_tokens,
                 cache_read_tokens, cache_creation_tokens, cwd, message_id, is_subagent, agent_id)
             VALUES (?1, '2026-08-01T10:00:00.000Z', 'claude-opus-5', 4, ?2, 30000, 900, ?3, ?4,
                 ?5, ?6)",
            params![session, output, cwd, message, agent.is_some(), agent],
        )
        .unwrap();
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    root: PathBuf,
    history: History,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".claude");
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();
        Self { dir, root, history }
    }

    fn transcript(&self, message: &str, request: &str, output: u64) {
        let record = json!({
            "type": "assistant", "uuid": format!("rec-{message}"), "requestId": request,
            "timestamp": "2026-08-01T10:00:00.000Z", "sessionId": SESSION, "cwd": r"C:\code\app",
            "message": {"id": message, "model": "claude-opus-5", "content": [],
                "usage": {"input_tokens": 4, "output_tokens": output, "cache_read_input_tokens": 30000,
                    "cache_creation_input_tokens": 900}}
        });
        let path = self
            .root
            .join("projects")
            .join("C--code-app")
            .join(format!("{SESSION}.jsonl"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("{record}\n")).unwrap();
    }

    fn ingest(&mut self) {
        ingest::ingest(&mut self.history, std::slice::from_ref(&self.root))
            .unwrap()
            .unwrap();
    }

    fn import(&mut self, database: &Path) -> Result<ImportReport, ImportError> {
        import::import_claude_usage(&mut self.history, database, NOW_MS)
    }

    fn query(&self, sql: &str) -> Vec<String> {
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

    fn requests(&self) -> Vec<String> {
        self.query(
            "SELECT sessions.session_id || ' ' || message_id || ' ' || coalesce(request_id, '-')
                 || ' ' || output_tokens || ' ' || cache_write_5m_tokens || ' '
                 || coalesce(subagent_id, '-') || ' ' || coalesce(imported_from, 'transcript')
             FROM requests JOIN sessions ON sessions.id = requests.session ORDER BY message_id",
        )
    }
}

#[test]
fn imports_the_requests_the_history_does_not_hold() {
    let mut fixture = Fixture::new();
    fixture.transcript("msg_1", "req_1", 50);
    fixture.ingest();
    let database = fixture.dir.path().join("usage.db");
    claude_usage(
        &database,
        &[
            (SESSION, "msg_1", r"C:\code\app", 50, None),
            (OLD_SESSION, "msg_2", r"C:\code\app", 60, None),
            (
                OLD_SESSION,
                "msg_3",
                r"C:\code\.worktrees\fix",
                70,
                Some("a7"),
            ),
            (API_SESSION, "msg_4", r"D:\work\api_v2", 80, None),
        ],
    );

    let report = fixture.import(&database).unwrap();

    assert_eq!(
        report,
        ImportReport {
            imported: 3,
            already_held: 1,
            unreadable: 0
        }
    );
    assert_eq!(
        fixture.requests(),
        [
            format!("{SESSION} msg_1 req_1 50 900 - transcript"),
            format!("{OLD_SESSION} msg_2 - 60 900 - claude-usage"),
            format!("{OLD_SESSION} msg_3 - 70 900 a7 claude-usage"),
            format!("{API_SESSION} msg_4 - 80 900 - claude-usage"),
        ]
    );
    assert_eq!(
        fixture.query("SELECT transcript_folder FROM projects ORDER BY id"),
        ["C--code-app", "D--work-api-v2"]
    );
    assert_eq!(
        fixture.query(&format!(
            "SELECT ai_title FROM sessions WHERE session_id = '{OLD_SESSION}'"
        )),
        ["Old importer work"]
    );
    assert_eq!(
        fixture
            .history
            .import(CLAUDE_USAGE)
            .unwrap()
            .map(|i| i.requests),
        Some(3)
    );
}

#[test]
fn a_transcript_read_after_the_import_replaces_the_imported_request() {
    let mut fixture = Fixture::new();
    let database = fixture.dir.path().join("usage.db");
    claude_usage(&database, &[(SESSION, "msg_1", r"C:\code\app", 50, None)]);
    fixture.import(&database).unwrap();

    fixture.transcript("msg_1", "req_1", 55);
    fixture.ingest();

    assert_eq!(
        fixture.requests(),
        [format!("{SESSION} msg_1 req_1 55 900 - transcript")]
    );
}

#[test]
fn importing_again_adds_nothing() {
    let mut fixture = Fixture::new();
    let database = fixture.dir.path().join("usage.db");
    claude_usage(
        &database,
        &[
            (OLD_SESSION, "msg_2", r"C:\code\app", 60, None),
            (OLD_SESSION, "msg_3", r"C:\code\app", 70, None),
        ],
    );
    fixture.import(&database).unwrap();

    let again = fixture.import(&database).unwrap();

    assert_eq!((again.imported, again.already_held), (0, 2));
    assert_eq!(fixture.requests().len(), 2);
}

#[test]
fn rows_without_a_time_model_or_message_id_are_counted_and_skipped() {
    let mut fixture = Fixture::new();
    let database = fixture.dir.path().join("usage.db");
    claude_usage(
        &database,
        &[(OLD_SESSION, "msg_2", r"C:\code\app", 60, None)],
    );
    Connection::open(&database)
        .unwrap()
        .execute_batch(
            "INSERT INTO turns (session_id, timestamp, model, message_id)
                 VALUES ('s', 'not a time', 'claude-opus-5', 'msg_9');
             INSERT INTO turns (session_id, timestamp, model, message_id)
                 VALUES ('s', '2026-08-01T10:00:00Z', NULL, 'msg_10');",
        )
        .unwrap();

    let report = fixture.import(&database).unwrap();

    assert_eq!((report.imported, report.unreadable), (1, 2));
}

#[test]
fn a_database_that_is_not_claude_usages_is_refused() {
    let mut fixture = Fixture::new();
    let database = fixture.dir.path().join("other.db");
    Connection::open(&database)
        .unwrap()
        .execute_batch("CREATE TABLE turns (id INTEGER PRIMARY KEY, words TEXT)")
        .unwrap();

    let error = fixture.import(&database).unwrap_err();

    assert!(
        matches!(error, ImportError::NotClaudeUsage { .. }),
        "{error}"
    );
    assert_eq!(fixture.requests().len(), 0);
}

#[test]
fn the_database_is_found_in_the_data_root() {
    let fixture = Fixture::new();
    assert_eq!(import::claude_usage_database(&fixture.root), None);
    fs::create_dir_all(&fixture.root).unwrap();
    claude_usage(&fixture.root.join("usage.db"), &[]);
    assert_eq!(
        import::claude_usage_database(&fixture.root),
        Some(fixture.root.join("usage.db"))
    );
}
