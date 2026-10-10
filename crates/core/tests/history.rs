use std::fs;
use std::path::Path;

use claude_dashboard_core::history::{
    Access, Attribution, Counts, History, HistoryError, Identity, Request, SessionKey, Tokens,
};
use rusqlite::{Connection, OpenFlags};

fn request<'a>(
    session: SessionKey,
    identity: Identity<'a>,
    record_id: &'a str,
    output: u32,
) -> Request<'a> {
    Request {
        session,
        identity,
        record_id,
        subagent_id: None,
        timestamp_ms: 1_791_590_400_000,
        model: "claude-sonnet-5-5",
        tokens: Tokens {
            input: 12,
            output,
            cache_read: 4_000,
            cache_write_5m: 300,
            cache_write_1h: 0,
        },
        web_search_requests: 0,
        web_fetch_requests: 0,
        speed: Some("standard"),
        service_tier: Some("standard"),
        inference_geo: None,
        attribution: Attribution::default(),
        entrypoint: Some("cli"),
        claude_code_version: Some("2.1.296"),
    }
}

const MESSAGE: Identity<'static> = Identity::MessageAndRequest {
    message_id: "msg_01",
    request_id: "req_01",
};

/// Opens a new history with one project and two sessions in it.
fn new_history(dir: &Path) -> (History, SessionKey, SessionKey) {
    let mut history = History::open(&dir.join("history.sqlite")).unwrap();
    let (first, second) = history
        .write(|w| {
            let project = w.project("C--code-app", Some(r"C:\code\app"))?;
            Ok((
                w.session("session-1", project)?,
                w.session("session-2", project)?,
            ))
        })
        .unwrap();
    (history, first, second)
}

fn record(history: &mut History, requests: &[Request<'_>]) {
    history
        .write(|w| requests.iter().try_for_each(|request| w.request(request)))
        .unwrap();
}

/// The recorded requests as (record id, output tokens), read through a separate connection.
fn recorded(path: &Path) -> Vec<(String, u32)> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    conn.prepare("SELECT record_id, output_tokens FROM requests ORDER BY record_id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn pragma<T: rusqlite::types::FromSql>(path: &Path, name: &str) -> T {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    conn.pragma_query_value(None, name, |row| row.get(0))
        .unwrap()
}

fn set_schema(path: &Path, version: u32, readable_from: u32) {
    let conn = Connection::open(path).unwrap();
    conn.pragma_update(None, "user_version", version).unwrap();
    conn.execute(
        "UPDATE compatibility SET readable_from = ?1",
        [readable_from],
    )
    .unwrap();
}

#[test]
fn a_new_history_is_created_with_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join("data")
        .join("claude-dashboard")
        .join("history.sqlite");

    let history = History::open(&path).unwrap();

    assert_eq!(history.access(), Access::ReadWrite);
    assert_eq!(history.counts().unwrap(), Counts::default());
    assert_eq!(history.integrity_check().unwrap(), Vec::<String>::new());
    assert_eq!(pragma::<String>(&path, "journal_mode"), "wal");
    assert_eq!(
        pragma::<i32>(&path, "application_id"),
        i32::from_be_bytes(*b"CDHI")
    );
    assert!(pragma::<u32>(&path, "user_version") >= 1);
}

#[test]
fn the_history_outlives_the_process_that_wrote_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut history, session, _) = new_history(dir.path());
    record(&mut history, &[request(session, MESSAGE, "rec-1", 50)]);
    drop(history);

    let history = History::open(&dir.path().join("history.sqlite")).unwrap();

    assert_eq!(
        history.counts().unwrap(),
        Counts {
            projects: 1,
            sessions: 2,
            requests: 1
        }
    );
}

#[test]
fn recording_a_request_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut history, session, _) = new_history(dir.path());
    let streamed = request(session, MESSAGE, "rec-1", 50);

    record(&mut history, &[streamed]);
    record(&mut history, &[streamed, streamed]);

    assert_eq!(recorded(history.path()), [("rec-1".to_owned(), 50)]);
}

