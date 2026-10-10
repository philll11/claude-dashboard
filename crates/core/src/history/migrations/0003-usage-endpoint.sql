-- Anthropic's usage endpoint, which the person turns on (ADR 0005), and how asking it went.
-- One row, shared by every process on the machine, so that together they ask at most once per
-- interval.
CREATE TABLE usage_endpoint (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    -- No process asks before this time, in milliseconds since the Unix epoch.
    next_ask_ms INTEGER NOT NULL DEFAULT 0,
    -- Answers in a row that said "asked too often", which lengthen the wait.
    rate_limited INTEGER NOT NULL DEFAULT 0,
    -- When a process last took its turn to ask, and what came of it.
    asked_ms INTEGER,
    outcome TEXT,
    -- What went wrong, for an outcome that needs it; never anything from the login.
    detail TEXT
) STRICT;

INSERT INTO usage_endpoint (id) VALUES (1);
