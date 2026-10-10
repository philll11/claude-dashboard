//! The history's schema, and upgrading it from one version to the next.

use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, TransactionBehavior};

use super::{Access, BUSY_TIMEOUT, HistoryError};

/// Marks an SQLite file as a claude-dashboard history.
const APPLICATION_ID: i32 = i32::from_be_bytes(*b"CDHI");

/// One step from a schema version to the next.
pub(super) struct Migration {
    pub(super) sql: &'static str,
    /// Set when binaries that know only the earlier versions can no longer read the history once
    /// this step has run, because it removes or renames something they read or changes what a
    /// stored value means. Adding tables, columns or indexes leaves them able to read it.
    pub(super) breaks_older_readers: bool,
}

/// Every migration in order; schema version N is the result of the first N.
///
/// A migration that a release has shipped never changes: add a new one instead. An upgrade runs
/// in one transaction with foreign keys enforced, which SQLite cannot switch off inside a
/// transaction, so a step that rebuilds a table keeps every reference valid throughout.
pub(super) const MIGRATIONS: &[Migration] = &[Migration {
    sql: include_str!("migrations/0001-projects-sessions-requests.sql"),
    breaks_older_readers: false,
}];

/// The schema version that a list of migrations leads to.
pub(super) fn latest(migrations: &[Migration]) -> u32 {
    u32::try_from(migrations.len()).expect("fewer than 2^32 migrations")
}

/// The schema version a history has, and the oldest version that can still read it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Schema {
    pub(super) version: u32,
    readable_from: u32,
}

impl Schema {
    /// Reads the version and the oldest readable version in two statements, so call it inside a
    /// transaction to get both from the same moment.
    pub(super) fn read(conn: &Connection) -> Result<Self, HistoryError> {
        let version = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let readable_from = if version == 0 {
            0
        } else {
            conn.query_row("SELECT readable_from FROM compatibility", [], |row| {
                row.get(0)
            })?
        };
        Ok(Self {
            version,
            readable_from,
        })
    }

    /// What a binary that knows the versions up to `latest` may do, or `None` when the history
    /// needs upgrading first.
    fn access(self, latest: u32) -> Result<Option<Access>, HistoryError> {
        match self.version.cmp(&latest) {
            Ordering::Less => Ok(None),
            Ordering::Equal => Ok(Some(Access::ReadWrite)),
            Ordering::Greater if self.readable_from <= latest => Ok(Some(Access::ReadOnly {
                schema: self.version,
            })),
            Ordering::Greater => Err(HistoryError::TooNew {
                schema: self.version,
            }),
        }
    }
}

