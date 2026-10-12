//! Recording projects, sessions and requests.

use rusqlite::{OptionalExtension, Transaction, params};

use super::HistoryError;
use crate::limits::LimitSnapshot;
use crate::tokens::Tokens;
use crate::usage_endpoint::Outcome;

/// A project recorded in the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectKey(pub(super) i64);

/// A session recorded in the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionKey(pub(super) i64);

/// What tells a request apart from every other request (ADR 0003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identity<'a> {
    /// The message id and the request id, which together are unique across sessions.
    MessageAndRequest {
        message_id: &'a str,
        request_id: &'a str,
    },
    /// The message id within the request's session, when the request id is missing.
    MessageInSession { message_id: &'a str },
    /// The id of the transcript record, when the message id is missing.
    Record,
}

impl<'a> Identity<'a> {
    /// The identity that a transcript record's ids give, treating an empty id as missing.
    #[must_use]
    pub fn from_ids(message_id: Option<&'a str>, request_id: Option<&'a str>) -> Self {
        let present = |id: Option<&'a str>| id.filter(|id| !id.is_empty());
        match (present(message_id), present(request_id)) {
            (Some(message_id), Some(request_id)) => Self::MessageAndRequest {
                message_id,
                request_id,
            },
            (Some(message_id), None) => Self::MessageInSession { message_id },
            (None, _) => Self::Record,
        }
    }
}

/// A session title, as Claude Code records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTitle<'a> {
    /// The title Claude Code generated.
    Ai(&'a str),
    /// The title the person gave the session.
    Custom(&'a str),
}

/// The cost that Claude Code reports for one model in a session, at list prices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReportedCost<'a> {
    pub model: &'a str,
    pub cost_usd: f64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    /// Cache writes of both durations.
    pub cache_write: u64,
}

/// How far a transcript has been read, and its size and modification time then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadPosition {
    /// The byte where the next read starts.
    pub read_to: u64,
    pub len: u64,
    /// Milliseconds since the Unix epoch.
    pub modified_ms: i64,
}

/// The skill, MCP server or MCP tool that Claude Code credits a request to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Attribution<'a> {
    pub skill: Option<&'a str>,
    pub mcp_server: Option<&'a str>,
    pub mcp_tool: Option<&'a str>,
}

/// One request, as a transcript records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request<'a> {
    pub session: SessionKey,
    pub identity: Identity<'a>,
    /// The transcript record the request was read from.
    pub record_id: &'a str,
    /// The subagent that made the request; `None` for the session itself.
    pub subagent_id: Option<&'a str>,
    /// When the record was written, in milliseconds since the Unix epoch.
    pub timestamp_ms: i64,
    pub model: &'a str,
    pub tokens: Tokens,
    pub web_search_requests: u64,
    pub web_fetch_requests: u64,
    pub speed: Option<&'a str>,
    pub service_tier: Option<&'a str>,
    pub inference_geo: Option<&'a str>,
    pub attribution: Attribution<'a>,
    /// How Claude Code was started, such as `cli` or `sdk-cli`.
    pub entrypoint: Option<&'a str>,
    pub claude_code_version: Option<&'a str>,
    /// The source the request was imported from; `None` for a request read from a transcript.
    pub imported_from: Option<&'a str>,
}

/// Records into the history within one transaction, which [`History::write`] opens.
///
/// [`History::write`]: super::History::write
#[derive(Debug)]
pub struct Writer<'conn> {
    tx: Transaction<'conn>,
    /// Whether the history holds imported requests, once a write has asked.
    imports_exist: Option<bool>,
}

impl<'conn> Writer<'conn> {
    pub(super) fn new(tx: Transaction<'conn>) -> Self {
        Self {
            tx,
            imports_exist: None,
        }
    }

    pub(super) fn commit(self) -> Result<(), HistoryError> {
        Ok(self.tx.commit()?)
    }

