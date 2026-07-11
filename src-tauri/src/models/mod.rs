use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Source metadata for a book obtained from scraper/plugins/userscripts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BookSource {
    pub plugin: String,
    pub source_url: String,
    pub scraped_at: Option<DateTime<Utc>>,
    /// Source-site post/work id (Pixiv illust id, EHentai gid).
    pub source_post_id: Option<String>,
    /// Author display name (Pixiv userName, EHentai uploader).
    pub author: Option<String>,
    /// Author id on the source site (Pixiv userId; EHentai has none).
    pub author_id: Option<String>,
    /// Website publish time as stored by the source (RFC3339 / site-local
    /// string). NOT the scrape or import time.
    pub published_at: Option<String>,
}

/// A book in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub title: String,
    pub original_filename: Option<String>,
    pub file_path: String,
    pub file_size: i64,
    pub format: String,
    pub page_count: i32,
    pub cover_path: Option<String>,
    pub source_plugin: Option<String>,
    pub source_url: Option<String>,
    pub source_post_id: Option<String>,
    pub author: Option<String>,
    pub author_id: Option<String>,
    /// Website publish time (RFC3339 / site-local string); not scrape/import.
    pub published_at: Option<String>,
    pub scraped_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
    pub read_count: i32,
    /// Comma-joined tag names (from a join), used for FTS + display.
    /// `#[sqlx(default)]`-equivalent: None when the column is absent.
    pub tags: Option<String>,
    /// Ugoira frame delays in ms, JSON-encoded (e.g. "[60,60,70,...]"). Present
    /// only for animated Pixiv works stored as a jpg sequence; the reader parses
    /// & plays them on a timer.
    pub delays: Option<String>,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for Book {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            title: row.try_get("title")?,
            original_filename: row.try_get("original_filename")?,
            file_path: row.try_get("file_path")?,
            file_size: row.try_get("file_size")?,
            format: row.try_get("format")?,
            page_count: row.try_get("page_count")?,
            cover_path: row.try_get("cover_path")?,
            source_plugin: row.try_get("source_plugin")?,
            source_url: row.try_get("source_url")?,
            source_post_id: row.try_get("source_post_id")?,
            author: row.try_get("author")?,
            author_id: row.try_get("author_id")?,
            published_at: row.try_get("published_at")?,
            scraped_at: row.try_get("scraped_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            last_read_at: row.try_get("last_read_at")?,
            read_count: row.try_get("read_count")?,
            // `tags` comes from an optional JOIN; default to None when absent.
            tags: row.try_get::<Option<String>, _>("tags").unwrap_or(None),
            delays: row.try_get::<Option<String>, _>("delays").unwrap_or(None),
        })
    }
}

/// A tag that can be attached to books.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub tag_type: String,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for Tag {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            tag_type: row.try_get("tag_type")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

/// A tag paired with how many books use it, for the tag-chip filter UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagCount {
    pub name: String,
    pub count: i64,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for TagCount {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            name: row.try_get("name")?,
            count: row.try_get("count")?,
        })
    }
}

/// A named collection of books.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for Collection {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

/// Metadata extracted from a scraped source.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BookMetadata {
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub status: Option<String>,
    pub rating: Option<f32>,
    // erolib provenance — written under the ero: namespace in ComicInfo.xml so
    // an exported cb7 round-trips losslessly back on import.
    pub source_plugin: Option<String>,
    pub source_url: Option<String>,
    pub source_post_id: Option<String>,
    pub published_at: Option<String>,
    pub scraped_at: Option<String>,
    /// Ugoira frame delays in ms, JSON-encoded (animated books only).
    pub delays: Option<String>,
}

/// Query parameters for searching books.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchQuery {
    pub text: Option<String>,
    pub tags: Option<Vec<String>>,
    pub tags_any: Option<Vec<String>>,
    pub collections: Option<Vec<String>>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub sources: Option<Vec<String>>,
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
    #[serde(default = "default_sort_order")]
    pub sort_order: String,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
}

fn default_sort_by() -> String {
    "relevance".to_string()
}
fn default_sort_order() -> String {
    "desc".to_string()
}
fn default_page() -> i64 {
    1
}
fn default_page_size() -> i64 {
    50
}

/// A page of search results plus facet counts.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub books: Vec<Book>,
    pub total: i64,
    pub facets: SearchFacets,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SearchFacets {
    pub tags: Vec<Tag>,
    pub collections: Vec<Collection>,
    pub sources: Vec<String>,
}
