-- Initial schema for EroLib.
--
-- Books + tags + collections, plus a self-contained FTS5 index over books.
-- The FTS table carries its own integer rowid and stores `book_id` as an
-- UNINDEXED lookup column: books.id is a TEXT UUID, which is NOT a valid
-- external-content rowid (FTS5's content_rowid must be NUMERIC), so the older
-- `content='books' content_rowid='id'` layout raised "datatype mismatch" on
-- every insert. Keeping FTS standalone and joining via book_id in the
-- triggers fixes that. SearchService currently queries books with LIKE and
-- does not read books_fts; this keeps the index structurally correct and
-- ready for FTS-backed search later.

-- Books
CREATE TABLE IF NOT EXISTS books (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    original_filename TEXT,
    file_path TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    format TEXT NOT NULL,
    page_count INTEGER NOT NULL DEFAULT 0,
    cover_path TEXT,
    source_plugin TEXT,
    source_url TEXT,
    source_post_id TEXT,
    author TEXT,
    author_id TEXT,
    published_at TEXT,
    scraped_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_read_at TEXT,
    read_count INTEGER NOT NULL DEFAULT 0,
    delays TEXT
);

CREATE INDEX IF NOT EXISTS idx_books_title ON books(title);
CREATE INDEX IF NOT EXISTS idx_books_created ON books(created_at);

-- Tags
CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    type TEXT NOT NULL DEFAULT 'custom',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Many-to-many: books <-> tags
CREATE TABLE IF NOT EXISTS book_tags (
    book_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (book_id, tag_id),
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_book_tags_tag ON book_tags(tag_id);

-- Collections
CREATE TABLE IF NOT EXISTS collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS collection_books (
    collection_id TEXT NOT NULL,
    book_id TEXT NOT NULL,
    PRIMARY KEY (collection_id, book_id),
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

-- Full-text search over books (title + filename + tags).
-- Self-contained FTS5: own integer rowid + `book_id UNINDEXED` lookup column.
-- Triggers join on book_id (NOT rowid), since books.id is a TEXT UUID.
CREATE VIRTUAL TABLE IF NOT EXISTS books_fts USING fts5(
    book_id UNINDEXED,
    title,
    original_filename,
    tags,
    tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS books_ai AFTER INSERT ON books BEGIN
    INSERT INTO books_fts(book_id, title, original_filename)
    VALUES (new.id, new.title, new.original_filename);
END;

CREATE TRIGGER IF NOT EXISTS books_ad AFTER DELETE ON books BEGIN
    DELETE FROM books_fts WHERE book_id = old.id;
END;

-- Only re-index when an indexed column actually changes, so routine updates
-- (e.g. opening a book writes last_read_at/read_count) don't churn the FTS
-- shadow tables on every read.
CREATE TRIGGER IF NOT EXISTS books_au AFTER UPDATE ON books
WHEN old.title IS NOT new.title OR old.original_filename IS NOT new.original_filename
BEGIN
    DELETE FROM books_fts WHERE book_id = old.id;
    INSERT INTO books_fts(book_id, title, original_filename)
    VALUES (new.id, new.title, new.original_filename);
END;


-- Download / packaging job queue (tasks).
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    status TEXT NOT NULL,
    title TEXT NOT NULL,
    detail TEXT NOT NULL DEFAULT '',
    progress_current INTEGER NOT NULL DEFAULT 0,
    progress_total INTEGER NOT NULL DEFAULT 0,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    speed INTEGER NOT NULL DEFAULT 0,
    logs TEXT NOT NULL DEFAULT '[]',
    book_id TEXT,
    -- Cumulative downloaded bytes / running time for the "共计 xxMB，用时 x
    -- 时x分x秒" readout shown on completion. elapsed_ms excludes paused/pending
    -- wall-clock (accumulated only while running). run_started_at holds the
    -- RFC3339 start of the current running segment, cleared on every non-running
    -- transition so the next run opens a fresh segment.
    total_bytes INTEGER NOT NULL DEFAULT 0,
    elapsed_ms INTEGER NOT NULL DEFAULT 0,
    run_started_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT,
    payload TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_created_at ON tasks(created_at);
