//! Reading the history back, for reports and reconciliation.

use rusqlite::OptionalExtension;
use rusqlite::types::ValueRef;

use super::{History, HistoryError, ProjectKey, SessionKey};
use crate::limits::{LimitSnapshot, Source};
use crate::tokens::Tokens;
use crate::usage_endpoint::{Outcome, State};

/// A request as the history holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredRequest<'a> {
    pub session: SessionKey,
    pub project: ProjectKey,
    /// The subagent that made the request; `None` for the session itself.
    pub subagent_id: Option<&'a str>,
    /// When the request was made, in milliseconds since the Unix epoch.
    pub timestamp_ms: i64,
    pub model: &'a str,
    pub tokens: Tokens,
    pub web_search_requests: u64,
    pub speed: Option<&'a str>,
    pub inference_geo: Option<&'a str>,
    pub skill: Option<&'a str>,
    pub mcp_server: Option<&'a str>,
    pub mcp_tool: Option<&'a str>,
    pub entrypoint: Option<&'a str>,
    pub claude_code_version: Option<&'a str>,
}

/// A session as the history holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    pub key: SessionKey,
    pub session_id: String,
    pub project: ProjectKey,
    /// The custom title, or else the AI title.
    pub title: Option<String>,
}

/// A project as the history holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProject {
    pub key: ProjectKey,
    /// The folder in a data root's projects folder that holds the project's transcripts.
    pub transcript_folder: String,
    /// The working folder, once a transcript has named it.
    pub path: Option<String>,
}

/// A message id the history holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldMessage {
    pub session: SessionKey,
    pub message_id: String,
    /// Whether a request id goes with it.
    pub with_request_id: bool,
}

/// A one-time import the history holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredImport {
    /// When the import last ran, in milliseconds since the Unix epoch.
    pub imported_ms: i64,
    /// Requests it added.
    pub requests: u64,
}

/// What Claude Code reports a session cost for one model, summed over its runs.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredReportedCost {
    pub session: SessionKey,
    pub model: String,
    pub cost_usd: f64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    /// Cache writes of both durations.
    pub cache_write: u64,
}

/// The columns [`each_request`] reads, in its order.
macro_rules! request_columns {
    () => {
        "requests.session, sessions.project, subagent_id, timestamp_ms, model, input_tokens,
         output_tokens, cache_read_tokens, cache_write_5m_tokens, cache_write_1h_tokens,
         web_search_requests, speed, inference_geo, skill, mcp_server, mcp_tool, entrypoint,
         claude_code_version"
    };
}

