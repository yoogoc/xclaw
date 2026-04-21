DROP TABLE IF EXISTS attachments;

-- SQLite does not support DROP COLUMN; recreate the table without it.
-- For simplicity during development, we accept that rolling back loses turn data.