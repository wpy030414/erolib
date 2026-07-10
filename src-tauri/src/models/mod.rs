use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Source metadata for a book obtained from scraper/plugins/userscripts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BookSource {
    pub plugin: String,
    pub source_url: String,
    pub scraped_at: Option<DateTime<Utc>>,
}

/// A book in the library.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
    pub scraped_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
    pub read_count: i32,
    /// Comma-joined tag names (from a join), used for FTS + display.
    #[sqlx(default)]
    pub tags: Option<String>,
}

/// A tag that can be attached to books.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub tag_type: String,
    pub created_at: DateTime<Utc>,
}

/// A named collection of books.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
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