/// Records three requests in every order, each order into a new history with the sessions
/// `session-1` and `session-2`, and returns what each history holds as (session id, record id,
/// output tokens).
fn in_every_order(
    requests: impl Fn(SessionKey, SessionKey) -> [Request<'static>; 3],
) -> Vec<Vec<(String, String, u32)>> {
    let orders = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    orders
        .iter()
        .map(|order| {
            let dir = tempfile::tempdir().unwrap();
            let (mut history, first, second) = new_history(dir.path());
            let requests = requests(first, second);
            for &index in order {
                record(&mut history, &[requests[index]]);
            }
            let conn =
                Connection::open_with_flags(history.path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .unwrap();
            conn.prepare(
                "SELECT sessions.session_id, record_id, output_tokens
                 FROM requests JOIN sessions ON sessions.id = requests.session",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
        })
        .collect()
}

fn held(session_id: &str, record_id: &str, output: u32) -> Vec<(String, String, u32)> {
    vec![(session_id.to_owned(), record_id.to_owned(), output)]
}

#[test]
fn the_record_with_the_most_tokens_wins_in_any_order() {
    let histories = in_every_order(|session, _| {
        [
            request(session, MESSAGE, "rec-a", 1),
            request(session, MESSAGE, "rec-b", 90),
            request(session, MESSAGE, "rec-c", 40),
        ]
    });

    for requests in histories {
        assert_eq!(requests, held("session-1", "rec-b", 90));
    }
}

#[test]
fn equal_usage_goes_to_the_later_record_then_the_larger_record_id() {
    let histories = in_every_order(|session, _| {
        let earlier = request(session, MESSAGE, "rec-z", 50);
        let later = Request {
            timestamp_ms: earlier.timestamp_ms + 1,
            ..request(session, MESSAGE, "rec-b", 50)
        };
        [
            earlier,
            later,
            Request {
                record_id: "rec-c",
                ..later
            },
        ]
    });

    for requests in histories {
        assert_eq!(requests, held("session-1", "rec-c", 50));
    }
}

#[test]
fn a_verbatim_copy_in_another_session_goes_to_the_larger_session_id() {
    let histories = in_every_order(|first, second| {
        let original = request(first, MESSAGE, "rec-1", 50);
        let copy = Request {
            session: second,
            ..original
        };
        [original, copy, original]
    });

    for requests in histories {
        assert_eq!(requests, held("session-2", "rec-1", 50));
    }
}

#[test]
fn a_request_copied_into_another_session_counts_once() {
    let dir = tempfile::tempdir().unwrap();
    let (mut history, first, second) = new_history(dir.path());

    record(
        &mut history,
        &[
            request(first, MESSAGE, "rec-1", 50),
            request(second, MESSAGE, "rec-2", 50),
        ],
    );

    assert_eq!(history.counts().unwrap().requests, 1);
}

#[test]
fn different_requests_for_one_message_count_separately() {
    let dir = tempfile::tempdir().unwrap();
    let (mut history, session, _) = new_history(dir.path());
    let retried = Identity::MessageAndRequest {
        message_id: "msg_01",
        request_id: "req_02",
    };

    record(
        &mut history,
        &[
            request(session, MESSAGE, "rec-1", 50),
            request(session, retried, "rec-2", 50),
        ],
    );

    assert_eq!(history.counts().unwrap().requests, 2);
}

#[test]
fn without_a_request_id_a_message_is_unique_within_its_session() {
    let dir = tempfile::tempdir().unwrap();
    let (mut history, first, second) = new_history(dir.path());
    let message = Identity::from_ids(Some("msg_01"), None);

    record(
        &mut history,
        &[
            request(first, message, "rec-1", 10),
            request(first, message, "rec-2", 20),
            request(second, message, "rec-3", 30),
        ],
    );

    assert_eq!(
        recorded(history.path()),
        [("rec-2".to_owned(), 20), ("rec-3".to_owned(), 30)]
    );
}

#[test]
fn without_a_message_id_each_record_is_its_own_request() {
    let dir = tempfile::tempdir().unwrap();
    let (mut history, session, _) = new_history(dir.path());
    let no_ids = Identity::from_ids(None, None);

    record(
        &mut history,
        &[
            request(session, no_ids, "rec-1", 10),
            request(session, no_ids, "rec-2", 20),
            request(session, no_ids, "rec-1", 10),
        ],
    );

    assert_eq!(
        recorded(history.path()),
        [("rec-1".to_owned(), 10), ("rec-2".to_owned(), 20)]
    );
}

#[test]
fn a_failed_write_records_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let mut history = History::open(&path).unwrap();

    let result: Result<(), HistoryError> = history.write(|w| {
        let project = w.project("C--code-app", None)?;
        let session = w.session("session-1", project)?;
        w.request(&request(session, MESSAGE, "rec-1", 50))?;
        Err(HistoryError::NotAHistory { path: path.clone() })
    });

    assert!(result.is_err());
    assert_eq!(history.counts().unwrap(), Counts::default());
}

#[test]
fn a_project_keeps_the_first_working_folder_named() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let mut history = History::open(&path).unwrap();

    let keys = history
        .write(|w| {
            Ok([
                w.project("C--code-app", None)?,
                w.project("C--code-app", Some(r"C:\code\app"))?,
                w.project("C--code-app", Some(r"C:\elsewhere"))?,
            ])
        })
        .unwrap();

    assert!(keys.iter().all(|key| *key == keys[0]));
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let folder: String = conn
        .query_row("SELECT path FROM projects", [], |row| row.get(0))
        .unwrap();
    assert_eq!(folder, r"C:\code\app");
}

#[test]
fn a_history_upgraded_by_a_newer_version_opens_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let (mut history, session, _) = new_history(dir.path());
    record(&mut history, &[request(session, MESSAGE, "rec-1", 50)]);
    drop(history);
    set_schema(&path, 999, 1);

    let mut history = History::open(&path).unwrap();

    assert_eq!(history.access(), Access::ReadOnly { schema: 999 });
    assert_eq!(history.counts().unwrap().requests, 1);
    let error = history.write(|_| Ok(())).unwrap_err();
    assert!(
        matches!(error, HistoryError::ReadOnly { schema: 999 }),
        "{error:?}"
    );
    assert!(matches!(
        history.try_lock_ingest(),
        Err(HistoryError::ReadOnly { schema: 999 })
    ));
}

