//! Saving an export: what a cancelled or failed one leaves in the folder, that the dashboard keeps
//! answering while one writes, and that every dataset saves in both formats.

use std::fs;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use claude_dashboard_core::history::{Attribution, History, Identity, Request};
use claude_dashboard_core::limits::{LimitSnapshot, Source};
use claude_dashboard_core::tokens::Tokens;
use claude_dashboard_server::Server;
use claude_dashboard_server::app::App;
use claude_dashboard_server::export::{self, ExportError};
use claude_dashboard_server::protocol::{Dataset, Days, ExportFormat, ExportParams, Filters};
use jiff::tz::TimeZone;
use serde_json::Value;

/// The requests the history holds: more than are written between two reports of progress.
const REQUESTS: u64 = 2_500;

/// How many requests are written between two reports of progress.
const PROGRESS_EVERY: u64 = 1_000;

/// When the first request was made, 2026-10-05T09:00:00Z; the others follow a second apart.
const FIRST_REQUEST_MS: i64 = 1_791_190_800_000;

/// How long a test waits for something it expects, so that a failure shows rather than hangs.
const PATIENCE: Duration = Duration::from_secs(30);

/// A history of [`REQUESTS`] requests made within the hour and a reading of the five-hour window
/// they fall in, an app that reads that history and nothing else, and an empty folder to save
/// exports in.
struct Setup {
    _dir: tempfile::TempDir,
    app: App,
    exports: PathBuf,
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let mut history = History::open(&path).unwrap();
    history
        .write(|w| {
            let project = w.project("C--code-app", Some(r"C:\code\app"))?;
            let session = w.session("session-1", project)?;
            for index in 0..REQUESTS {
                let message = format!("msg_{index}");
                let request = format!("req_{index}");
                let record = format!("rec-{index}");
                w.request(&Request {
                    session,
                    identity: Identity::from_ids(Some(&message), Some(&request)),
                    record_id: &record,
                    subagent_id: None,
                    timestamp_ms: FIRST_REQUEST_MS + i64::try_from(index).unwrap() * 1_000,
                    model: "claude-opus-5-5",
                    tokens: Tokens {
                        input: 3,
                        output: 640,
                        cache_read: 42_000,
                        cache_write_5m: 500,
                        cache_write_1h: 1_000,
                    },
                    web_search_requests: 0,
                    web_fetch_requests: 0,
                    speed: None,
                    service_tier: None,
                    inference_geo: None,
                    attribution: Attribution::default(),
                    entrypoint: Some("cli"),
                    claude_code_version: Some("2.1.296"),
                    imported_from: None,
                })?;
            }
            w.limit_snapshot(&LimitSnapshot {
                window: "five_hour".to_owned(),
                source: Source::StatusLine,
                observed_ms: FIRST_REQUEST_MS + 5 * 60 * 1_000,
                used_percentage: 12.5,
                resets_at_ms: Some(FIRST_REQUEST_MS + 4 * 60 * 60 * 1_000),
            })?;
            Ok(())
        })
        .unwrap();
    drop(history);
    let exports = dir.path().join("exports");
    fs::create_dir(&exports).unwrap();
    // The data root is a folder that does not exist, so that the server reads no transcripts,
    // whatever this computer holds, and days are counted in UTC wherever the test runs.
    let app = App {
        history_path: Some(path),
        roots: Vec::new(),
        config_dir: Some(dir.path().join(".claude").into_os_string()),
        home: Some(dir.path().to_owned()),
        zone: TimeZone::UTC,
        system_zone: TimeZone::UTC,
        ..App::new()
    };
    Setup {
        _dir: dir,
        app,
        exports,
    }
}

fn export_of(dataset: Dataset, format: ExportFormat) -> ExportParams {
    ExportParams {
        dataset,
        format,
        days: None,
        filters: Filters::default(),
        search: None,
        session: None,
    }
}

fn every_request(format: ExportFormat) -> ExportParams {
    export_of(Dataset::Requests, format)
}

/// The names of what a folder holds.
fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_cancelled_export_leaves_no_file() {
    let setup = setup();
    let destination = setup.exports.join("history.csv");
    let mut reports = Vec::new();

    let result = export::save(
        &setup.app,
        &every_request(ExportFormat::Csv),
        &destination,
        &mut |written, total| {
            reports.push((written, total));
            ControlFlow::Break(())
        },
    );

    assert_eq!(result, Err(ExportError::Cancelled));
    assert_eq!(
        reports,
        [(PROGRESS_EVERY, REQUESTS)],
        "the export stops at the first report"
    );
    assert!(!destination.exists());
    assert_eq!(
        names(&setup.exports),
        Vec::<String>::new(),
        "neither the file nor a .claude-dashboard-*.part file is left"
    );
}

#[test]
fn a_cancelled_json_export_leaves_no_file() {
    let setup = setup();
    let destination = setup.exports.join("history.json");

    let result = export::save(
        &setup.app,
        &every_request(ExportFormat::Json),
        &destination,
        &mut |_, _| ControlFlow::Break(()),
    );

    assert_eq!(result, Err(ExportError::Cancelled));
    assert_eq!(names(&setup.exports), Vec::<String>::new());
}