    /// Records a project, identified by the folder that holds its transcripts, and fills in its
    /// working folder once it is known.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn project(
        &mut self,
        transcript_folder: &str,
        path: Option<&str>,
    ) -> Result<ProjectKey, HistoryError> {
        self.tx
            .prepare_cached(
                "INSERT INTO projects (transcript_folder, path) VALUES (?1, ?2)
                 ON CONFLICT (transcript_folder) DO UPDATE SET path = excluded.path
                 WHERE projects.path IS NULL AND excluded.path IS NOT NULL",
            )?
            .execute(params![transcript_folder, path])?;
        let id = self
            .tx
            .prepare_cached("SELECT id FROM projects WHERE transcript_folder = ?1")?
            .query_row([transcript_folder], |row| row.get(0))?;
        Ok(ProjectKey(id))
    }

    /// The project whose working folder is `path`, if the history holds one.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn project_with_path(&mut self, path: &str) -> Result<Option<ProjectKey>, HistoryError> {
        let id = self
            .tx
            .prepare_cached("SELECT id FROM projects WHERE path = ?1 ORDER BY id LIMIT 1")?
            .query_row([path], |row| row.get(0))
            .optional()?;
        Ok(id.map(ProjectKey))
    }

    /// Records a session of a project, identified by Claude Code's session id.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn session(
        &mut self,
        session_id: &str,
        project: ProjectKey,
    ) -> Result<SessionKey, HistoryError> {
        self.tx
            .prepare_cached(
                "INSERT INTO sessions (session_id, project) VALUES (?1, ?2)
                 ON CONFLICT (session_id) DO NOTHING",
            )?
            .execute(params![session_id, project.0])?;
        let id = self
            .tx
            .prepare_cached("SELECT id FROM sessions WHERE session_id = ?1")?
            .query_row([session_id], |row| row.get(0))?;
        Ok(SessionKey(id))
    }

    /// Records a session's latest title of one kind.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn session_title(
        &mut self,
        session: SessionKey,
        title: SessionTitle<'_>,
    ) -> Result<(), HistoryError> {
        let (sql, text) = match title {
            SessionTitle::Ai(text) => ("UPDATE sessions SET ai_title = ?2 WHERE id = ?1", text),
            SessionTitle::Custom(text) => {
                ("UPDATE sessions SET custom_title = ?2 WHERE id = ?1", text)
            }
        };
        self.tx
            .prepare_cached(sql)?
            .execute(params![session.0, text])?;
        Ok(())
    }

    /// Records the kind of agent a subagent of a session is.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn subagent_type(
        &mut self,
        session: SessionKey,
        subagent_id: &str,
        agent_type: &str,
    ) -> Result<(), HistoryError> {
        self.tx
            .prepare_cached(
                "INSERT INTO subagents (session, subagent_id, agent_type) VALUES (?1, ?2, ?3)
                 ON CONFLICT (session, subagent_id) DO UPDATE SET agent_type = excluded.agent_type",
            )?
            .execute(params![session.0, subagent_id, agent_type])?;
        Ok(())
    }

    /// Records the cost that one run of Claude Code reports for a session and model.
    ///
    /// A run reports its running total several times, so the largest report of a run wins,
    /// whatever order the reports arrive in.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn reported_cost(
        &mut self,
        session: SessionKey,
        started_ms: i64,
        cost: &ReportedCost<'_>,
    ) -> Result<(), HistoryError> {
        self.tx
            .prepare_cached(
                "INSERT INTO reported_costs (
                     session, started_ms, model, cost_usd, input_tokens, output_tokens,
                     cache_read_tokens, cache_write_tokens
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (session, started_ms, model) DO UPDATE SET
                     cost_usd = excluded.cost_usd,
                     input_tokens = excluded.input_tokens,
                     output_tokens = excluded.output_tokens,
                     cache_read_tokens = excluded.cache_read_tokens,
                     cache_write_tokens = excluded.cache_write_tokens
                 WHERE (
                     excluded.cost_usd, excluded.output_tokens, excluded.input_tokens,
                     excluded.cache_read_tokens, excluded.cache_write_tokens
                 ) > (
                     reported_costs.cost_usd, reported_costs.output_tokens,
                     reported_costs.input_tokens, reported_costs.cache_read_tokens,
                     reported_costs.cache_write_tokens
                 )",
            )?
            .execute(params![
                session.0,
                started_ms,
                cost.model,
                cost.cost_usd,
                cost.input,
                cost.output,
                cost.cache_read,
                cost.cache_write
            ])?;
        Ok(())
    }

    /// Records a session title unless the session already has a title of either kind.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn session_title_if_none(
        &mut self,
        session: SessionKey,
        title: SessionTitle<'_>,
    ) -> Result<(), HistoryError> {
        let (sql, text) = match title {
            SessionTitle::Ai(text) => (
                "UPDATE sessions SET ai_title = ?2
                 WHERE id = ?1 AND ai_title IS NULL AND custom_title IS NULL",
                text,
            ),
            SessionTitle::Custom(text) => (
                "UPDATE sessions SET custom_title = ?2
                 WHERE id = ?1 AND ai_title IS NULL AND custom_title IS NULL",
                text,
            ),
        };
        self.tx
            .prepare_cached(sql)?
            .execute(params![session.0, text])?;
        Ok(())
    }

    /// Records a plan-limit reading. The latest reading in each five minutes of each window and
    /// source is kept.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn limit_snapshot(&mut self, snapshot: &LimitSnapshot) -> Result<(), HistoryError> {
        const BUCKET_MS: i64 = 5 * 60 * 1000;
        self.tx
            .prepare_cached(
                "INSERT INTO limit_snapshots (
                     window, source, bucket_ms, observed_ms, used_percentage, resets_at_ms
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (window, source, bucket_ms) DO UPDATE SET
                     observed_ms = excluded.observed_ms,
                     used_percentage = excluded.used_percentage,
                     resets_at_ms = excluded.resets_at_ms
                 WHERE excluded.observed_ms >= limit_snapshots.observed_ms",
            )?
            .execute(params![
                snapshot.window,
                snapshot.source.as_str(),
                snapshot.observed_ms.div_euclid(BUCKET_MS) * BUCKET_MS,
                snapshot.observed_ms,
                snapshot.used_percentage,
                snapshot.resets_at_ms
            ])?;
        Ok(())
    }

    /// Turns the usage endpoint on or off, for every process on the machine.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn set_usage_endpoint(&mut self, enabled: bool) -> Result<(), HistoryError> {
        self.tx
            .prepare_cached("UPDATE usage_endpoint SET enabled = ?1 WHERE id = 1")?
            .execute([enabled])?;
        Ok(())
    }

    /// Takes this process's turn to ask the usage endpoint, and puts the next turn
    /// `interval_ms` later. Returns `false`, taking nothing, while the endpoint is off or another
    /// process's turn is more recent.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn take_usage_endpoint_turn(
        &mut self,
        now_ms: i64,
        interval_ms: i64,
    ) -> Result<bool, HistoryError> {
        let taken = self
            .tx
            .prepare_cached(
                "UPDATE usage_endpoint
                 SET next_ask_ms = ?1 + ?2, asked_ms = ?1, outcome = NULL, detail = NULL
                 WHERE id = 1 AND enabled = 1 AND next_ask_ms <= ?1",
            )?
            .execute(params![now_ms, interval_ms])?;
        Ok(taken == 1)
    }

    /// Records what came of this process's turn, and puts the next turn at `wait_until_ms` when
    /// that is later than planned.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn usage_endpoint_outcome(
        &mut self,
        outcome: Outcome,
        detail: Option<&str>,
        wait_until_ms: Option<i64>,
    ) -> Result<(), HistoryError> {
        self.tx
            .prepare_cached(
                "UPDATE usage_endpoint
                 SET outcome = ?1, detail = ?2,
                     rate_limited = CASE ?1
                         WHEN 'rate-limited' THEN rate_limited + 1
                         WHEN 'answered' THEN 0
                         ELSE rate_limited
                     END,
                     next_ask_ms = max(next_ask_ms, coalesce(?3, next_ask_ms))
                 WHERE id = 1",
            )?
            .execute(params![outcome.as_str(), detail, wait_until_ms])?;
        Ok(())
    }

    /// Records how far a transcript has been read.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn transcript_read(
        &mut self,
        path: &str,
        position: ReadPosition,
    ) -> Result<(), HistoryError> {
        self.tx
            .prepare_cached(
                "INSERT INTO transcripts (path, read_to, len, modified_ms) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (path) DO UPDATE SET
                     read_to = excluded.read_to,
                     len = excluded.len,
                     modified_ms = excluded.modified_ms",
            )?
            .execute(params![
                path,
                position.read_to,
                position.len,
                position.modified_ms
            ])?;
        Ok(())
    }

    /// Forgets how far a transcript has been read, once it is no longer there. Its requests stay.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn forget_transcript(&mut self, path: &str) -> Result<(), HistoryError> {
        self.tx
            .prepare_cached("DELETE FROM transcripts WHERE path = ?1")?
            .execute([path])?;
        Ok(())
    }

    /// Records a request, or replaces the recorded request with the same identity.
    ///
    /// Of the records that share an identity, one read from a transcript wins over an imported
    /// one, and then the one with the most tokens, because it carries the final tally of a
    /// streamed response. Equal usage goes to the later record, then to the larger record id,
    /// then to the larger session id, which separates a resumed session's verbatim copy from its
    /// original. The winning record gives the request its details, and the earliest record its
    /// time: when it was made. So the history never depends on the order records arrive in. A
    /// request read from a transcript also replaces an imported request for the same message.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn request(&mut self, request: &Request<'_>) -> Result<(), HistoryError> {
        let (sql, earlier, message_id, request_id) = match request.identity {
            Identity::MessageAndRequest {
                message_id,
                request_id,
            } => (
                UPSERT_BY_MESSAGE_AND_REQUEST,
                EARLIER_BY_MESSAGE_AND_REQUEST,
                Some(message_id),
                Some(request_id),
            ),
            Identity::MessageInSession { message_id } => (
                UPSERT_BY_MESSAGE_IN_SESSION,
                EARLIER_BY_MESSAGE_IN_SESSION,
                Some(message_id),
                None,
            ),
            Identity::Record => (UPSERT_BY_RECORD, EARLIER_BY_RECORD, None, None),
        };
        let Tokens {
            input,
            output,
            cache_read,
            cache_write_5m,
            cache_write_1h,
        } = request.tokens;
        let Attribution {
            skill,
            mcp_server,
            mcp_tool,
        } = request.attribution;
        let changed = self.tx.prepare_cached(sql)?.execute(params![
            request.session.0,
            message_id,
            request_id,
            request.record_id,
            request.subagent_id,
            request.timestamp_ms,
            request.model,
            input,
            output,
            cache_read,
            cache_write_5m,
            cache_write_1h,
            request.web_search_requests,
            request.web_fetch_requests,
            request.speed,
            request.service_tier,
            request.inference_geo,
            skill,
            mcp_server,
            mcp_tool,
            request.entrypoint,
            request.claude_code_version,
            request.imported_from,
        ])?;
        if changed == 0 {
            self.tx.prepare_cached(earlier)?.execute(params![
                request.session.0,
                message_id,
                request_id,
                request.record_id,
                request.timestamp_ms,
            ])?;
        }
        if let (Identity::MessageAndRequest { message_id, .. }, None) =
            (request.identity, request.imported_from)
            && self.imports_exist()?
        {
            self.tx
                .prepare_cached(
                    "DELETE FROM requests
                     WHERE session = ?1 AND message_id = ?2 AND request_id IS NULL
                         AND imported_from IS NOT NULL",
                )?
                .execute(params![request.session.0, message_id])?;
        }
        Ok(())
    }

    /// Records an import from `source`, adding `requests` to the requests it has added.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn record_import(
        &mut self,
        source: &str,
        imported_ms: i64,
        requests: u64,
    ) -> Result<(), HistoryError> {
        self.tx
            .prepare_cached(
                "INSERT INTO imports (source, imported_ms, requests) VALUES (?1, ?2, ?3)
                 ON CONFLICT (source) DO UPDATE SET
                     imported_ms = excluded.imported_ms,
                     requests = imports.requests + excluded.requests",
            )?
            .execute(params![source, imported_ms, requests])?;
        self.imports_exist = Some(true);
        Ok(())
    }

    /// Sets a setting every surface shares, or clears it with `None`.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn set_setting(&mut self, name: &str, value: Option<&str>) -> Result<(), HistoryError> {
        match value {
            Some(value) => self
                .tx
                .prepare_cached(
                    "INSERT INTO settings (name, value) VALUES (?1, ?2)
                 ON CONFLICT (name) DO UPDATE SET value = excluded.value",
                )?
                .execute(params![name, value])?,
            None => self
                .tx
                .prepare_cached("DELETE FROM settings WHERE name = ?1")?
                .execute([name])?,
        };
        Ok(())
    }

    /// Takes a turn that the setting `name` keeps, as a time in milliseconds before which no
    /// process takes it: when that time has come, or the setting is unset, holds the turn until
    /// `now_ms + hold_ms` and says so.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn take_turn(
        &mut self,
        name: &str,
        now_ms: i64,
        hold_ms: i64,
    ) -> Result<bool, HistoryError> {
        let taken = self
            .tx
            .prepare_cached(
                "INSERT INTO settings (name, value) VALUES (?1, ?2)
                 ON CONFLICT (name) DO UPDATE SET value = excluded.value
                 WHERE CAST(settings.value AS INTEGER) <= ?3",
            )?
            .execute(params![
                name,
                now_ms.saturating_add(hold_ms).to_string(),
                now_ms
            ])?;
        Ok(taken == 1)
    }

    /// Adds a data folder, read after the default one or `CLAUDE_CONFIG_DIR`'s.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn add_data_folder(&mut self, path: &str, added_ms: i64) -> Result<(), HistoryError> {
        self.tx
            .prepare_cached(
                "INSERT INTO data_folders (path, added_ms) VALUES (?1, ?2)
                 ON CONFLICT (path) DO NOTHING",
            )?
            .execute(params![path, added_ms])?;
        Ok(())
    }

    /// Removes a data folder the person added; what was read from it stays.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn remove_data_folder(&mut self, path: &str) -> Result<(), HistoryError> {
        self.tx
            .prepare_cached("DELETE FROM data_folders WHERE path = ?1")?
            .execute([path])?;
        Ok(())
    }

    /// Records a model's tokens on one day, from Claude Code's statistics.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn daily_total(&mut self, day: &str, model: &str, tokens: u64) -> Result<(), HistoryError> {
        self.tx
            .prepare_cached(
                "INSERT INTO daily_totals (day, model, tokens) VALUES (?1, ?2, ?3)
                 ON CONFLICT (day, model) DO UPDATE SET tokens = excluded.tokens",
            )?
            .execute(params![day, model, tokens])?;
        Ok(())
    }

    fn imports_exist(&mut self) -> Result<bool, HistoryError> {
        if let Some(exist) = self.imports_exist {
            return Ok(exist);
        }
        let exist = self
            .tx
            .prepare_cached("SELECT EXISTS (SELECT 1 FROM imports)")?
            .query_row([], |row| row.get(0))?;
        self.imports_exist = Some(exist);
        Ok(exist)
    }
}

