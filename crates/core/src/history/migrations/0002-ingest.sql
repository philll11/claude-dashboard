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