#[test]
fn a_failed_export_leaves_no_partial_file() {
    let setup = setup();
    // A folder in the file's place: writing succeeds, putting the file there does not.
    let destination = setup.exports.join("history.csv");
    fs::create_dir(&destination).unwrap();

    let result = export::save(
        &setup.app,
        &every_request(ExportFormat::Csv),
        &destination,
        &mut |_, _| ControlFlow::Continue(()),
    );

    match result {
        Err(ExportError::Failed(reason)) => assert!(
            reason.contains("Cannot save") && reason.contains("history.csv"),
            "{reason}"
        ),
        other => panic!("expected a failure, got {other:?}"),
    }
    assert!(destination.is_dir());
    assert_eq!(
        names(&setup.exports),
        ["history.csv"],
        "no .claude-dashboard-*.part file is left beside the folder in the way"
    );
}

#[test]
fn the_dashboard_keeps_answering_while_an_export_writes() {
    let setup = setup();
    let destination = setup.exports.join("history.csv");
    let mut server = Server::start(setup.app.clone(), Arc::new(|_, _| {}));
    let (reached, has_reached) = mpsc::channel::<()>();
    let (release, released) = mpsc::channel::<()>();

    let export_thread = {
        let app = setup.app.clone();
        let destination = destination.clone();
        thread::spawn(move || {
            let mut reports = Vec::new();
            let result = export::save(
                &app,
                &every_request(ExportFormat::Csv),
                &destination,
                &mut |written, total| {
                    if reports.is_empty() {
                        reached.send(()).unwrap();
                        released.recv_timeout(PATIENCE).unwrap();
                    }
                    reports.push((written, total));
                    ControlFlow::Continue(())
                },
            );
            (result, reports)
        })
    };
    has_reached
        .recv_timeout(PATIENCE)
        .expect("the export reports its progress");

    let status = server.answer("status", Value::Null);
    let still_writing = !export_thread.is_finished();
    let _ = release.send(());
    let outcome = export_thread.join();
    server.stop();

    let status = status.expect("the server answers while the export is held up");
    assert!(status["version"].is_string(), "{status}");
    assert!(
        still_writing,
        "the answer came only once the export was over"
    );
    let (result, reports) = outcome.expect("the export thread does not panic");
    let exported = result.expect("the export saves once it is released");
    assert_eq!(exported.rows, REQUESTS);
    assert_eq!(exported.what, "2,500 requests");
    assert_eq!(exported.path, Some(destination.display().to_string()));
    assert_eq!(exported.content, None);
    assert_eq!(
        reports,
        [
            (PROGRESS_EVERY, REQUESTS),
            (2 * PROGRESS_EVERY, REQUESTS),
            (REQUESTS, REQUESTS)
        ]
    );

    let csv = fs::read_to_string(&destination).unwrap();
    assert!(csv.starts_with("\u{feff}at,session_id,title,"));
    assert_eq!(csv.lines().count(), 1 + usize::try_from(REQUESTS).unwrap());
    assert_eq!(names(&setup.exports), ["history.csv"]);
}

#[test]
fn every_dataset_saves_as_csv_and_as_json() {
    let setup = setup();
    // Days either side of the one the requests were made on.
    let days = Days {
        first: "2026-10-04".to_owned(),
        last: "2026-10-06".to_owned(),
    };
    let datasets = [
        (Dataset::Days, 1),
        (Dataset::Weeks, 1),
        (Dataset::Months, 1),
        (Dataset::Models, 1),
        (Dataset::Projects, 1),
        (Dataset::Sessions, 1),
        (Dataset::FiveHourWindows, 1),
        (Dataset::Requests, REQUESTS),
    ];
    for (index, (dataset, rows)) in datasets.into_iter().enumerate() {
        for (format, extension) in [(ExportFormat::Csv, "csv"), (ExportFormat::Json, "json")] {
            let destination = setup.exports.join(format!("{index}.{extension}"));
            let mut params = export_of(dataset, format);
            params.days = Some(days.clone());

            let exported = export::save(&setup.app, &params, &destination, &mut |_, _| {
                ControlFlow::Continue(())
            })
            .unwrap();

            assert_eq!(exported.rows, rows, "{dataset:?} as {extension}");
            let text = fs::read_to_string(&destination).unwrap();
            if format == ExportFormat::Csv {
                let csv = text.strip_prefix('\u{feff}').expect("a byte order mark");
                let lines: Vec<&str> = csv.lines().collect();
                assert_eq!(
                    lines.len() as u64,
                    rows + 1,
                    "{dataset:?}: a header and a line each"
                );
                let fields = lines[0].matches(',').count();
                assert!(
                    lines.iter().all(|line| line.matches(',').count() == fields),
                    "{dataset:?}: every line has the header's fields"
                );
            } else {
                let json: Value = serde_json::from_str(&text).unwrap();
                assert_eq!(json.as_array().unwrap().len() as u64, rows, "{dataset:?}");
            }
        }
    }
}
