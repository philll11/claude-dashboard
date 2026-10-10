//! Import: one-time transfers into the history from other sources.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, params};

use crate::history::{
    Attribution, History, HistoryError, Identity, ProjectKey, Request, SessionKey, SessionTitle,
    Writer,
};
use crate::tokens::Tokens;

/// The source that requests imported from the claude-usage dashboard are recorded under.
pub const CLAUDE_USAGE: &str = "claude-usage";

/// How many of the source's rows one transaction imports.
const BATCH: i64 = 2_000;

/// The columns of claude-usage's `turns` table that the import reads.
const TURN_COLUMNS: [&str; 10] = [
    "id",
    "session_id",
    "timestamp",
    "model",
    "input_tokens",
    "output_tokens",
    "cache_read_tokens",
    "cache_creation_tokens",
    "cwd",
    "message_id",
];

/// Why an import failed.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("{} is not a claude-usage database", path.display())]
    NotClaudeUsage { path: PathBuf },
    #[error("cannot read {}: {source}", path.display())]
    Source {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("another claude-dashboard is reading transcripts; try again once it has finished")]
    Busy,
    #[error(transparent)]
    History(#[from] HistoryError),
}

/// What an import did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Requests added to the history.
    pub imported: u64,
    /// Requests the history already held, from transcripts or an earlier import.
    pub already_held: u64,
    /// Rows without the time, model or message id that a request needs.
    pub unreadable: u64,
}

/// What an import would add, before it runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportPreview {
    /// Requests the history does not hold yet.
    pub requests: u64,
    /// When the first and last of them were made, in milliseconds since the Unix epoch.
    pub span_ms: Option<(i64, i64)>,
}

struct Turn {
    id: i64,
    session_id: String,
    timestamp: String,
    model: String,
    tokens: Tokens,
    cwd: Option<String>,
    message_id: String,
    subagent_id: Option<String>,
}

/// What the import knows about the history, read once while it holds the ingest lock.
#[derive(Default)]
struct Known {
    /// Message ids the history holds with a request id, in any session.
    with_request_id: HashSet<String>,
    /// Message ids the history holds without a request id, by session.
    in_session: HashMap<SessionKey, HashSet<String>>,
    sessions: HashMap<String, SessionKey>,
    projects: HashMap<Option<String>, ProjectKey>,
    /// Sessions whose topic the import has already offered as a title.
    titled: HashSet<SessionKey>,
}

impl Known {
    fn read(history: &History) -> Result<Self, HistoryError> {
        let mut known = Self::default();
        for held in history.message_ids()? {
            if held.with_request_id {
                known.with_request_id.insert(held.message_id);
            } else {
                known
                    .in_session
                    .entry(held.session)
                    .or_default()
                    .insert(held.message_id);
            }
        }
        known.sessions = history
            .sessions()?
            .into_iter()
            .map(|session| (session.session_id, session.key))
            .collect();
        Ok(known)
    }

    fn holds(&self, session: SessionKey, message_id: &str) -> bool {
        self.with_request_id.contains(message_id)
            || self
                .in_session
                .get(&session)
                .is_some_and(|ids| ids.contains(message_id))
    }
}

/// Imports the requests that the claude-usage dashboard recorded in its database at `database`,
/// leaving out the requests the history already holds.
///
/// claude-usage kept one row per message, without the request id, and did not tell five-minute
/// and one-hour cache writes apart, so every imported cache write counts as five minutes. A
/// transcript read later replaces an imported request for the same message.
///
/// # Errors
///
/// [`ImportError::NotClaudeUsage`] when `database` lacks claude-usage's tables,
/// [`ImportError::Busy`] while another process is ingesting, and errors reading `database` or
/// writing the history.
pub fn import_claude_usage(
    history: &mut History,
    database: &Path,
    now_ms: i64,
) -> Result<ImportReport, ImportError> {
    let source_error = |source| ImportError::Source {
        path: database.to_owned(),
        source,
    };
    let source = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(source_error)?;
    let columns = table_columns(&source, "turns").map_err(source_error)?;
    if !TURN_COLUMNS.iter().all(|column| columns.contains(*column)) {
        return Err(ImportError::NotClaudeUsage {
            path: database.to_owned(),
        });
    }
    let has_agent_id = columns.contains("agent_id");
    let topics = topics(&source).map_err(source_error)?;
    let started_in = started_in(&source, columns.contains("is_subagent")).map_err(source_error)?;

    // No ingest may add the same messages while the import checks for them.
    let Some(_lock) = history.try_lock_ingest()? else {
        return Err(ImportError::Busy);
    };
    let mut known = Known::read(history)?;
    // Recorded before any request, so that an ingest replaces imported requests even when the
    // import stops part-way.
    history.write(|w| w.record_import(CLAUDE_USAGE, now_ms, 0))?;

    let mut report = ImportReport::default();
    let mut after = i64::MIN;
    loop {
        let turns = read_turns(&source, after, has_agent_id).map_err(source_error)?;
        let Some(last) = turns.last() else {
            break;
        };
        after = last.id;
        let imported_before = report.imported;
        history.write(|w| {
            for turn in &turns {
                import_turn(w, turn, &topics, &started_in, &mut known, &mut report)?;
            }
            w.record_import(CLAUDE_USAGE, now_ms, report.imported - imported_before)
        })?;
    }
    Ok(report)
}

