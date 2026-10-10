//! The history: every project, session and request recorded, in one SQLite database that every
//! surface opens at the same time (ADR 0002).
//!
//! The database holds numbers, names and titles, never prompt or code content.

mod lock;
mod migrations;
mod writer;

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::{Connection, ErrorCode, OpenFlags, TransactionBehavior};

pub use lock::IngestLock;
pub use writer::{
    Attribution, Identity, ProjectKey, ReadPosition, Request, SessionKey, SessionTitle, Writer,
};

use migrations::{MIGRATIONS, Schema};

/// How long a write waits while another process writes.
const BUSY_TIMEOUT: Duration = Duration::from_secs(30);

/// Where the history lives unless the caller chooses another place.
///
/// That is the operating system's folder for local app data, never a data root:
/// `%LOCALAPPDATA%\claude-dashboard` on Windows, `~/Library/Application Support/claude-dashboard` on
/// macOS, and `$XDG_DATA_HOME/claude-dashboard` (by default `~/.local/share/claude-dashboard`) on
/// Linux. `None` when the operating system names no such folder.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|folder| folder.join("claude-dashboard").join("history.sqlite"))
}

/// What this process may do with the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Read and record.
    ReadWrite,
    /// Read only, because a newer claude-dashboard has upgraded the history to `schema`.
    ReadOnly { schema: u32 },
}

/// Why the history cannot be opened, read or written.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("cannot use {}: {source}", path.display())]
    File { path: PathBuf, source: io::Error },
    #[error("{} is not a claude-dashboard history", path.display())]
    NotAHistory { path: PathBuf },
    #[error(
        "SQLite cannot keep a write-ahead log for {} (journal mode \"{mode}\"); the history must be on a local disk",
        path.display()
    )]
    WalUnavailable { path: PathBuf, mode: String },
    #[error(
        "a newer claude-dashboard has upgraded the history (schema {schema}); update claude-dashboard to read it"
    )]
    TooNew { schema: u32 },
    #[error(
        "a newer claude-dashboard has upgraded the history (schema {schema}); update claude-dashboard to keep recording"
    )]
    ReadOnly { schema: u32 },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

/// How many projects, sessions and requests the history holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub projects: u64,
    pub sessions: u64,
    pub requests: u64,
}

/// The history, open in this process.
#[derive(Debug)]
pub struct History {
    conn: Connection,
    path: PathBuf,
    access: Access,
}

impl History {
    /// Opens the history at `path`, creating it and its folder when missing and upgrading it when
    /// an older claude-dashboard created it.
    ///
    /// # Errors
    ///
    /// [`HistoryError::NotAHistory`] for an SQLite file that claude-dashboard did not create,
    /// [`HistoryError::WalUnavailable`] where SQLite cannot keep a write-ahead log,
    /// [`HistoryError::TooNew`] when the history needs a newer claude-dashboard to be read, and
    /// file or SQLite errors.
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        if let Some(folder) = path
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
        {
            fs::create_dir_all(folder).map_err(|source| HistoryError::File {
                path: folder.to_owned(),
                source,
            })?;
        }
        let mut conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        migrations::check_owner(&conn, path)?;
        enable_wal(&conn, path)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let access = migrations::migrate(&mut conn, path, MIGRATIONS)?;
        Ok(Self {
            conn,
            path: path.to_owned(),
            access,
        })
    }

    /// The database file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What this process may do with the history.
    #[must_use]
    pub fn access(&self) -> Access {
        self.access
    }

    /// Records in one transaction: `record` gets a [`Writer`], and nothing it records is kept
    /// unless it succeeds.
    ///
    /// # Errors
    ///
    /// [`HistoryError::ReadOnly`] or [`HistoryError::TooNew`] once a newer claude-dashboard has
    /// upgraded the history, the error `record` returns, and SQLite errors.
    pub fn write<T>(
        &mut self,
        record: impl FnOnce(&mut Writer<'_>) -> Result<T, HistoryError>,
    ) -> Result<T, HistoryError> {
        loop {
            if let Access::ReadOnly { schema } = self.access {
                return Err(HistoryError::ReadOnly { schema });
            }
            let tx = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            if Schema::read(&tx)?.version == migrations::latest(MIGRATIONS) {
                let mut writer = Writer::new(tx);
                let value = record(&mut writer)?;
                writer.commit()?;
                return Ok(value);
            }
            drop(tx);
            // Another claude-dashboard has changed the schema since this process opened the history.
            self.access = migrations::migrate(&mut self.conn, &self.path, MIGRATIONS)?;
        }
    }

    /// Takes the ingest lock, so that this process is the only one ingesting.
    ///
    /// Returns `None` while another process holds it.
    ///
    /// # Errors
    ///
    /// [`HistoryError::ReadOnly`] when this process may not record, and errors creating or locking
    /// the lock file.
    pub fn try_lock_ingest(&self) -> Result<Option<IngestLock>, HistoryError> {
        if let Access::ReadOnly { schema } = self.access {
            return Err(HistoryError::ReadOnly { schema });
        }
        IngestLock::try_acquire(&self.path.with_extension("ingest.lock"))
    }

    /// How many projects, sessions and requests the history holds.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn counts(&self) -> Result<Counts, HistoryError> {
        let counts = self.conn.query_row(
            "SELECT (SELECT count(*) FROM projects), (SELECT count(*) FROM sessions), \
             (SELECT count(*) FROM requests)",
            [],
            |row| {
                Ok(Counts {
                    projects: row.get(0)?,
                    sessions: row.get(1)?,
                    requests: row.get(2)?,
                })
            },
        )?;
        Ok(counts)
    }

    /// Runs SQLite's integrity check and returns the problems it reports; none when the database
    /// is sound.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn integrity_check(&self) -> Result<Vec<String>, HistoryError> {
        let mut statement = self.conn.prepare("PRAGMA integrity_check")?;
        let problems = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(if problems == ["ok"] {
            Vec::new()
        } else {
            problems
        })
    }

    /// How far each transcript has been read, by its path.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn read_positions(&self) -> Result<HashMap<String, ReadPosition>, HistoryError> {
        let mut statement = self
            .conn
            .prepare("SELECT path, read_to, len, modified_ms FROM transcripts")?;
        let positions = statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    ReadPosition {
                        read_to: row.get(1)?,
                        len: row.get(2)?,
                        modified_ms: row.get(3)?,
                    },
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok(positions)
    }
}

/// Switches the database to write-ahead logging, which lets readers and a writer work at once.
fn enable_wal(conn: &Connection, path: &Path) -> Result<(), HistoryError> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        match conn
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))
        {
            Ok(mode) if mode.eq_ignore_ascii_case("wal") => return Ok(()),
            Ok(mode) => {
                return Err(HistoryError::WalUnavailable {
                    path: path.to_owned(),
                    mode,
                });
            }
            // When processes switch a new history at the same moment, waiting could deadlock, so
            // SQLite fails all but one of them at once instead of waiting; those try again.
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == ErrorCode::DatabaseBusy && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_ahead_logging_is_required() {
        let conn = Connection::open_in_memory().unwrap();
        let error = enable_wal(&conn, Path::new(":memory:")).unwrap_err();
        assert!(
            matches!(&error, HistoryError::WalUnavailable { mode, .. } if mode == "memory"),
            "{error:?}"
        );
    }
}