/// The insert-or-replace statement for requests whose identity a unique index in the schema
/// enforces; each identity needs its own conflict target.
macro_rules! upsert_request {
    ($conflict_target:literal) => {
        concat!(
            "INSERT INTO requests (
                session, message_id, request_id, record_id, subagent_id, timestamp_ms, model,
                input_tokens, output_tokens, cache_read_tokens, cache_write_5m_tokens,
                cache_write_1h_tokens, web_search_requests, web_fetch_requests, speed,
                service_tier, inference_geo, skill, mcp_server, mcp_tool, entrypoint,
                claude_code_version, imported_from, record_ms
            )
            VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                ?19, ?20, ?21, ?22, ?23, ?6
            )
            ON CONFLICT ",
            $conflict_target,
            " DO UPDATE SET
                session = excluded.session,
                record_id = excluded.record_id,
                subagent_id = excluded.subagent_id,
                timestamp_ms = min(requests.timestamp_ms, excluded.timestamp_ms),
                record_ms = excluded.record_ms,
                model = excluded.model,
                input_tokens = excluded.input_tokens,
                output_tokens = excluded.output_tokens,
                cache_read_tokens = excluded.cache_read_tokens,
                cache_write_5m_tokens = excluded.cache_write_5m_tokens,
                cache_write_1h_tokens = excluded.cache_write_1h_tokens,
                web_search_requests = excluded.web_search_requests,
                web_fetch_requests = excluded.web_fetch_requests,
                speed = excluded.speed,
                service_tier = excluded.service_tier,
                inference_geo = excluded.inference_geo,
                skill = excluded.skill,
                mcp_server = excluded.mcp_server,
                mcp_tool = excluded.mcp_tool,
                entrypoint = excluded.entrypoint,
                claude_code_version = excluded.claude_code_version,
                imported_from = excluded.imported_from
            WHERE (
                excluded.imported_from IS NULL,
                excluded.input_tokens + excluded.output_tokens + excluded.cache_read_tokens
                    + excluded.cache_write_5m_tokens + excluded.cache_write_1h_tokens,
                excluded.record_ms,
                excluded.record_id,
                (SELECT session_id FROM sessions WHERE id = excluded.session)
            ) > (
                requests.imported_from IS NULL,
                requests.input_tokens + requests.output_tokens + requests.cache_read_tokens
                    + requests.cache_write_5m_tokens + requests.cache_write_1h_tokens,
                requests.record_ms,
                requests.record_id,
                (SELECT session_id FROM sessions WHERE id = requests.session)
            )"
        )
    };
}