/// What importing the claude-usage database at `database` would add: the requests the history
/// does not hold yet. Nothing is written.
///
/// # Errors
///
/// [`ImportError::NotClaudeUsage`] when `database` lacks claude-usage's tables, and errors
/// reading `database` or the history.
pub fn preview_claude_usage(
    history: &History,
    database: &Path,
) -> Result<ImportPreview, ImportError> {
    let source_error = |source| ImportError::Source {
        path: database.to_owned(),
        source,
    };
    let source = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(source_error)?;
    let columns = table_columns(&source, "turns").map_err(source_error)?;
    if !TURN_COLUMNS.iter().all(|column| columns.contains(*column)) {
        return Err(ImportError::NotClaudeUsage {
            path: database.to_owned(),
        });
    }
    let has_agent_id = columns.contains("agent_id");
    let known = Known::read(history)?;
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut preview = ImportPreview::default();
    let mut after = i64::MIN;
    loop {
        let turns = read_turns(&source, after, has_agent_id).map_err(source_error)?;
        let Some(last) = turns.last() else {
            break;
        };
        after = last.id;
        for turn in turns {
            let Ok(timestamp) = turn.timestamp.parse::<jiff::Timestamp>() else {
                continue;
            };
            if turn.model.is_empty() || turn.message_id.is_empty() {
                continue;
            }
            let held = known
                .sessions
                .get(&turn.session_id)
                .is_some_and(|&session| known.holds(session, &turn.message_id))
                || known.with_request_id.contains(&turn.message_id);
            if held || !seen.insert((turn.session_id, turn.message_id)) {
                continue;
            }
            let ms = timestamp.as_millisecond();
            preview.requests += 1;
            preview.span_ms = Some(
                preview
                    .span_ms
                    .map_or((ms, ms), |(first, last)| (first.min(ms), last.max(ms))),
            );
        }
    }
    Ok(preview)
}

fn import_turn(
    w: &mut Writer<'_>,
    turn: &Turn,
    topics: &HashMap<String, String>,
    started_in: &HashMap<String, String>,
    known: &mut Known,
    report: &mut ImportReport,
) -> Result<(), HistoryError> {
    let Ok(timestamp) = turn.timestamp.parse::<jiff::Timestamp>() else {
        report.unreadable += 1;
        return Ok(());
    };
    if turn.model.is_empty() || turn.message_id.is_empty() {
        report.unreadable += 1;
        return Ok(());
    }
    let session = if let Some(session) = known.sessions.get(&turn.session_id) {
        *session
    } else {
        // Claude Code files a session under the folder it started in, however often its
        // requests change folder later.
        let cwd = started_in
            .get(&turn.session_id)
            .cloned()
            .or_else(|| turn.cwd.clone());
        let project = if let Some(project) = known.projects.get(&cwd) {
            *project
        } else {
            let project = match cwd.as_deref() {
                Some(cwd) => match w.project_with_path(cwd)? {
                    Some(project) => project,
                    None => w.project(&transcript_folder(cwd), Some(cwd))?,
                },
                None => w.project("unknown", None)?,
            };
            known.projects.insert(cwd, project);
            project
        };
        let session = w.session(&turn.session_id, project)?;
        known.sessions.insert(turn.session_id.clone(), session);
        session
    };
    if known.titled.insert(session)
        && let Some(topic) = topics.get(&turn.session_id)
    {
        w.session_title_if_none(session, SessionTitle::Ai(topic))?;
    }
    if known.holds(session, &turn.message_id) {
        report.already_held += 1;
        return Ok(());
    }
    let record_id = format!("{CLAUDE_USAGE}:{}", turn.id);
    w.request(&Request {
        session,
        identity: Identity::MessageInSession {
            message_id: &turn.message_id,
        },
        record_id: &record_id,
        subagent_id: turn.subagent_id.as_deref(),
        timestamp_ms: timestamp.as_millisecond(),
        model: &turn.model,
        tokens: turn.tokens,
        web_search_requests: 0,
        web_fetch_requests: 0,
        speed: None,
        service_tier: None,
        inference_geo: None,
        attribution: Attribution::default(),
        entrypoint: None,
        claude_code_version: None,
        imported_from: Some(CLAUDE_USAGE),
    })?;
    known
        .in_session
        .entry(session)
        .or_default()
        .insert(turn.message_id.clone());
    report.imported += 1;
    Ok(())
}

