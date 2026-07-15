use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Datelike, Local, TimeZone, Utc};
use sqlx::{Row, Sqlite};
use uuid::Uuid;

use crate::db::Database;
use crate::errors::AppError;
use crate::models::{Book, BookMetadata, BookSource};
use crate::services::StorageService;

pub struct LibraryService {
    pub(crate) db: Arc<Database>,
    pub(crate) storage: Arc<StorageService>,
}

impl LibraryService {
    pub fn new(db: Arc<Database>, storage: Arc<StorageService>) -> Self {
        Self { db, storage }
    }

    /// Import an existing CB7/CBZ/CBR/PDF file into the library.
    ///
    /// For CB7/CBZ the archive's ComicInfo.xml is read back (title, tags,
    /// source, delays) so an erolib-exported cb7 round-trips losslessly.
    /// CBR/PDF and archives without ComicInfo fall back to the file name and
    /// empty source.
    pub async fn import_book(&self, file_path: String) -> Result<Book, AppError> {
        let path = Path::new(&file_path);
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let format = detect_format(&file_name);
        let mut page_count = count_archive_pages(path, &format).unwrap_or(0);

        let book_id = Uuid::new_v4().to_string();
        let dest = self
            .storage
            .library_path
            .join(format!("{}.cb7", book_id));

        // Copy into library storage.
        std::fs::copy(path, &dest).map_err(AppError::Io)?;

        // Recover metadata from ComicInfo.xml (cb7/cbz only). read_comic_info
        // returns None for non-zip formats or archives without ComicInfo.
        let meta = self.storage.read_comic_info(&dest);

        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&file_name)
            .to_string();
        let title = meta
            .as_ref()
            .map(|m| m.title.clone())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(file_stem);
        let tags = meta.as_ref().map(|m| m.tags.clone()).unwrap_or_default();
        let delays = meta.as_ref().and_then(|m| m.delays.clone());
        // Animated books (ugoira) contain per-frame delays and are logically a
        // single page — the reader plays the jpg sequence on a timer. The raw
        // image count (count_archive_pages) would be the frame count, not 1.
        if delays.as_deref().map_or(false, |d| !d.is_empty()) {
            page_count = 1;
        }
        // Only reconstruct a BookSource when the archive actually carried an
        // erolib source plugin; otherwise leave source as None.
        let source = meta.as_ref().and_then(|m| {
            let plugin = m.source_plugin.as_deref()?.trim();
            if plugin.is_empty() {
                return None;
            }
            Some(BookSource {
                plugin: plugin.to_string(),
                source_url: m.source_url.clone().unwrap_or_default(),
                scraped_at: m.scraped_at.as_deref().and_then(parse_rfc3339),
                source_post_id: m.source_post_id.clone(),
                author: m.author.clone(),
                author_id: None,
                published_at: m.published_at.clone(),
            })
        });

        // register_stored_book handles cover extraction, source/tags/delays
        // binding, and the books INSERT.
        let mut book = self
            .register_stored_book(
                &book_id,
                &title,
                &dest,
                page_count,
                source.as_ref(),
                &tags,
                delays.as_deref(),
            )
            .await?;

        // register_stored_book's INSERT omits original_filename; backfill it so
        // the library card can show the file the user imported from.
        sqlx::query("UPDATE books SET original_filename = ? WHERE id = ?")
            .bind(&file_name)
            .bind(&book_id)
            .execute(&self.db.pool)
            .await
            .map_err(AppError::Db)?;
        book.original_filename = Some(file_name);