/// Calls `each` for every row of a query that selects [`request_columns`].
fn each_request(
    rows: &mut rusqlite::Rows<'_>,
    mut each: impl FnMut(&StoredRequest<'_>),
) -> Result<(), HistoryError> {
    while let Some(row) = rows.next()? {
        let text = |index: usize| -> Result<Option<&str>, HistoryError> {
            Ok(match row.get_ref(index)? {
                ValueRef::Text(text) => std::str::from_utf8(text).ok(),
                _ => None,
            })
        };
        each(&StoredRequest {
            session: SessionKey(row.get(0)?),
            project: ProjectKey(row.get(1)?),
            subagent_id: text(2)?,
            timestamp_ms: row.get(3)?,
            model: text(4)?.unwrap_or_default(),
            tokens: Tokens {
                input: row.get(5)?,
                output: row.get(6)?,
                cache_read: row.get(7)?,
                cache_write_5m: row.get(8)?,
                cache_write_1h: row.get(9)?,
            },
            web_search_requests: row.get(10)?,
            speed: text(11)?,
            inference_geo: text(12)?,
            skill: text(13)?,
            mcp_server: text(14)?,
            mcp_tool: text(15)?,
            entrypoint: text(16)?,
            claude_code_version: text(17)?,
        });
    }
    Ok(())
}

impl History {
    /// Calls `each` for every request made from `from_ms` up to, but not including, `to_ms`,
    /// oldest first.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn for_each_request(
        &self,
        from_ms: i64,
        to_ms: i64,
        each: impl FnMut(&StoredRequest<'_>),
    ) -> Result<(), HistoryError> {
        let mut statement = self.conn.prepare_cached(concat!(
            "SELECT ",
            request_columns!(),
            " FROM requests JOIN sessions ON sessions.id = requests.session
             WHERE timestamp_ms >= ?1 AND timestamp_ms < ?2
             ORDER BY timestamp_ms"
        ))?;
        each_request(&mut statement.query([from_ms, to_ms])?, each)
    }

    /// Calls `each` for every request of one session, its subagents' included, oldest first.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn for_each_request_in_session(
        &self,
        session: SessionKey,
        each: impl FnMut(&StoredRequest<'_>),
    ) -> Result<(), HistoryError> {
        let mut statement = self.conn.prepare_cached(concat!(
            "SELECT ",
            request_columns!(),
            " FROM requests JOIN sessions ON sessions.id = requests.session
             WHERE requests.session = ?1
             ORDER BY timestamp_ms"
        ))?;
        each_request(&mut statement.query([session.0])?, each)
    }

    /// A number that changes whenever another connection, in this process or another, commits a
    /// change to the history (SQLite's `data_version`).
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn data_version(&self) -> Result<i64, HistoryError> {
        Ok(self
            .conn
            .pragma_query_value(None, "data_version", |row| row.get(0))?)
    }

    /// How many requests were made from `since_ms` on, their output tokens, and the time of the
    /// latest: together they change when requests arrive or a streamed one is completed.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn recent_requests(&self, since_ms: i64) -> Result<(u64, u64, Option<i64>), HistoryError> {
        Ok(self
            .conn
            .prepare_cached(
                "SELECT count(*), coalesce(sum(output_tokens), 0), max(timestamp_ms)
                 FROM requests WHERE timestamp_ms >= ?1",
            )?
            .query_row([since_ms], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?)
    }

    /// When the history's first request was made.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn first_request_ms(&self) -> Result<Option<i64>, HistoryError> {
        Ok(self
            .conn
            .prepare_cached("SELECT min(timestamp_ms) FROM requests")?
            .query_row([], |row| row.get(0))?)
    }

    /// When the newest plan-limit reading was taken, from any source.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn latest_limit_ms(&self) -> Result<Option<i64>, HistoryError> {
        Ok(self
            .conn
            .prepare_cached("SELECT max(observed_ms) FROM limit_snapshots")?
            .query_row([], |row| row.get(0))?)
    }

    /// Every reading of one plan window from `from_ms` up to, but not including, `to_ms`, from
    /// any source, oldest first.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn limit_snapshots(
        &self,
        window: &str,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<LimitSnapshot>, HistoryError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT source, observed_ms, used_percentage, resets_at_ms FROM limit_snapshots
             WHERE window = ?1 AND observed_ms >= ?2 AND observed_ms < ?3
             ORDER BY observed_ms",
        )?;
        let mut rows = statement.query(rusqlite::params![window, from_ms, to_ms])?;
        let mut snapshots = Vec::new();
        while let Some(row) = rows.next()? {
            if let Some(source) = Source::from_name(&row.get::<_, String>(0)?) {
                snapshots.push(LimitSnapshot {
                    window: window.to_owned(),
                    source,
                    observed_ms: row.get(1)?,
                    used_percentage: row.get(2)?,
                    resets_at_ms: row.get(3)?,
                });
            }
        }
        Ok(snapshots)
    }

    /// Every session the history holds.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn sessions(&self) -> Result<Vec<StoredSession>, HistoryError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT id, session_id, project, coalesce(custom_title, ai_title) FROM sessions",
        )?;
        let sessions = statement
            .query_map([], |row| {
                Ok(StoredSession {
                    key: SessionKey(row.get(0)?),
                    session_id: row.get(1)?,
                    project: ProjectKey(row.get(2)?),
                    title: row.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(sessions)
    }

    /// Every project the history holds.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn projects(&self) -> Result<Vec<StoredProject>, HistoryError> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT id, transcript_folder, path FROM projects")?;
        let projects = statement
            .query_map([], |row| {
                Ok(StoredProject {
                    key: ProjectKey(row.get(0)?),
                    transcript_folder: row.get(1)?,
                    path: row.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(projects)
    }

    /// Every model the history's requests name, in order.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn models(&self) -> Result<Vec<String>, HistoryError> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT DISTINCT model FROM requests ORDER BY model")?;
        let models = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(models)
    }

    /// The cost Claude Code reports for each session and model, summed over its runs.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn reported_costs(&self) -> Result<Vec<StoredReportedCost>, HistoryError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT session, model, sum(cost_usd), sum(input_tokens), sum(output_tokens),
                 sum(cache_read_tokens), sum(cache_write_tokens)
             FROM reported_costs
             GROUP BY session, model",
        )?;
        let costs = statement
            .query_map([], |row| {
                Ok(StoredReportedCost {
                    session: SessionKey(row.get(0)?),
                    model: row.get(1)?,
                    cost_usd: row.get(2)?,
                    input: row.get(3)?,
                    output: row.get(4)?,
                    cache_read: row.get(5)?,
                    cache_write: row.get(6)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(costs)
    }

    /// Every message id the history holds.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn message_ids(&self) -> Result<Vec<HeldMessage>, HistoryError> {
        let mut statement = self.conn.prepare(
            "SELECT session, message_id, request_id IS NOT NULL FROM requests
             WHERE message_id IS NOT NULL",
        )?;
        let held = statement
            .query_map([], |row| {
                Ok(HeldMessage {
                    session: SessionKey(row.get(0)?),
                    message_id: row.get(1)?,
                    with_request_id: row.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(held)
    }

    /// The latest reading of each plan window, from any source.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn latest_limits(&self) -> Result<Vec<LimitSnapshot>, HistoryError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT window, source, observed_ms, used_percentage, resets_at_ms
             FROM limit_snapshots AS reading
             WHERE observed_ms = (
                 SELECT max(observed_ms) FROM limit_snapshots WHERE window = reading.window
             )
             ORDER BY window, source",
        )?;
        let mut seen = std::collections::HashSet::new();
        let mut snapshots = Vec::new();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let window: String = row.get(0)?;
            let Some(source) = Source::from_name(&row.get::<_, String>(1)?) else {
                continue;
            };
            if seen.insert(window.clone()) {
                snapshots.push(LimitSnapshot {
                    window,
                    source,
                    observed_ms: row.get(2)?,
                    used_percentage: row.get(3)?,
                    resets_at_ms: row.get(4)?,
                });
            }
        }
        Ok(snapshots)
    }

    /// Whether the usage endpoint is on, and how asking it last went.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn usage_endpoint(&self) -> Result<State, HistoryError> {
        let state = self
            .conn
            .prepare_cached(
                "SELECT enabled, next_ask_ms, rate_limited, asked_ms, outcome, detail
                 FROM usage_endpoint WHERE id = 1",
            )?
            .query_row([], |row| {
                Ok(State {
                    enabled: row.get(0)?,
                    next_ask_ms: row.get(1)?,
                    rate_limited: row.get(2)?,
                    asked_ms: row.get(3)?,
                    outcome: row
                        .get::<_, Option<String>>(4)?
                        .and_then(|name| Outcome::from_name(&name)),
                    detail: row.get(5)?,
                })
            })
            .optional()?;
        Ok(state.unwrap_or_default())
    }

    /// The history's schema version.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn schema_version(&self) -> Result<u32, HistoryError> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    /// The import from `source`, if the history holds one.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn import(&self, source: &str) -> Result<Option<StoredImport>, HistoryError> {
        let import = self
            .conn
            .prepare_cached("SELECT imported_ms, requests FROM imports WHERE source = ?1")?
            .query_row([source], |row| {
                Ok(StoredImport {
                    imported_ms: row.get(0)?,
                    requests: row.get(1)?,
                })
            })
            .optional()?;
        Ok(import)
    }
}
