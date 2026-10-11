-- The kind of agent each subagent is, such as `Explore`, from the metadata Claude Code keeps beside
-- the subagent's transcript.
CREATE TABLE subagents (
    session INTEGER NOT NULL REFERENCES sessions (id),
    subagent_id TEXT NOT NULL,
    agent_type TEXT NOT NULL,
    PRIMARY KEY (session, subagent_id)
) STRICT, WITHOUT ROWID;

-- Settings that every surface shares, such as the first day of the week.
CREATE TABLE settings (
    name TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT, WITHOUT ROWID;

-- Claude Code data folders the person added, read after the default one or CLAUDE_CONFIG_DIR's.
CREATE TABLE data_folders (
    path TEXT PRIMARY KEY,
    -- When it was added, in milliseconds since the Unix epoch; folders are read in that order.
    added_ms INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- Subagent transcripts are read again from the start, so that their metadata is read too.
DELETE FROM transcripts WHERE path LIKE '%subagents%';
