CREATE TABLE attachments (
    id         TEXT PRIMARY KEY NOT NULL,
    kind       TEXT NOT NULL,
    mime_type  TEXT NOT NULL,
    filename   TEXT,
    size_bytes INTEGER,
    source_url TEXT,
    created_at TEXT NOT NULL
);

ALTER TABLE turns ADD COLUMN attachments TEXT NOT NULL DEFAULT '[]';