        Ok(book)
    }

    /// Import a book from a set of in-memory images + metadata.
    pub async fn import_from_images(
        &self,
        images: Vec<Vec<u8>>,
        metadata: BookMetadata,
    ) -> Result<Book, AppError> {
        if images.is_empty() {
            return Err(AppError::Other("No images provided".into()));
        }

        // Animated books: frame count ≠ page count (always 1 logical page).
        let page_count = if metadata.delays.as_deref().map_or(false, |d| !d.is_empty()) {
            1
        } else {
            images.len() as i32
        };

        let book_id = Uuid::new_v4().to_string();
        let file_path = self.storage.create_cb7(&images, &metadata)?;

        let cover_path = self
            .storage
            .extract_cover(&file_path, &book_id)
            .ok()
            .map(|p| p.to_string_lossy().to_string());

        let file_size = std::fs::metadata(&file_path).map(|m| m.len() as i64).unwrap_or(0);
        let now = Utc::now();

        let book = Book {
            id: book_id,
            title: metadata.title.clone(),
            original_filename: None,
            file_path: file_path.to_string_lossy().to_string(),
            file_size,
            format: "cb7".into(),
            page_count,
            cover_path,
            source_plugin: None,
            source_url: None,
            source_post_id: None,
            author: None,
            author_id: None,
            published_at: None,
            scraped_at: Some(now),
            created_at: now,
            updated_at: now,
            last_read_at: None,
            read_count: 0,
            tags: None,
            delays: None,
        };

        sqlx::query(
            r#"INSERT INTO books
            (id, title, file_path, file_size, format, page_count, cover_path, scraped_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&book.id)
        .bind(&book.title)
        .bind(&book.file_path)
        .bind(book.file_size)
        .bind(&book.format)
        .bind(book.page_count)
        .bind(&book.cover_path)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.db.pool)
        .await
        .map_err(AppError::Db)?;

        // Insert tags and associations.
        for tag_name in &metadata.tags {
            upsert_tag_and_link(&self.db.pool, &book.id, tag_name, "custom").await?;
        }

        Ok(book)
    }

    pub async fn delete_book(&self, id: String) -> Result<(), AppError> {
        let row = sqlx::query_as::<_, Book>("SELECT * FROM books WHERE id = ?")
            .bind(&id)
            .fetch_optional(&self.db.pool)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::BookNotFound(id.clone()))?;

        self.storage
            .delete_book(Path::new(&row.file_path), &id)
            .map_err(AppError::Anyhow)?;

        sqlx::query("DELETE FROM books WHERE id = ?")
            .bind(&id)
            .execute(&self.db.pool)
            .await
            .map_err(AppError::Db)?;

        // Detach any completed task that pointed at this book so its "Read"
        // button disappears (the book is gone). Best-effort — a failure here
        // must not block the delete; the frontend also clears book_id via the
        // `book://deleted` event emitted by the command.
        let _ = sqlx::query("UPDATE tasks SET book_id = NULL WHERE book_id = ?")
            .bind(&id)
            .execute(&self.db.pool)
            .await;

        Ok(())
    }

    pub async fn update_metadata(
        &self,
        id: String,
        metadata: BookMetadata,
    ) -> Result<Book, AppError> {
        let mut book = sqlx::query_as::<_, Book>("SELECT * FROM books WHERE id = ?")
            .bind(&id)
            .fetch_optional(&self.db.pool)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::BookNotFound(id.clone()))?;

        book.title = metadata.title;
        book.updated_at = Utc::now();

        sqlx::query("UPDATE books SET title = ?, updated_at = ? WHERE id = ?")
            .bind(&book.title)
            .bind(book.updated_at.to_rfc3339())
            .bind(&id)
            .execute(&self.db.pool)
            .await
            .map_err(AppError::Db)?;

        Ok(book)
    }

    pub async fn get_book(&self, id: &str) -> Result<Book, AppError> {
        sqlx::query_as::<_, Book>(
            "SELECT books.*, GROUP_CONCAT(tags.name, ',') AS tags \
             FROM books \
             LEFT JOIN book_tags ON book_tags.book_id = books.id \
             LEFT JOIN tags ON tags.id = book_tags.tag_id \
             WHERE books.id = ? \
             GROUP BY books.id",
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::BookNotFound(id.to_string()))
    }

    /// Fetch only the on-disk file path for a book — a lightweight single-column
    /// SELECT by primary key, no JOIN/GROUP_CONCAT. The reader calls this on
    /// every page fetch, so keeping it cheap avoids re-running the heavy tag
    /// aggregation once per page during a rapid page-through.
    pub async fn get_book_file_path(&self, id: &str) -> Result<String, AppError> {
        let row = sqlx::query("SELECT file_path FROM books WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.db.pool)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::BookNotFound(id.to_string()))?;
        let file_path: String = row.try_get("file_path").map_err(AppError::Db)?;
        Ok(file_path)
    }

    /// Like `get_book_file_path` but also returns the stored `page_count`, used
    /// as a fallback when the archive on disk can't be read (missing/corrupt) —
    /// restores the pre-refactor behaviour where the reader fell back to the DB
    /// value instead of reporting 0 pages.
    pub async fn get_book_file_path_and_count(&self, id: &str) -> Result<(String, i32), AppError> {
        let row = sqlx::query("SELECT file_path, page_count FROM books WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.db.pool)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::BookNotFound(id.to_string()))?;
        let file_path: String = row.try_get("file_path").map_err(AppError::Db)?;
        let page_count: i32 = row.try_get("page_count").map_err(AppError::Db)?;
        Ok((file_path, page_count))
    }

    pub async fn list_books(&self, limit: i64, offset: i64) -> Result<Vec<Book>, AppError> {
        sqlx::query_as::<_, Book>(
            "SELECT books.*, GROUP_CONCAT(tags.name, ',') AS tags \
             FROM books \
             LEFT JOIN book_tags ON book_tags.book_id = books.id \
             LEFT JOIN tags ON tags.id = book_tags.tag_id \
             GROUP BY books.id \
             ORDER BY books.created_at DESC \
             LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db.pool)
        .await
        .map_err(AppError::Db)
    }

    pub async fn get_cover(&self, id: &str) -> Result<Vec<u8>, AppError> {
        self.storage
            .read_cover(id)
            .ok_or_else(|| AppError::NotFound(format!("Cover for {}", id)))
    }

    /// Low-res cover thumbnail (longest edge ≤ 256px JPEG) for the library
    /// grid — cheap to ship over IPC and easy to cache client-side.
    pub async fn get_cover_thumb(&self, id: &str) -> Result<Vec<u8>, AppError> {
        self.storage
            .read_cover_thumb(id, 256)
            .ok_or_else(|| AppError::NotFound(format!("Cover for {}", id)))
    }

    /// Full removal of a book: its row, tag links, and on-disk CB7 + cover.
    /// Used when re-downloading an updated artwork so it can be re-registered
    /// under the same book_id.
    pub async fn remove_book(&self, id: &str) -> Result<(), AppError> {
        let book = match self.get_book(id).await {
            Ok(b) => b,
            Err(AppError::BookNotFound(_)) => return Ok(()),
            Err(e) => return Err(e),
        };
        self.storage
            .delete_book(Path::new(&book.file_path), id)
            .map_err(|e| AppError::Other(e.to_string()))?;
        sqlx::query("DELETE FROM book_tags WHERE book_id = ?")
            .bind(id)
            .execute(&self.db.pool)
            .await
            .map_err(AppError::Db)?;
        sqlx::query("DELETE FROM books WHERE id = ?")
            .bind(id)
            .execute(&self.db.pool)
            .await
            .map_err(AppError::Db)?;
        Ok(())
    }

    /// Register an already-stored CB7 (or other) file into the library without
    /// copying. Used by downloaders (e.g. Pixiv) that produce their own CB7.
    /// `book_id` should already be reflected in the file name on disk.
    pub async fn register_stored_book(
        &self,
        book_id: &str,
        title: &str,
        file_path: &Path,
        page_count: i32,
        source: Option<&BookSource>,
        tags: &[String],
        delays: Option<&str>,
    ) -> Result<Book, AppError> {
        let file_size = std::fs::metadata(file_path)
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        let cover_path = self
            .storage
            .extract_cover(file_path, book_id)
            .ok()
            .map(|p| p.to_string_lossy().to_string());
        let now = Utc::now();

        let source_plugin = source.as_ref().map(|s| s.plugin.clone()).unwrap_or_default();
        let source_url = source.as_ref().map(|s| s.source_url.clone()).unwrap_or_default();
        let scraped_at = source.as_ref().and_then(|s| s.scraped_at);
        let source_post_id = source.as_ref().and_then(|s| s.source_post_id.clone());
        let author = source.as_ref().and_then(|s| s.author.clone());
        let author_id = source.as_ref().and_then(|s| s.author_id.clone());
        let published_at = source.as_ref().and_then(|s| s.published_at.clone());

        sqlx::query(
            r#"INSERT INTO books
            (id, title, file_path, file_size, format, page_count, cover_path,
             source_plugin, source_url, scraped_at, source_post_id, author,
             author_id, published_at, created_at, updated_at, delays)
            VALUES (?, ?, ?, ?, 'cb7', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(book_id)
        .bind(title)
        .bind(file_path.to_string_lossy().as_ref())
        .bind(file_size)
        .bind(page_count)
        .bind(cover_path)
        .bind(&source_plugin)
        .bind(&source_url)
        .bind(scraped_at.map(|t| t.to_rfc3339()))
        .bind(&source_post_id)
        .bind(&author)
        .bind(&author_id)
        .bind(&published_at)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(delays)
        .execute(&self.db.pool)
        .await
        .map_err(AppError::Db)?;

        for tag_name in tags {
            upsert_tag_and_link(&self.db.pool, book_id, tag_name, "custom").await?;
        }

        let book = self.get_book(book_id).await?;
        Ok(book)
    }

    /// Link tags to an already-registered book (idempotent via ON CONFLICT).
    /// Used to backfill tags for books registered before tag scraping worked.
    pub async fn link_tags(&self, book_id: &str, tags: &[String]) -> Result<(), AppError> {
        for tag_name in tags {
            upsert_tag_and_link(&self.db.pool, book_id, tag_name, "custom").await?;
        }
        Ok(())
    }

    /// Mark a book as just-read and open a fresh reading session for it.
    ///
    /// Bumps `last_read_at` + `read_count` (the routine read marker) and inserts a
    /// new `reading_sessions` row whose `duration_ms` starts at 0; the returned
    /// session id is later passed to `record_reading` when the book is closed so
    /// the span's duration can be finalized. The two writes are independent: a
    /// failure to open a session must not roll back the read marker.
    pub async fn open_book(&self, id: &str) -> Result<i64, AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE books SET last_read_at = ?, read_count = read_count + 1 WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(&self.db.pool)
            .await
            .map_err(AppError::Db)?;

        // RETURNING id yields the freshly inserted row's id on the SAME
        // connection that ran the INSERT. This replaces a separate
        // `SELECT last_insert_rowid()`, which is per-connection and unsafe across
        // sqlx's pool (max_connections=8): the INSERT and that SELECT could land
        // on different connections, returning some OTHER connection's last rowid
        // (e.g. a concurrent tasks insert) or 0. A wrong id meant record_reading's
        // `WHERE id = ?` never matched the real session row, so every session was
        // left at duration_ms=0 / ended_at=NULL — the Home "本周已阅读" stayed 0.
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO reading_sessions (book_id, started_at, duration_ms) \
             VALUES (?, ?, 0) \
             RETURNING id",
        )
        .bind(id)
        .bind(&now)
        .fetch_one(&self.db.pool)
        .await
        .map_err(AppError::Db)?;
        Ok(id)
    }

    /// Finalize a reading span: stamp `ended_at` and the session's
    /// `duration_ms` (the per-session delta reported by the reader). Scoped to
    /// both the session id and its book so a stale session id can't mutate
    /// another book's row.
    pub async fn record_reading(
        &self,
        id: &str,
        session_id: i64,
        duration_ms: i64,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE reading_sessions SET ended_at = ?, duration_ms = ? WHERE id = ? AND book_id = ?",
        )
        .bind(&now)
        .bind(duration_ms)
        .bind(session_id)
        .bind(id)
        .execute(&self.db.pool)
        .await
        .map_err(AppError::Db)?;
        Ok(())
    }

    /// Crash recovery: any session left open (NULL `ended_at`) means the app
    /// closed without recording. Neutralize them with a zero-length closed span
    /// so they still render in history but contribute 0 to duration stats.
    pub async fn close_stale_sessions(&self) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE reading_sessions \
             SET ended_at = started_at, duration_ms = 0 \
             WHERE ended_at IS NULL",
        )
        .execute(&self.db.pool)
        .await
        .map_err(AppError::Db)?;
        Ok(())
    }

    /// Total reading duration (ms) for the current week (Monday 00:00 local →
    /// now). Aggregated purely from `reading_sessions`; returns 0 when there are
    /// no sessions in the window.
    pub async fn get_weekly_reading_ms(&self) -> Result<i64, AppError> {
        let week_start = monday_start_local().to_rfc3339();
        let total: Option<i64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(duration_ms), 0) \
             FROM reading_sessions \
             WHERE started_at >= ?",
        )
        .bind(&week_start)
        .fetch_one(&self.db.pool)
        .await
        .map_err(AppError::Db)?;
        Ok(total.unwrap_or(0))
    }

    /// Most-recently-read books first (those with a `last_read_at`), for the
    /// home "recently read" shelf. Reuses the same `books.* + tags` projection
    /// the rest of the library uses so the `Book` FromRow mapping lines up.
    pub async fn list_recent_books(&self, limit: i64) -> Result<Vec<Book>, AppError> {
        sqlx::query_as::<_, Book>(
            "SELECT books.*, GROUP_CONCAT(tags.name, ',') AS tags \
             FROM books \
             LEFT JOIN book_tags ON book_tags.book_id = books.id \
             LEFT JOIN tags ON tags.id = book_tags.tag_id \
             WHERE books.last_read_at IS NOT NULL \
             GROUP BY books.id \
             ORDER BY books.last_read_at DESC \
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.db.pool)
        .await
        .map_err(AppError::Db)
    }

    /// The single book with the highest cumulative reading duration over the
    /// last `days` days — the "近 N 天最爱" pick. Ties break by most recent
    /// `last_read_at`. Returns None when nothing's been read in the window.
    pub async fn get_recent_favorite_book(&self, days: i64) -> Result<Option<Book>, AppError> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let book = sqlx::query_as::<_, Book>(
            "SELECT books.*, GROUP_CONCAT(tags.name, ',') AS tags \
             FROM books \
             JOIN reading_sessions rs ON rs.book_id = books.id \
             LEFT JOIN book_tags ON book_tags.book_id = books.id \
             LEFT JOIN tags ON tags.id = book_tags.tag_id \
             WHERE rs.started_at >= ? \
             GROUP BY books.id \
             ORDER BY SUM(rs.duration_ms) DESC, books.last_read_at DESC \
             LIMIT 1",
        )
        .bind(&since)
        .fetch_optional(&self.db.pool)
        .await
        .map_err(AppError::Db)?;
        Ok(book)
    }
}

