//! Histories that earlier versions made, holding data, upgraded through the real migrations. Each
//! keeps everything it held and reports as a history made at the latest version with the same rows
//! does, and its copy from before the upgrade stays beside it.

use std::collections::BTreeMap;
use std::fmt::{Debug, Write as _};
use std::fs;
use std::path::Path;

use claude_dashboard_core::history::{Access, History};
use claude_dashboard_core::pricing::PriceTable;
use claude_dashboard_core::report::{self, Range};
use jiff::tz::TimeZone;
use rusqlite::Connection;

/// Every migration, as the history applies them: schema version N is the result of the first N.
const MIGRATIONS: [&str; 5] = [
    include_str!("../src/history/migrations/0001-projects-sessions-requests.sql"),
    include_str!("../src/history/migrations/0002-ingest.sql"),
    include_str!("../src/history/migrations/0003-usage-endpoint.sql"),
    include_str!("../src/history/migrations/0004-request-time.sql"),
    include_str!("../src/history/migrations/0005-subagents-and-settings.sql"),
];

/// The application id that marks an SQLite file as a history.
const HISTORY_ID: i32 = i32::from_be_bytes(*b"CDHI");

/// From 08:00 on 30 August 2026 to the end of 2 September, in UTC: every request below.
const RANGE: Range = Range {
    from_ms: 1_788_076_800_000,
    to_ms: 1_788_393_600_000,
};

/// The rows a history can hold from the schema version given: two projects, a subagent's request,
/// fast mode, US-only inference, web searches and fetches, a skill and an MCP tool, a request
/// without a request id, one without a message id, a model with no price, a request brought in
/// from claude-usage, titles, a reported cost, a plan window's reading, a day from Claude Code's
/// statistics, read positions, the usage endpoint's last turn, a subagent's type, a shared setting
/// and an added data folder.
const ROWS: &[(u32, &str)] = &[
    (
        1,
        "INSERT INTO projects (id, transcript_folder, path)
         VALUES (1, 'C--code-app', 'C:\\code\\app'), (2, 'C--code-site', NULL)",
    ),
    (
        1,
        "INSERT INTO sessions (id, session_id, project)
         VALUES (1, 'session-app', 1), (2, 'session-site', 2)",
    ),
    (
        1,
        "INSERT INTO requests (id, session, message_id, request_id, record_id, subagent_id,
             timestamp_ms, model, input_tokens, output_tokens, cache_read_tokens,
             cache_write_5m_tokens, cache_write_1h_tokens, web_search_requests,
             web_fetch_requests, speed, service_tier, inference_geo, skill, mcp_server, mcp_tool,
             entrypoint, claude_code_version)
         VALUES
             (1, 1, 'msg_1', 'req_1', 'rec-1', NULL, 1788256800000, 'claude-opus-5-5',
              3, 640, 42000, 1500, 0, 0, 0, 'fast', 'standard', NULL, NULL, NULL, NULL,
              'cli', '2.1.0'),
             (2, 1, 'msg_2', 'req_2', 'rec-2', 'a1b2c3', 1788257100000, 'claude-haiku-5-5',
              120, 300, 9000, 0, 2000, 1, 2, 'standard', 'standard', NULL, 'review', 'github',
              'get_issue', 'cli', '2.1.0'),
             (3, 2, 'msg_3', NULL, 'rec-3', NULL, 1788339600000, 'claude-sonnet-5-5',
              10, 800, 15000, 500, 0, 0, 0, 'standard', 'standard', 'us', NULL, NULL, NULL,
              'vscode', '2.1.1'),
             (4, 2, NULL, NULL, 'rec-4', NULL, 1788341400000, 'claude-unknown-9',
              5, 50, 0, 0, 0, 0, 0, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
             (5, 1, 'msg_5', 'req_5', 'rec-5', NULL, 1788076800000, 'claude-opus-5-5',
              8, 400, 20000, 0, 0, 0, 0, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
    ),
    (
        2,
        "UPDATE requests SET imported_from = 'claude-usage' WHERE id = 5",
    ),
    (
        2,
        "UPDATE sessions SET ai_title = 'Fix the build' WHERE id = 1",
    ),
    (
        2,
        "UPDATE sessions SET ai_title = 'Draft notes', custom_title = 'Release notes' WHERE id = 2",
    ),
    (
        2,
        "INSERT INTO reported_costs (session, started_ms, model, cost_usd, input_tokens,
             output_tokens, cache_read_tokens, cache_write_tokens)
         VALUES (1, 1788256000000, 'claude-opus-5-5', 0.0312, 3, 640, 42000, 1500)",
    ),
    (
        2,
        "INSERT INTO imports (source, imported_ms, requests)
         VALUES ('claude-usage', 1788250000000, 1)",
    ),
    (
        2,
        "INSERT INTO daily_totals (day, model, tokens)
         VALUES ('2026-08-29', 'claude-opus-5-5', 125000)",
    ),
    (
        2,
        "INSERT INTO limit_snapshots (window, source, bucket_ms, observed_ms, used_percentage,
             resets_at_ms)
         VALUES ('five_hour', 'status-line', 1788256800000, 1788256860000, 42.0, 1788264000000)",
    ),
    (
        2,
        "INSERT INTO transcripts (path, read_to, len, modified_ms)
         VALUES
             ('/home/me/.claude/projects/C--code-app/session-app.jsonl',
              4096, 4096, 1788257200000),
             ('/home/me/.claude/projects/C--code-app/session-app/subagents/agent-a1b2c3.jsonl',
              1024, 1024, 1788257200000)",
    ),
    (
        3,
        "UPDATE usage_endpoint
         SET enabled = 1, next_ask_ms = 1788257400000, asked_ms = 1788257100000,
             outcome = 'answered'",
    ),
    (
        5,
        "INSERT INTO subagents (session, subagent_id, agent_type)
         VALUES (1, 'a1b2c3', 'Explore')",
    ),
    (
        5,
        "INSERT INTO settings (name, value) VALUES ('week_start', 'sunday')",
    ),
    (
        5,
        "INSERT INTO data_folders (path, added_ms)
         VALUES ('/home/me/.claude-work', 1788250000000)",
    ),
];

