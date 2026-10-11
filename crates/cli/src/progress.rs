//! Reading new transcript content for a command, with its progress on standard error when that
//! is a terminal.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use claude_dashboard_core::history::{History, HistoryError};
use claude_dashboard_core::ingest::{self, Progress, Report};
use claude_dashboard_server::app::App;
use claude_dashboard_server::fmt::grouped;

/// How long a first ingest runs before it shows its progress.
const QUIET_FOR: Duration = Duration::from_millis(300);

/// Reads new transcript content into the history. A first ingest that takes a while shows its
/// progress on standard error when that is a terminal.
pub fn ingest(app: &App, history: &mut History) -> Result<Option<Report>, HistoryError> {
    let started = Instant::now();
    let show = std::io::stderr().is_terminal();
    let mut shown = false;
    let report =
        ingest::ingest_with_progress(history, &app.root_paths(), |Progress { done, total }| {
            if show && started.elapsed() >= QUIET_FOR && (done % 25 == 0 || done == total) {
                shown = true;
                let mut stderr = std::io::stderr();
                let _ = write!(
                    stderr,
                    "\rReading your history: {} of {} transcripts",
                    grouped(done as u64),
                    grouped(total as u64)
                );
                let _ = stderr.flush();
            }
        });
    if shown {
        let _ = write!(std::io::stderr(), "\r{}\r", " ".repeat(60));
    }
    report
}
