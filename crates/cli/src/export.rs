//! `claude-dashboard export`, and the reports' JSON and CSV, which come from the same export as
//! the dashboard's files.

use std::io::{self, BufWriter, IsTerminal, Write};
use std::ops::ControlFlow;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use claude_dashboard_server::app::App;
use claude_dashboard_server::export;
use claude_dashboard_server::fmt::grouped;
use claude_dashboard_server::protocol::{Dataset, Days, ExportFormat, ExportParams, Filters};
use jiff::Timestamp;
use jiff::civil::Date;

use crate::report::{Output, colour_json, ready_history};
use crate::style::Paint;

/// How often an export to a file shows its progress on a terminal.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// What a report's `--json` or `--csv` writes: `dataset` for `days`.
pub fn params(dataset: Dataset, output: Output, first: Date, last: Date) -> ExportParams {
    ExportParams {
        dataset,
        format: if output == Output::Json {
            ExportFormat::Json
        } else {
            ExportFormat::Csv
        },
        days: Some(Days {
            first: first.to_string(),
            last: last.to_string(),
        }),
        filters: Filters::default(),
        search: None,
        session: None,
    }
}

/// Prints an export to standard output, its JSON coloured as the design shows it when standard
/// output is a terminal.
pub fn print(app: &App, params: &ExportParams) -> ExitCode {
    let paint = Paint::for_stdout();
    let mut quiet = |_, _| ControlFlow::Continue(());
    let result = if params.format == ExportFormat::Json && paint != Paint::PLAIN {
        let mut json = Vec::new();
        export::write(app, params, &mut json, false, &mut quiet).map(|_| {
            println!(
                "{}",
                colour_json(String::from_utf8_lossy(&json).trim_end(), paint)
            );
        })
    } else {
        let mut out = BufWriter::new(io::stdout().lock());
        export::write(app, params, &mut out, false, &mut quiet).and_then(|_| {
            out.flush().map_err(|error| {
                export::ExportError::Failed(format!("Cannot write the export: {error}"))
            })
        })
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// `claude-dashboard export`: every request, or those of the days given, as CSV or JSON, to
/// standard output or to a file saved for a spreadsheet.
pub fn run(
    app: &App,
    since: Option<Date>,
    until: Option<Date>,
    format: ExportFormat,
    output: Option<&Path>,
) -> ExitCode {
    let history = match ready_history(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    let days = if since.is_some() || until.is_some() {
        let today = app.now_zoned().date();
        let last = until.unwrap_or(today);
        let first = since.unwrap_or_else(|| {
            history
                .first_request_ms()
                .ok()
                .flatten()
                .and_then(|ms| Timestamp::from_millisecond(ms).ok())
                .map_or(last, |at| at.to_zoned(app.zone.clone()).date())
                .min(last)
        });
        Some(Days {
            first: first.to_string(),
            last: last.to_string(),
        })
    } else {
        None
    };
    drop(history);
    let params = ExportParams {
        dataset: Dataset::Requests,
        format,
        days,
        filters: Filters::default(),
        search: None,
        session: None,
    };
    let Some(path) = output else {
        return print(app, &params);
    };
    let show = io::stderr().is_terminal();
    let mut shown = Instant::now();
    let mut progress = |written: u64, total: u64| {
        if show && shown.elapsed() >= PROGRESS_EVERY {
            shown = Instant::now();
            let mut stderr = io::stderr();
            let _ = write!(
                stderr,
                "\rExporting: {} of {} requests",
                grouped(written),
                grouped(total)
            );
            let _ = stderr.flush();
        }
        ControlFlow::Continue(())
    };
    let saved = export::save(app, &params, path, &mut progress);
    if show {
        let _ = write!(io::stderr(), "\r{}\r", " ".repeat(60));
    }
    match saved {
        Ok(exported) => {
            println!("Saved {} to {}", exported.what, path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