/// Refuses an SQLite file that claude-dashboard did not create.
pub(super) fn check_owner(conn: &Connection, path: &Path) -> Result<(), HistoryError> {
    // One statement, so both values come from the same moment while another process creates the
    // history.
    let (application_id, objects): (i32, i64) = conn.query_row(
        "SELECT application_id, (SELECT count(*) FROM sqlite_schema) FROM pragma_application_id",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if application_id == APPLICATION_ID || (application_id == 0 && objects == 0) {
        Ok(())
    } else {
        Err(HistoryError::NotAHistory {
            path: path.to_owned(),
        })
    }
}

/// Upgrades the history to the latest schema version this binary knows, and says what this
/// process may do with it.
pub(super) fn migrate(
    conn: &mut Connection,
    path: &Path,
    migrations: &[Migration],
) -> Result<Access, HistoryError> {
    let latest = latest(migrations);
    let schema = {
        let snapshot = conn.transaction()?;
        Schema::read(&snapshot)?
    };
    if let Some(access) = schema.access(latest)? {
        return Ok(access);
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // Another process may have upgraded the history while this one waited for the write lock.
    let schema = Schema::read(&tx)?;
    if let Some(access) = schema.access(latest)? {
        return Ok(access);
    }
    if schema.version > 0 {
        back_up(path)?;
    }
    for (version, migration) in (1..=latest).zip(migrations) {
        if version <= schema.version {
            continue;
        }
        tx.execute_batch(migration.sql)?;
        if migration.breaks_older_readers {
            tx.execute("UPDATE compatibility SET readable_from = ?1", [version])?;
        }
    }
    tx.pragma_update(None, "user_version", latest)?;
    tx.pragma_update(None, "application_id", APPLICATION_ID)?;
    tx.commit()?;
    Ok(Access::ReadWrite)
}

/// Copies the history before an upgrade changes it, because it holds requests whose transcripts
/// Claude Code has deleted. The copy replaces the one taken before the previous upgrade.
fn back_up(path: &Path) -> Result<(), HistoryError> {
    let backup = backup_path(path);
    let file_error = |source| HistoryError::File {
        path: backup.clone(),
        source,
    };
    match fs::remove_file(&backup) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(file_error(error)),
        _ => {}
    }
    let target = backup.to_str().ok_or_else(|| {
        file_error(io::Error::new(
            io::ErrorKind::InvalidFilename,
            "the path is not valid Unicode",
        ))
    })?;
    // The caller holds the write lock, so this second connection copies the last committed state.
    let source = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    source.busy_timeout(BUSY_TIMEOUT)?;
    source.execute("VACUUM INTO ?1", [target])?;
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("backup.sqlite")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CREATE: Migration = Migration {
        sql: "CREATE TABLE compatibility (
                  id INTEGER PRIMARY KEY CHECK (id = 1),
                  readable_from INTEGER NOT NULL
              ) STRICT;
              INSERT INTO compatibility (id, readable_from) VALUES (1, 1);
              CREATE TABLE notes (body TEXT NOT NULL) STRICT;",
        breaks_older_readers: false,
    };
    const ADD_COLUMN: Migration = Migration {
        sql: "ALTER TABLE notes ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;",
        breaks_older_readers: false,
    };
    const RENAME_COLUMN: Migration = Migration {
        sql: "ALTER TABLE notes RENAME COLUMN body TO text;",
        breaks_older_readers: true,
    };

    fn connect(path: &Path) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.busy_timeout(BUSY_TIMEOUT).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn
    }

    fn schema(path: &Path) -> (u32, u32) {
        let schema = Schema::read(&Connection::open(path).unwrap()).unwrap();
        (schema.version, schema.readable_from)
    }

    #[test]
    fn a_new_history_takes_every_step_without_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");

        let access = migrate(
            &mut connect(&path),
            &path,
            &[CREATE, ADD_COLUMN, RENAME_COLUMN],
        )
        .unwrap();

        assert_eq!(access, Access::ReadWrite);
        assert_eq!(schema(&path), (3, 3));
        assert!(!backup_path(&path).exists());
    }

    #[test]
    fn an_upgrade_backs_up_the_history_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        let mut conn = connect(&path);
        migrate(&mut conn, &path, &[CREATE]).unwrap();
        conn.execute("INSERT INTO notes (body) VALUES ('kept')", [])
            .unwrap();

        migrate(&mut conn, &path, &[CREATE, ADD_COLUMN]).unwrap();

        assert_eq!(schema(&path), (2, 1));
        let backup = Connection::open(backup_path(&path)).unwrap();
        assert_eq!(Schema::read(&backup).unwrap().version, 1);
        let body: String = backup
            .query_row("SELECT body FROM notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(body, "kept");
    }

    #[test]
    fn an_up_to_date_history_is_left_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        let mut conn = connect(&path);
        migrate(&mut conn, &path, &[CREATE]).unwrap();
        migrate(&mut conn, &path, &[CREATE, ADD_COLUMN]).unwrap();
        fs::remove_file(backup_path(&path)).unwrap();

        let access = migrate(&mut conn, &path, &[CREATE, ADD_COLUMN]).unwrap();

        assert_eq!(access, Access::ReadWrite);
        assert_eq!(schema(&path), (2, 1));
        assert!(!backup_path(&path).exists());
    }

    #[test]
    fn older_binaries_read_a_newer_history_until_a_step_breaks_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        let mut conn = connect(&path);

        migrate(&mut conn, &path, &[CREATE, ADD_COLUMN]).unwrap();
        assert_eq!(
            migrate(&mut conn, &path, &[CREATE]).unwrap(),
            Access::ReadOnly { schema: 2 }
        );

        migrate(&mut conn, &path, &[CREATE, ADD_COLUMN, RENAME_COLUMN]).unwrap();
        let error = migrate(&mut conn, &path, &[CREATE, ADD_COLUMN]).unwrap_err();
        assert!(
            matches!(error, HistoryError::TooNew { schema: 3 }),
            "{error:?}"
        );
    }

    #[test]
    fn only_an_empty_database_or_a_history_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        let conn = connect(&path);
        check_owner(&conn, &path).unwrap();

        conn.execute_batch("CREATE TABLE notes (body TEXT)")
            .unwrap();
        let error = check_owner(&conn, &path).unwrap_err();
        assert!(
            matches!(error, HistoryError::NotAHistory { .. }),
            "{error:?}"
        );
    }
}
