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

-- The source a request was imported from, such as `claude-usage`; NULL for a request read from a
-- transcript.
ALTER TABLE requests ADD COLUMN imported_from TEXT;

-- One-time imports from other sources.
CREATE TABLE imports (
    source TEXT PRIMARY KEY,
    imported_ms INTEGER NOT NULL,
    requests INTEGER NOT NULL
) STRICT;

-- Tokens per model per day from Claude Code's statistics, which outlive its transcripts. They
-- stand in for days that no request covers.
CREATE TABLE daily_totals (
    -- The day as Claude Code dates it, YYYY-MM-DD.
    day TEXT NOT NULL,
    model TEXT NOT NULL,
    tokens INTEGER NOT NULL,
    PRIMARY KEY (day, model)
) STRICT, WITHOUT ROWID;

-- Readings of how much of each plan window was used.
CREATE TABLE limit_snapshots (
    -- The plan window as Claude Code names it, such as `five_hour` or `seven_day`.
    window TEXT NOT NULL,
    -- Where the reading came from: `status-line` or `usage-endpoint`.
    source TEXT NOT NULL,
    -- The five minutes the reading falls in; the latest reading in them is kept.
    bucket_ms INTEGER NOT NULL,
    observed_ms INTEGER NOT NULL,
    used_percentage REAL NOT NULL,
    resets_at_ms INTEGER,
    PRIMARY KEY (window, source, bucket_ms)
) STRICT, WITHOUT ROWID;

CREATE INDEX limit_snapshots_by_time ON limit_snapshots (window, observed_ms);
