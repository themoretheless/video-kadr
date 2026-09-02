CREATE TABLE IF NOT EXISTS media_index_cursors (
    name TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    entry_id TEXT NOT NULL
);
