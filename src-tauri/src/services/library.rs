use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use sqlx::Sqlite;
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
    pub async fn import_book(&self, file_path: String) -> Result<Book, AppError> {
        let path = Path::new(&file_path);
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let metadata = std::fs::metadata(path).map_err(AppError::Io)?;
        let format = detect_format(&file_name);
        let page_count = count_archive_pages(path, &format).unwrap_or(0);

        let book_id = Uuid::new_v4().to_string();
        let dest = self
            .storage
            .library_path
            .join(format!("{}.cb7", book_id));

        // Copy into library storage.
        std::fs::copy(path, &dest).map_err(AppError::Io)?;

        // Extract cover.
        let cover_path = self
            .storage
            .extract_cover(&dest, &book_id)
            .ok()
            .map(|p| p.to_string_lossy().to_string());

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&file_name)
            .to_string();

        let now = Utc::now();
        let book = Book {
            id: book_id.clone(),
            title: title.clone(),
            original_filename: Some(file_name),
            file_path: dest.to_string_lossy().to_string(),
            file_size: metadata.len() as i64,
            format,
            page_count,
            cover_path,
            source_plugin: None,
            source_url: None,
            source_post_id: None,
            author: None,
            author_id: None,
            published_at: None,
            scraped_at: None,
            created_at: now,
            updated_at: now,
            last_read_at: None,
            read_count: 0,
            tags: None,
        };

        sqlx::query(
            r#"INSERT INTO books
            (id, title, original_filename, file_path, file_size, format, page_count, cover_path, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&book.id)
        .bind(&book.title)
        .bind(&book.original_filename)
        .bind(&book.file_path)
        .bind(book.file_size)
        .bind(&book.format)
        .bind(book.page_count)
        .bind(&book.cover_path)
        .bind(book.created_at.to_rfc3339())
        .bind(book.updated_at.to_rfc3339())
        .execute(&self.db.pool)
        .await
        .map_err(AppError::Db)?;

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
            page_count: images.len() as i32,
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
             author_id, published_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, 'cb7', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
