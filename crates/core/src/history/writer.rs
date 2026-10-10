//! Recording projects, sessions and requests.

use rusqlite::{Transaction, params};

use super::HistoryError;

/// A project recorded in the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectKey(i64);

/// A session recorded in the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionKey(i64);

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

/// The tokens a request used, by type.
///
/// One request's counts fit in `u32`, because a request cannot exceed a model's context window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u32,
    pub output: u32,
    pub cache_read: u32,
    pub cache_write_5m: u32,
    pub cache_write_1h: u32,
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
    /// When the request was made, in milliseconds since the Unix epoch.
    pub timestamp_ms: i64,
    pub model: &'a str,
    pub tokens: Tokens,
    pub web_search_requests: u32,
    pub web_fetch_requests: u32,
    pub speed: Option<&'a str>,
    pub service_tier: Option<&'a str>,
    pub inference_geo: Option<&'a str>,
    pub attribution: Attribution<'a>,
    /// How Claude Code was started, such as `cli` or `sdk-cli`.
    pub entrypoint: Option<&'a str>,
    pub claude_code_version: Option<&'a str>,
}

/// Records into the history within one transaction, which [`History::write`] opens.
///
/// [`History::write`]: super::History::write
#[derive(Debug)]
pub struct Writer<'conn> {
    tx: Transaction<'conn>,
}

impl<'conn> Writer<'conn> {
    pub(super) fn new(tx: Transaction<'conn>) -> Self {
        Self { tx }
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

    /// Records a request, or replaces the recorded request with the same identity.
    ///
    /// Of the records that share an identity, the one with the most tokens wins, because it
    /// carries the final tally of a streamed response. Equal usage goes to the later record, then
    /// to the larger record id, so the history never depends on the order records arrive in.
    ///
    /// # Errors
    ///
    /// SQLite errors.
    pub fn request(&mut self, request: &Request<'_>) -> Result<(), HistoryError> {
        let (sql, message_id, request_id) = match request.identity {
            Identity::MessageAndRequest {
                message_id,
                request_id,
            } => (
                UPSERT_BY_MESSAGE_AND_REQUEST,
                Some(message_id),
                Some(request_id),
            ),
            Identity::MessageInSession { message_id } => {
                (UPSERT_BY_MESSAGE_IN_SESSION, Some(message_id), None)
            }
            Identity::Record => (UPSERT_BY_RECORD, None, None),
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
        self.tx.prepare_cached(sql)?.execute(params![
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
        ])?;
        Ok(())
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
                claude_code_version
            )
            VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                ?19, ?20, ?21, ?22
            )
            ON CONFLICT ",
            $conflict_target,
            " DO UPDATE SET
                session = excluded.session,
                record_id = excluded.record_id,
                subagent_id = excluded.subagent_id,
                timestamp_ms = excluded.timestamp_ms,
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
                claude_code_version = excluded.claude_code_version
            WHERE (
                excluded.input_tokens + excluded.output_tokens + excluded.cache_read_tokens
                    + excluded.cache_write_5m_tokens + excluded.cache_write_1h_tokens,
                excluded.timestamp_ms,
                excluded.record_id
            ) > (
                requests.input_tokens + requests.output_tokens + requests.cache_read_tokens
                    + requests.cache_write_5m_tokens + requests.cache_write_1h_tokens,
                requests.timestamp_ms,
                requests.record_id
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