fn latest() -> u32 {
    u32::try_from(MIGRATIONS.len()).unwrap()
}

/// Makes a history at schema `version`, as that version made it, holding the rows a history of
/// version `rows_of` could hold.
fn make(path: &Path, version: u32, rows_of: u32) {
    let conn = Connection::open(path).unwrap();
    for migration in &MIGRATIONS[..version as usize] {
        conn.execute_batch(migration).unwrap();
    }
    conn.pragma_update(None, "user_version", version).unwrap();
    conn.pragma_update(None, "application_id", HISTORY_ID)
        .unwrap();
    for (since, sql) in ROWS {
        if *since <= rows_of {
            conn.execute_batch(sql).unwrap();
        }
    }
}

/// The schema version of a history file, read without opening it as a history.
fn version_of(path: &Path) -> u32 {
    Connection::open(path)
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

/// Items in a stable order, whatever order the history gave them in.
fn sorted<T: Debug>(items: impl IntoIterator<Item = T>) -> Vec<String> {
    let mut items: Vec<String> = items.into_iter().map(|item| format!("{item:?}")).collect();
    items.sort();
    items
}

/// Everything a history says about what it holds, as text to compare.
fn picture(history: &History) -> String {
    let table = PriceTable::bundled();
    let zone = TimeZone::UTC;
    let mut picture = String::new();
    let mut add = |part: &dyn Debug| writeln!(picture, "{part:?}").unwrap();
    add(&history.counts().unwrap());
    add(&report::by_day(history, table, RANGE, &zone).unwrap());
    add(&report::by_model(history, table, RANGE).unwrap());
    add(&report::by_session(history, table, RANGE).unwrap());
    add(&report::by_project(history, table, RANGE).unwrap());
    add(&sorted(history.projects().unwrap()));
    add(&sorted(history.sessions().unwrap()));
    add(&sorted(history.all_subagent_types().unwrap()));
    add(&sorted(history.reported_costs().unwrap()));
    add(&sorted(history.latest_limits().unwrap()));
    add(&history.usage_endpoint().unwrap());
    add(&history.import("claude-usage").unwrap());
    add(&history
        .settings()
        .unwrap()
        .into_iter()
        .collect::<BTreeMap<_, _>>());
    add(&history.data_folders().unwrap());
    picture
}

#[test]
fn a_history_of_every_version_upgrades_with_everything_it_holds() {
    for version in 1..=latest() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("history.sqlite");
        let new = dir.path().join("made-at-the-latest-version.sqlite");
        make(&old, version, version);
        make(&new, latest(), version);

        let upgraded = History::open(&old).unwrap();
        let made = History::open(&new).unwrap();
        assert_eq!(upgraded.access(), Access::ReadWrite, "version {version}");
        assert_eq!(
            upgraded.schema_version().unwrap(),
            latest(),
            "version {version}"
        );
        assert_eq!(
            upgraded.integrity_check().unwrap(),
            Vec::<String>::new(),
            "version {version}"
        );
        assert_eq!(picture(&upgraded), picture(&made), "version {version}");

        let backup = old.with_extension("backup.sqlite");
        if version < latest() {
            assert_eq!(
                version_of(&backup),
                version,
                "the copy from before the upgrade"
            );
        } else {
            assert!(
                !backup.exists(),
                "a history at the latest version needs no copy"
            );
        }
    }
}

#[test]
fn an_empty_history_of_every_version_upgrades() {
    for version in 1..=latest() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("history.sqlite");
        let new = dir.path().join("made-at-the-latest-version.sqlite");
        make(&old, version, 0);
        make(&new, latest(), 0);

        let upgraded = History::open(&old).unwrap();
        assert_eq!(
            upgraded.schema_version().unwrap(),
            latest(),
            "version {version}"
        );
        assert_eq!(
            picture(&upgraded),
            picture(&History::open(&new).unwrap()),
            "version {version}"
        );
    }
}

#[test]
fn an_upgrade_reads_again_the_transcripts_its_steps_ask_for() {
    // Version 4 dates each request by its earliest record, so every transcript is read again; version
    // 5 reads subagents' metadata, so their transcripts are read again.
    for (version, kept) in [(2, 0), (3, 0), (4, 1), (5, 2)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        make(&path, version, version);
        let upgraded = History::open(&path).unwrap();
        assert_eq!(
            upgraded.read_positions().unwrap().len(),
            kept,
            "version {version}"
        );
    }
}

#[test]
fn these_tests_know_every_migration() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/history/migrations");
    let files = fs::read_dir(folder)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .is_ok_and(|entry| entry.path().extension().is_some_and(|ext| ext == "sql"))
        })
        .count();
    assert_eq!(
        files,
        MIGRATIONS.len(),
        "a new migration joins MIGRATIONS, and the rows its version can hold join ROWS"
    );
}
