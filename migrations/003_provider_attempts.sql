CREATE TABLE IF NOT EXISTS metadata_provider_attempts (
    book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    attempted_at DATETIME NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (book_id, provider)
);