#[test]
fn a_history_that_older_versions_cannot_read_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    drop(History::open(&path).unwrap());
    set_schema(&path, 999, 999);

    let error = History::open(&path).unwrap_err();

    assert!(
        matches!(error, HistoryError::TooNew { schema: 999 }),
        "{error:?}"
    );
}

#[test]
fn writes_stop_once_a_newer_version_upgrades_the_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let (mut history, session, _) = new_history(dir.path());

    set_schema(&path, 999, 1);
    let error = history
        .write(|w| w.request(&request(session, MESSAGE, "rec-1", 50)))
        .unwrap_err();

    assert!(
        matches!(error, HistoryError::ReadOnly { schema: 999 }),
        "{error:?}"
    );
    assert_eq!(history.access(), Access::ReadOnly { schema: 999 });
    assert_eq!(history.counts().unwrap().requests, 0);
}

#[test]
fn another_programs_database_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE notes (body TEXT)")
        .unwrap();

    let error = History::open(&path).unwrap_err();

    assert!(
        matches!(error, HistoryError::NotAHistory { .. }),
        "{error:?}"
    );
    assert_eq!(pragma::<String>(&path, "journal_mode"), "delete");
    assert_eq!(pragma::<u32>(&path, "user_version"), 0);
}

#[test]
fn a_file_that_is_not_a_database_is_refused_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    fs::write(&path, "not a database, but keep me").unwrap();

    assert!(History::open(&path).is_err());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "not a database, but keep me"
    );
}

#[test]
fn one_process_at_a_time_holds_the_ingest_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let first = History::open(&path).unwrap();
    let second = History::open(&path).unwrap();

    let lock = first.try_lock_ingest().unwrap();
    assert!(lock.is_some());
    assert!(second.try_lock_ingest().unwrap().is_none());

    drop(lock);
    assert!(second.try_lock_ingest().unwrap().is_some());
}