/// The folder Claude Code keeps a working folder's transcripts in: the path with every character
/// other than an ASCII letter or digit replaced by `-`.
fn transcript_folder(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn table_columns(conn: &Connection, table: &str) -> rusqlite::Result<HashSet<String>> {
    let mut statement = conn.prepare("SELECT name FROM pragma_table_info(?1)")?;
    statement.query_map([table], |row| row.get(0))?.collect()
}

/// The folder each session started in: where its first request of its own ran, or else its
/// first subagent's.
fn started_in(
    conn: &Connection,
    has_is_subagent: bool,
) -> rusqlite::Result<HashMap<String, String>> {
    let own = if has_is_subagent {
        "coalesce(is_subagent, 0) = 0"
    } else {
        "1"
    };
    let mut statement = conn.prepare(&format!(
        "SELECT session_id, cwd, {own} FROM turns
         WHERE session_id IS NOT NULL AND cwd IS NOT NULL AND cwd <> ''
         ORDER BY id"
    ))?;
    let mut first: HashMap<String, (String, bool)> = HashMap::new();
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let (session, cwd, own): (String, String, bool) = (row.get(0)?, row.get(1)?, row.get(2)?);
        match first.get_mut(&session) {
            Some(found) if !found.1 && own => *found = (cwd, true),
            Some(_) => {}
            None => {
                first.insert(session, (cwd, own));
            }
        }
    }
    Ok(first
        .into_iter()
        .map(|(session, (cwd, _))| (session, cwd))
        .collect())
}

/// claude-usage's session topics, which are titles.
fn topics(conn: &Connection) -> rusqlite::Result<HashMap<String, String>> {
    if !table_columns(conn, "sessions")?.contains("topic") {
        return Ok(HashMap::new());
    }
    let mut statement = conn.prepare(
        "SELECT session_id, topic FROM sessions
         WHERE session_id IS NOT NULL AND topic IS NOT NULL AND topic <> ''",
    )?;
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect()
}

fn read_turns(conn: &Connection, after: i64, has_agent_id: bool) -> rusqlite::Result<Vec<Turn>> {
    let agent = if has_agent_id { "agent_id" } else { "NULL" };
    let mut statement = conn.prepare_cached(&format!(
        "SELECT id, session_id, timestamp, model, input_tokens, output_tokens, cache_read_tokens,
             cache_creation_tokens, cwd, message_id, {agent}
         FROM turns WHERE id > ?1 ORDER BY id LIMIT ?2"
    ))?;
    statement
        .query_map(params![after, BATCH], |row| {
            let count = |index: usize| -> rusqlite::Result<u64> {
                Ok(row
                    .get::<_, Option<i64>>(index)?
                    .and_then(|value| u64::try_from(value).ok())
                    .unwrap_or(0))
            };
            Ok(Turn {
                id: row.get(0)?,
                session_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                timestamp: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                model: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                tokens: Tokens {
                    input: count(4)?,
                    output: count(5)?,
                    cache_read: count(6)?,
                    cache_write_5m: count(7)?,
                    cache_write_1h: 0,
                },
                cwd: row
                    .get::<_, Option<String>>(8)?
                    .filter(|cwd| !cwd.is_empty()),
                message_id: row.get::<_, Option<String>>(9)?.unwrap_or_default(),
                subagent_id: row
                    .get::<_, Option<String>>(10)?
                    .filter(|id| !id.is_empty()),
            })
        })?
        .collect()
}

/// Finds the claude-usage database in a data root, where claude-usage keeps it.
#[must_use]
pub fn claude_usage_database(root: &Path) -> Option<PathBuf> {
    let path = root.join("usage.db");
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::transcript_folder;

    #[test]
    fn working_folders_map_to_claude_codes_transcript_folders() {
        assert_eq!(
            transcript_folder(r"C:\Users\ada\Documents\Development\ClaudeDashboard"),
            "C--Users-ada-Documents-Development-ClaudeDashboard"
        );
        assert_eq!(
            transcript_folder("/home/ada/code/my_app.v2"),
            "-home-ada-code-my-app-v2"
        );
    }
}