/// Monday 00:00:00 in the local timezone, as a UTC DateTime — the start of the
/// current week for the "本周阅读时长" stat. RFC3339-comparable against the
/// UTC `started_at` strings stored in `reading_sessions`.
fn monday_start_local() -> DateTime<Utc> {
    let now = Local::now();
    let days_from_monday = now.weekday().num_days_from_monday() as i64;
    let monday = now.date_naive() - chrono::Duration::days(days_from_monday);
    let monday_local = monday
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is a valid time");
    Local
        .from_local_datetime(&monday_local)
        .single()
        .expect("midnight is unambiguous")
        .with_timezone(&Utc)
}

/// Parse an RFC3339 timestamp (as written into ComicInfo's ero:ScrapedAt) back
/// into a UTC DateTime. Returns None on malformed input so import degrades
/// gracefully instead of failing the whole book.
fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn detect_format(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with(".cb7") {
        "cb7".into()
    } else if lower.ends_with(".cbz") {
        "cbz".into()
    } else if lower.ends_with(".cbr") {
        "cbr".into()
    } else if lower.ends_with(".pdf") {
        "pdf".into()
    } else {
        "cb7".into()
    }
}

fn count_archive_pages(path: &Path, format: &str) -> Option<i32> {
    if format == "cb7" || format == "cbz" {
        let file = std::fs::File::open(path).ok()?;
        let mut archive = zip::ZipArchive::new(file).ok()?;
        let mut count = 0;
        for i in 0..archive.len() {
            let Ok(entry) = archive.by_index(i) else {
                continue;
            };
            let name = entry.name().to_lowercase();
            if name.ends_with(".jpg")
                || name.ends_with(".jpeg")
                || name.ends_with(".png")
                || name.ends_with(".webp")
            {
                count += 1;
            }
        }
        Some(count)
    } else {
        None
    }
}

async fn upsert_tag_and_link(
    pool: &sqlx::Pool<Sqlite>,
    book_id: &str,
    tag_name: &str,
    tag_type: &str,
) -> Result<(), AppError> {
    let tag_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO tags (id, name, type) VALUES (?, ?, ?)
           ON CONFLICT(name) DO UPDATE SET type = excluded.type"#,
    )
    .bind(&tag_id)
    .bind(tag_name)
    .bind(tag_type)
    .execute(pool)
    .await
    .ok();

    // Fetch the (possibly existing) tag id.
    let row: (String,) = sqlx::query_as("SELECT id FROM tags WHERE name = ?")
        .bind(tag_name)
        .fetch_one(pool)
        .await
        .map_err(AppError::Db)?;
    let tag_id = row.0;

    sqlx::query(
        "INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?, ?)",
    )
    .bind(book_id)
    .bind(&tag_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}
