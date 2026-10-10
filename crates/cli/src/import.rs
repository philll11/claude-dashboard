//! `claude-dashboard import`: bringing in history that another tool recorded.

use std::path::PathBuf;
use std::process::ExitCode;

use claude_dashboard_core::import::{self, ImportReport};

use crate::app::App;
use crate::fmt::grouped;
use crate::report::ready_history;

pub fn claude_usage(app: &App, database: Option<PathBuf>) -> ExitCode {
    let Some(database) = database.or_else(|| {
        app.roots
            .iter()
            .find_map(|root| import::claude_usage_database(&root.path))
    }) else {
        eprintln!(
            "There is no claude-usage database in Claude Code's folder; name it with --database."
        );
        return ExitCode::FAILURE;
    };
    // Transcripts come first, so that the import only adds what they no longer hold.
    let mut history = match ready_history(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    match import::import_claude_usage(&mut history, &database, app.now.as_millisecond()) {
        Ok(ImportReport {
            imported,
            already_held,
            unreadable,
        }) => {
            println!(
                "Imported {} {} from {}; the history already held {}.",
                grouped(imported),
                if imported == 1 { "request" } else { "requests" },
                database.display(),
                grouped(already_held)
            );
            if unreadable > 0 {
                println!(
                    "{} rows were left out, without the time, model or message id a request needs.",
                    grouped(unreadable)
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Cannot import: {error}");
            ExitCode::FAILURE
        }
    }
}
