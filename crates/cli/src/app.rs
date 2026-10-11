//! What every command works with: the history, the data roots, the time zone and the prices.

use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use claude_dashboard_core::history::{self, History, HistoryError};
use claude_dashboard_core::ingest::{self, Progress, Report};
use claude_dashboard_core::pricing::PriceTable;
use claude_dashboard_core::roots::{self, CONFIG_DIR_ENV, DataRoot};
use claude_dashboard_core::usage_endpoint::LoginSource;
use jiff::civil::{Date, Weekday};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan, Zoned};

use crate::fmt::grouped;

/// The environment variable that moves the history to another file, for a portable setup or a
/// test.
pub const HISTORY_ENV: &str = "CLAUDE_DASHBOARD_HISTORY";

/// The environment variable that fixes the time every command sees, as an RFC 3339 timestamp such
/// as `2026-10-10T12:15:00+13:00`, so that tests and the web UI's screenshots do not depend on
/// when they run.
pub const NOW_ENV: &str = "CLAUDE_DASHBOARD_NOW";

/// The current time, or the time [`NOW_ENV`] fixes.
pub fn now() -> Timestamp {
    static FIXED: LazyLock<Option<Timestamp>> =
        LazyLock::new(|| std::env::var(NOW_ENV).ok()?.parse().ok());
    FIXED.unwrap_or_else(Timestamp::now)
}

/// A day of the week by its English name, such as `monday`, in any case.
pub fn weekday(name: &str) -> Option<Weekday> {
    WEEKDAYS
        .iter()
        .find(|(day, _)| day.eq_ignore_ascii_case(name))
        .map(|&(_, weekday)| weekday)
}

/// The days of the week by name, Monday first.
pub const WEEKDAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Monday),
    ("tuesday", Weekday::Tuesday),
    ("wednesday", Weekday::Wednesday),
    ("thursday", Weekday::Thursday),
    ("friday", Weekday::Friday),
    ("saturday", Weekday::Saturday),
    ("sunday", Weekday::Sunday),
];

/// How long a first ingest runs before it shows its progress.
const QUIET_FOR: Duration = Duration::from_millis(300);

/// The shared setting that holds the IANA time zone days are counted in; without it, this
/// computer's.
pub const TIME_ZONE_SETTING: &str = "time_zone";

/// The shared setting that holds the first day of the week, such as `monday`.
pub const WEEK_START_SETTING: &str = "week_start";

#[derive(Clone)]
pub struct App {
    pub history_path: Option<PathBuf>,
    pub roots: Vec<DataRoot>,
    /// The value of `CLAUDE_CONFIG_DIR`.
    pub config_dir: Option<OsString>,
    pub home: Option<PathBuf>,
    /// The time zone days are counted in.
    pub zone: TimeZone,
    /// This computer's time zone.
    pub system_zone: TimeZone,
    /// The first day of the week.
    pub week_start: Weekday,
    pub now: Timestamp,
    pub prices: &'static PriceTable,
    /// Whether a status line connected from here runs a copy of this program kept next to the
    /// history, because an update moves this one (`serve --status-line-copy`).
    pub status_line_copy: bool,
}

impl App {
    pub fn new() -> Self {
        let history_path = std::env::var_os(HISTORY_ENV)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(history::default_path);
        let config_dir = std::env::var_os(CONFIG_DIR_ENV);
        let home = std::env::home_dir();
        let zone = TimeZone::system();
        Self {
            history_path,
            roots: roots::data_roots(config_dir.as_deref(), home.as_deref(), &[]),
            config_dir,
            home,
            system_zone: zone.clone(),
            zone,
            week_start: Weekday::Monday,
            now: now(),
            prices: PriceTable::bundled(),
            status_line_copy: false,
        }
    }

    /// The app with the settings every surface shares, kept in the history: the data folders the
    /// person added, the time zone and the first day of the week. Without a history yet, there
    /// is nothing to apply.
    pub fn with_shared_settings(self) -> Self {
        match &self.history_path {
            Some(path) if path.is_file() => match History::open(path) {
                Ok(history) => self.with_shared(&history),
                Err(_) => self,
            },
            _ => self,
        }
    }

    /// The app with the settings `history` holds for every surface.
    pub fn with_shared(&self, history: &History) -> Self {
        let mut app = self.clone();
        if let Ok(folders) = history.data_folders() {
            app.roots = roots::data_roots(app.config_dir.as_deref(), app.home.as_deref(), &folders);
        }
        let settings = history.settings().unwrap_or_default();
        app.zone = settings
            .get(TIME_ZONE_SETTING)
            .and_then(|name| TimeZone::get(name).ok())
            .unwrap_or_else(|| app.system_zone.clone());
        app.week_start = settings
            .get(WEEK_START_SETTING)
            .and_then(|name| weekday(name))
            .unwrap_or(Weekday::Monday);
        app
    }

    /// The first day of the week `day` falls in.
    pub fn start_of_week(&self, day: Date) -> Date {
        let back = (7 + day.weekday().to_monday_zero_offset()
            - self.week_start.to_monday_zero_offset())
            % 7;
        day.checked_sub(i64::from(back).days()).unwrap_or(day)
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
