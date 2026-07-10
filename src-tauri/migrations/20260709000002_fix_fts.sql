-- FIX: the initial migration's FTS table declared content='books' with
-- content_rowid='id', but books.id is a TEXT UUID, not an integer rowid.
-- FTS5's external-content rowid must be NUMERIC, so the books_ai / books_au
-- / books_ad triggers did `INSERT INTO books_fts(rowid) VALUES (new.id)` and
-- sqlite raised "datatype mismatch (code 20)" on every insert. That broke ALL
-- book registration paths (register_stored_book / import_* / pixiv), which is
-- why no books ever landed in the library even though downloads + cb7 zips
-- succeeded.
--
-- Rebuild books_fts as a self-contained FTS table keyed by its own integer
-- rowid, keeping book_id as a stored (UNINDEXED) lookup column. Triggers now
-- join on book_id instead of the rowid. SearchService currently uses LIKE on
-- books.* and does not query books_fts; this makes the index structurally
-- correct and keep-ready for FTS-backed search later.

DROP TRIGGER IF EXISTS books_ai;
DROP TRIGGER IF EXISTS books_ad;
DROP TRIGGER IF EXISTS books_au;
DROP TABLE IF EXISTS books_fts;

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

CREATE TRIGGER IF NOT EXISTS books_au AFTER UPDATE ON books BEGIN
    DELETE FROM books_fts WHERE book_id = old.id;
    INSERT INTO books_fts(book_id, title, original_filename)
    VALUES (new.id, new.title, new.original_filename);
END;

-- Backfill any books that already exist in this database (older installs
-- that somehow have rows despite the broken trigger, or test/dev data).
INSERT INTO books_fts(book_id, title, original_filename)
SELECT id, title, original_filename FROM books
WHERE id NOT IN (SELECT book_id FROM books_fts WHERE book_id IS NOT NULL);
