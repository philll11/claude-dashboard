CREATE TABLE compatibility (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    -- The oldest schema version whose binaries can still read this history.
    readable_from INTEGER NOT NULL
) STRICT;

INSERT INTO compatibility (id, readable_from) VALUES (1, 1);

CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    -- The folder in a data root's projects folder that holds the project's transcripts.
    transcript_folder TEXT NOT NULL UNIQUE,
    -- The working folder, once a transcript names it.
    path TEXT
) STRICT;

CREATE TABLE sessions (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    project INTEGER NOT NULL REFERENCES projects (id)
) STRICT;

CREATE INDEX sessions_by_project ON sessions (project);

CREATE TABLE requests (
    id INTEGER PRIMARY KEY,
    session INTEGER NOT NULL REFERENCES sessions (id),
    message_id TEXT,
    request_id TEXT,
    -- The transcript record the request was read from.
    record_id TEXT NOT NULL,
    -- The subagent that made the request; NULL for the session itself.
    subagent_id TEXT,
    -- Milliseconds since the Unix epoch, in UTC.
    timestamp_ms INTEGER NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cache_read_tokens INTEGER NOT NULL,
    cache_write_5m_tokens INTEGER NOT NULL,
    cache_write_1h_tokens INTEGER NOT NULL,
    web_search_requests INTEGER NOT NULL,
    web_fetch_requests INTEGER NOT NULL,
    speed TEXT,
    service_tier TEXT,
    inference_geo TEXT,
    skill TEXT,
    mcp_server TEXT,
    mcp_tool TEXT,
    entrypoint TEXT,
    claude_code_version TEXT
) STRICT;

-- A request's identity (ADR 0003): its message id and request id; without a request id, its
-- message id within the session; without a message id, its record id.
CREATE UNIQUE INDEX requests_by_message_and_request ON requests (message_id, request_id)
    WHERE message_id IS NOT NULL AND request_id IS NOT NULL;
CREATE UNIQUE INDEX requests_by_message_in_session ON requests (session, message_id)
    WHERE message_id IS NOT NULL AND request_id IS NULL;
CREATE UNIQUE INDEX requests_by_record ON requests (record_id)
    WHERE message_id IS NULL;

CREATE INDEX requests_by_time ON requests (timestamp_ms);
CREATE INDEX requests_by_session ON requests (session);
