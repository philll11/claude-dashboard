-- When the record whose details a request keeps was written, which decides between records with
-- equal usage. A request's `timestamp_ms` is when it was made: the time of its earliest record.
ALTER TABLE requests ADD COLUMN record_ms INTEGER NOT NULL DEFAULT 0;
UPDATE requests SET record_ms = timestamp_ms;

-- Reading every transcript again dates each request by its earliest record.
DELETE FROM transcripts;
