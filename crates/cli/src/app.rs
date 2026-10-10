//! What every command works with: the history, the data roots, the time zone and the prices.

use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use claude_dashboard_core::history::{self, History, HistoryError};
use claude_dashboard_core::ingest::{self, Progress, Report};
use claude_dashboard_core::pricing::PriceTable;
use claude_dashboard_core::roots::{self, CONFIG_DIR_ENV, DataRoot};
use claude_dashboard_core::usage_endpoint::LoginSource;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

use crate::fmt::grouped;

/// The environment variable that moves the history to another file, for a portable setup or a
/// test.
pub const HISTORY_ENV: &str = "CLAUDE_DASHBOARD_HISTORY";

/// How long a first ingest runs before it shows its progress.
const QUIET_FOR: Duration = Duration::from_millis(300);

#[derive(Clone)]
pub struct App {
    pub history_path: Option<PathBuf>,
    pub roots: Vec<DataRoot>,
    /// The value of `CLAUDE_CONFIG_DIR`.
    pub config_dir: Option<OsString>,
    pub zone: TimeZone,
    pub now: Timestamp,
    pub prices: &'static PriceTable,
}

impl App {
    pub fn new() -> Self {
        let history_path = std::env::var_os(HISTORY_ENV)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(history::default_path);
        let config_dir = std::env::var_os(CONFIG_DIR_ENV);
        let home = std::env::home_dir();
        Self {
            history_path,
            roots: roots::data_roots(config_dir.as_deref(), home.as_deref(), &[]),
            config_dir,
            zone: TimeZone::system(),
            now: Timestamp::now(),
            prices: PriceTable::bundled(),
        }
    }

    pub fn now_zoned(&self) -> Zoned {
        self.now.to_zoned(self.zone.clone())
    }

    /// The same app as it is at `now`, for work that outlives a moment, such as a server.
    pub fn at(&self, now: Timestamp) -> Self {
        Self {
            now,
            ..self.clone()
        }
    }

    /// Opens the history, creating it on first use.
    pub fn open_history(&self) -> Result<History, String> {
        let path = self.history_path.as_ref().ok_or_else(|| {
            format!(
                "There is no folder for local app data on this system; set {HISTORY_ENV} to a file for the history."
            )
        })?;
        History::open(path).map_err(|error| error.to_string())
    }

    /// Where Claude Code keeps the login that the usage endpoint needs.
    pub fn login_source(&self) -> Option<LoginSource> {
        LoginSource::of(&self.roots, self.config_dir.as_deref())
    }

    /// The data roots, as paths.
    pub fn root_paths(&self) -> Vec<PathBuf> {
        self.roots.iter().map(|root| root.path.clone()).collect()
    }

    /// Reads new transcript content into the history. A first ingest that takes a while shows
    /// its progress on standard error when that is a terminal.
    pub fn ingest(&self, history: &mut History) -> Result<Option<Report>, HistoryError> {
        let started = Instant::now();
        let show = std::io::stderr().is_terminal();
        let mut shown = false;
        let report = ingest::ingest_with_progress(
            history,
            &self.root_paths(),
            |Progress { done, total }| {
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
            },
        );
        if shown {
            let _ = write!(std::io::stderr(), "\r{}\r", " ".repeat(60));
        }
        report
    }
}
