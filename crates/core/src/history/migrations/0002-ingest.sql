-- How far each transcript has been read.
CREATE TABLE transcripts (
    path TEXT PRIMARY KEY,
    -- The byte where the next read starts.
    read_to INTEGER NOT NULL,
    -- The transcript's size and modification time when it was last read, so that an unchanged
    -- transcript is skipped without opening it.
    len INTEGER NOT NULL,
    modified_ms INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- The last title of each kind; a custom title wins over the AI title.
ALTER TABLE sessions ADD COLUMN ai_title TEXT;
ALTER TABLE sessions ADD COLUMN custom_title TEXT;

-- The cost Claude Code reports for a session at list prices, by run and model.
CREATE TABLE reported_costs (
    session INTEGER NOT NULL REFERENCES sessions (id),
    -- When the run of Claude Code that reported it started, in milliseconds since the Unix epoch.
    started_ms INTEGER NOT NULL,
    model TEXT NOT NULL,
    cost_usd REAL NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cache_read_tokens INTEGER NOT NULL,
    -- Cache writes of both durations, which Claude Code does not report apart.
    cache_write_tokens INTEGER NOT NULL,
    PRIMARY KEY (session, started_ms, model)
) STRICT, WITHOUT ROWID;
