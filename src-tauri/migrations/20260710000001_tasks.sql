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