const UPSERT_BY_MESSAGE_AND_REQUEST: &str = upsert_request!(
    "(message_id, request_id) WHERE message_id IS NOT NULL AND request_id IS NOT NULL"
);
const UPSERT_BY_MESSAGE_IN_SESSION: &str =
    upsert_request!("(session, message_id) WHERE message_id IS NOT NULL AND request_id IS NULL");
const UPSERT_BY_RECORD: &str = upsert_request!("(record_id) WHERE message_id IS NULL");

/// Dates a request by a record that did not win, when that record is the earliest yet. The
/// parameters are ?1 session, ?2 message id, ?3 request id, ?4 record id and ?5 the record's time.
const EARLIER_BY_MESSAGE_AND_REQUEST: &str = "UPDATE requests SET timestamp_ms = ?5
     WHERE message_id = ?2 AND request_id = ?3 AND timestamp_ms > ?5";
const EARLIER_BY_MESSAGE_IN_SESSION: &str = "UPDATE requests SET timestamp_ms = ?5
     WHERE session = ?1 AND message_id = ?2 AND request_id IS NULL AND timestamp_ms > ?5";
const EARLIER_BY_RECORD: &str = "UPDATE requests SET timestamp_ms = ?5
     WHERE record_id = ?4 AND message_id IS NULL AND timestamp_ms > ?5";

#[cfg(test)]
mod tests {
    use super::Identity;

    #[test]
    fn identity_prefers_message_and_request_ids() {
        assert_eq!(
            Identity::from_ids(Some("msg_1"), Some("req_1")),
            Identity::MessageAndRequest {
                message_id: "msg_1",
                request_id: "req_1"
            }
        );
        assert_eq!(
            Identity::from_ids(Some("msg_1"), None),
            Identity::MessageInSession {
                message_id: "msg_1"
            }
        );
        assert_eq!(
            Identity::from_ids(Some("msg_1"), Some("")),
            Identity::MessageInSession {
                message_id: "msg_1"
            }
        );
        assert_eq!(Identity::from_ids(None, Some("req_1")), Identity::Record);
        assert_eq!(
            Identity::from_ids(Some(""), Some("req_1")),
            Identity::Record
        );
    }
}
