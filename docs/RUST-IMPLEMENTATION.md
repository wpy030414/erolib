# Rust Implementation Details

This document provides implementation details for the Rust backend of the Manga Manager application.

## Project Structure

```toml
# Cargo.toml
[package]
name = "manga-manager"
version = "0.1.0"
edition = "2021"

[dependencies]
tauri = { version = "2", features = ["shell-open"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.7", features = ["sqlite", "runtime-tokio"] }
sea-orm = { version = "0.12", features = ["sqlx-sqlite", "runtime-tokio"] }
uuid = { version = "1.0", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.11", features = ["json"] }
scraper = "0.17"
select = "0.6"
axum = "0.7"
tower = "0.4"
tower-http = { version = "0.5", features = ["fs", "cors"] }
rust-embed = "8.0"
zip = { version = "0.6", features = ["deflate"] }
quick-xml = { version = "0.31", features = ["serialize"] }
thiserror = "1.0"
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
config = "0.14"
dirs = "5.0"

# Scripting
boa_engine = "0.17"
```

## Database Schema

```sql
-- books.sql
CREATE TABLE books (
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
    scraped_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_read_at TIMESTAMP,
    read_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    type TEXT NOT NULL DEFAULT 'custom',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE book_tags (
    book_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (book_id, tag_id),
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE collection_books (
    collection_id TEXT NOT NULL,
    book_id TEXT NOT NULL,
    PRIMARY KEY (collection_id, book_id),
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

-- Full-text search
CREATE VIRTUAL TABLE books_fts USING fts5(
    title,
    original_filename,
    tags,
    content='books',
    content_rowid='id'
);

-- Triggers
CREATE TRIGGER books_ai AFTER INSERT ON books BEGIN
    INSERT INTO books_fts(rowid, title, original_filename)
    VALUES (new.id, new.title, new.original_filename);
END;

CREATE TRIGGER books_ad AFTER DELETE ON books BEGIN
    DELETE FROM books_fts WHERE rowid = old.id;
END;

CREATE TRIGGER books_au AFTER UPDATE ON books BEGIN
    UPDATE books_fts
    SET title = new.title, original_filename = new.original_filename
    WHERE rowid = new.id;
END;

-- Scraper jobs
CREATE TABLE scraper_plugins (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    version TEXT NOT NULL,
    author TEXT,
    description TEXT,
    config TEXT NOT NULL,  -- JSON
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE scraper_jobs (
    id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    items_processed INTEGER NOT NULL DEFAULT 0,
    items_failed INTEGER NOT NULL DEFAULT 0,
    errors TEXT,  -- JSON array
    FOREIGN KEY (plugin_id) REFERENCES scraper_plugins(id) ON DELETE CASCADE
);

-- Schedules
CREATE TABLE scraper_schedules (
    id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    frequency TEXT NOT NULL,
    cron_expression TEXT,
    filters TEXT,  -- JSON
    download_options TEXT,  -- JSON
    last_run_at TIMESTAMP,
    next_run_at TIMESTAMP,
    FOREIGN KEY (plugin_id) REFERENCES scraper_plugins(id) ON DELETE CASCADE
);
```

## Database Models

```rust
// models/mod.rs
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "books")]
pub struct Model {
    #[sea_orm(primary_key)]
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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::book_tags::Entity")]
    BookTags,
    #[sea_orm(
        has_many = "super::collection_books::Entity"
    )]
    CollectionBooks,
}

impl Related<super::book_tags::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BookTags.to()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

## Plugin System

```rust
// plugins/base.rs
use async_trait::async_trait;
use anyhow::Result;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ScraperResult {
    pub title: String,
    pub url: String,
    pub thumbnail_url: Option<String>,
    pub metadata: Option<BookMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookMetadata {
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub status: Option<String>,
    pub rating: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub images: Vec<Vec<u8>>,
    pub metadata: BookMetadata,
}

#[async_trait]
pub trait ScraperPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    
    async fn search(&self, query: &str) -> Result<Vec<ScraperResult>>;
    async fn get_chapter_list(&self, url: &str) -> Result<Vec<ScraperResult>>;
    async fn download_chapter(&self, url: &str) -> Result<DownloadResult>;
    
    fn validate_config(&self, config: &Value) -> Result<()>;
}
```

## Generic HTML Scraper

```rust
// plugins/generic_html.rs
use super::base::{ScraperPlugin, ScraperResult, DownloadResult};
use scraper::{Html, Selector};
use reqwest::Client;
use anyhow::Result;

pub struct GenericHtmlScraper {
    id: String,
    name: String,
    client: Client,
    config: GenericHtmlConfig,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GenericHtmlConfig {
    base_url: String,
    search: Option<SearchConfig>,
    chapter_list: ChapterListConfig,
    image_list: ImageListConfig,
    rate_limit: Option<u64>,
}

#[async_trait::impl_for_trait]
impl ScraperPlugin for GenericHtmlScraper {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn version(&self) -> &str { "1.0.0" }
    
    async fn search(&self, query: &str) -> Result<Vec<ScraperResult>> {
        let search_config = self.config.search.as_ref()
            .ok_or_else(|| anyhow!("Search not configured"))?;
        
        let url = search_config.url_template
            .replace("{query}", &urlencoding::encode(query))
            .replace("{page}", "1");
        
        let response = self.client.get(&url).send().await?;
        let html = response.text().await?;
        let document = Html::parse_document(&html);
        
        let container = Selector::parse(&search_config.results.container)?;
        let title_selector = Selector::parse(&search_config.results.title)?;
        let url_selector = Selector::parse(&search_config.results.url)?;
        
        let mut results = Vec::new();
        for element in document.select(&container) {
            let title = element
                .select(&title_selector)
                .next()
                .map(|e| e.text().collect::<String>())
                .unwrap_or_default();
            
            let url = element
                .select(&url_selector)
                .next()
                .and_then(|e| e.value().attr("href"))
                .map(|href| Self::resolve_url(&self.config.base_url, href))
                .unwrap_or_default();
            
            results.push(ScraperResult {
                title,
                url,
                thumbnail_url: None,
                metadata: None,
            });
        }
        
        Ok(results)
    }
    
    async fn download_chapter(&self, url: &str) -> Result<DownloadResult> {
        let response = self.client.get(url).send().await?;
        let html = response.text().await?;
        let document = Html::parse_document(&html);
        
        let container = Selector::parse(&self.config.image_list.container)?;
        let url_selector = Selector::parse(&self.config.image_list.url)?;
        
        let mut images = Vec::new();
        for element in document.select(&container) {
            let src = element
                .select(&url_selector)
                .next()
                .and_then(|e| {
                    self.config.image_list.url.strip_suffix("@src")
                        .map(|_| e.value().attr("src"))
                        .or_else(|| Some(e.text().collect::<String>().trim()))
                        .flatten()
                });
            
            if let Some(src) = src {
                let image_url = Self::resolve_url(&self.config.base_url, src);
                let img_resp = self.client
                    .get(&image_url)
                    .header("Referer", url)
                    .send()
                    .await?;
                let bytes = img_resp.bytes().await?.to_vec();
                images.push(bytes);
            }
        }
        
        Ok(DownloadResult {
            images,
            metadata: BookMetadata {
                title: "Chapter".to_string(),
                author: None,
                artist: None,
                description: None,
                tags: Vec::new(),
                status: None,
                rating: None,
            },
        })
    }
    
    fn validate_config(&self, _config: &Value) -> Result<()> {
        Ok(())
    }
}

impl GenericHtmlScraper {
    fn resolve_url(base: &str, href: &str) -> String {
        if href.starts_with("http://") || href.starts_with("https://") {
            href.to_string()
        } else {
            format!("{}{}", base.trim_end_matches('/'), href)
        }
    }
}
```

## CB7 Storage Service

```rust
// services/storage.rs
use anyhow::Result;
use std::path::{Path, PathBuf};
use zip::{ZipWriter, write::FileOptions};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read};
use uuid::Uuid;

pub struct StorageService {
    library_path: PathBuf,
    cache_path: PathBuf,
    cover_path: PathBuf,
}

impl StorageService {
    pub fn new(base_path: PathBuf) -> Result<Self> {
        let library_path = base_path.join("library");
        let cache_path = base_path.join("cache");
        let cover_path = base_path.join("covers");
        
        std::fs::create_dir_all(&library_path)?;
        std::fs::create_dir_all(&cache_path)?;
        std::fs::create_dir_all(&cover_path)?;
        
        Ok(Self {
            library_path,
            cache_path,
            cover_path,
        })
    }
    
    pub async fn create_cb7(
        &self,
        images: Vec<Vec<u8>>,
        metadata: &BookMetadata,
    ) -> Result<PathBuf> {
        let book_id = Uuid::new_v4().to_string();
        let file_path = self.library_path.join(format!("{}.cb7", book_id));
        
        let file = File::create(&file_path)?;
        let mut zip = ZipWriter::new(BufWriter::new(file));
        
        // Add ComicInfo.xml
        let comic_info = Self::create_comic_info(metadata);
        zip.start_file("ComicInfo.xml", FileOptions::default())?;
        zip.write_all(comic_info.as_bytes())?;
        
        // Add images
        for (index, image) in images.iter().enumerate() {
            let filename = format!("{:04}.jpg", index + 1);
            zip.start_file(&filename, FileOptions::default())?;
            zip.write_all(image)?;
        }
        
        zip.finish()?;
        
        Ok(file_path)
    }
    
    pub async fn extract_cover(&self, cb7_path: &Path, book_id: &str) -> Result<PathBuf> {
        let cover_path = self.cover_path.join(format!("{}.jpg", book_id));
        
        let mut archive = zip::ZipArchive::new(File::open(cb7_path)?)?;
        
        // Find first image file
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_lowercase();
            if name.ends_with(".jpg") || name.ends_with(".jpeg") || name.ends_with(".png") {
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)?;
                
                let mut cover_file = File::create(&cover_path)?;
                cover_file.write_all(&buffer)?;
                
                return Ok(cover_path);
            }
        }
        
        anyhow::bail!("No cover image found in archive")
    }
    
    pub async fn delete_book(&self, book_id: &str, file_path: &Path) -> Result<()> {
        // Delete CB7 file
        if file_path.exists() {
            std::fs::remove_file(file_path)?;
        }
        
        // Delete cover
        let cover_path = self.cover_path.join(format!("{}.jpg", book_id));
        if cover_path.exists() {
            std::fs::remove_file(cover_path)?;
        }
        
        Ok(())
    }
    
    fn create_comic_info(metadata: &BookMetadata) -> String {
        format!(
            r#"<?xml version="1.0"?>
<ComicInfo xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <Title>{}</Title>
    <Writer>{}</Writer>
    <Penciller>{}</Penciller>
    <Summary>{}</Summary>
    <Tags>{}</Tags>
</ComicInfo>"#,
            xml_escape(&metadata.title),
            xml_escape(&metadata.author.as_deref().unwrap_or("")),
            xml_escape(&metadata.artist.as_deref().unwrap_or("")),
            xml_escape(&metadata.description.as_deref().unwrap_or("")),
            xml_escape(&metadata.tags.join(", "))
        )
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
```

## OPDS Service

```rust
// services/opds.rs
use axum::{
    response::{IntoResponse, Response},
    Json,
    extract::{Path, Query},
};
use quick_xml::se::to_string;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize)]
struct OpdsFeed {
    #[serde(rename = "@xmlns")]
    xmlns: &'static str,
    #[serde(rename = "@xmlns:opds")]
    xmlns_opds: &'static str,
    id: String,
    title: String,
    updated: DateTime<Utc>,
    #[serde(rename = "link")]
    links: Vec<OpdsLink>,
    #[serde(rename = "entry")]
    entries: Vec<OpdsEntry>,
}

#[derive(Debug, Serialize)]
struct OpdsLink {
    #[serde(rename = "@rel")]
    rel: String,
    #[serde(rename = "@href")]
    href: String,
    #[serde(rename = "@type", skip_serializing_if = "Option::is_none")]
    type_: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpdsEntry {
    title: String,
    id: String,
    updated: DateTime<Utc>,
    #[serde(rename = "link")]
    links: Vec<OpdsLink>,
    #[serde(rename = "author")]
    authors: Vec<OpdsAuthor>,
    #[serde(rename = "category", skip_serializing_if = "Vec::is_empty")]
    categories: Vec<OpdsCategory>,
    #[serde(rename = "summary", skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpdsAuthor {
    name: String,
}

#[derive(Debug, Serialize)]
struct OpdsCategory {
    #[serde(rename = "@term")]
    term: String,
    #[serde(rename = "@scheme", skip_serializing_if = "Option::is_none")]
    scheme: Option<String>,
}

pub struct OpdsService {
    db: Db,
    base_url: String,
}

impl OpdsService {
    pub async fn root_feed(&self) -> Result<OpdsFeed> {
        let books = self.db.get_all_books().await?;
        
        let entries = books.into_iter().map(|book| {
            OpdsEntry {
                title: book.title,
                id: book.id,
                updated: book.updated_at,
                links: vec![
                    OpdsLink {
                        rel: "http://opds-spec.org/acquisition".to_string(),
                        href: format!("/download/{}", book.id),
                        type_: Some("application/x-cb7".to_string()),
                    },
                    OpdsLink {
                        rel: "http://opds-spec.org/image/thumbnail".to_string(),
                        href: format!("/covers/{}", book.id),
                        type_: Some("image/jpeg".to_string()),
                    },
                ],
                authors: vec![],
                categories: vec![],
                summary: None,
            }
        }).collect();
        
        Ok(OpdsFeed {
            xmlns: "http://www.w3.org/2005/Atom",
            xmlns_opds: "http://opds-spec.org/2010/catalog",
            id: "urn:uuid:manga-manager".to_string(),
            title: "Manga Manager Library".to_string(),
            updated: Utc::now(),
            links: vec![
                OpdsLink {
                    rel: "self".to_string(),
                    href: "/opds".to_string(),
                    type_: None,
                },
                OpdsLink {
                    rel: "start".to_string(),
                    href: "/opds".to_string(),
                    type_: None,
                },
            ],
            entries,
        })
    }
    
    pub async fn search_feed(&self, query: &str) -> Result<OpdsFeed> {
        let books = self.db.search_books(query).await?;
        // Similar structure to root_feed
        // ...
    }
}

// Axum handler
pub async fn opds_handler(
    State(service): State<Arc<OpdsService>>,
) -> Result<impl IntoResponse, AppError> {
    let feed = service.root_feed().await?;
    let xml = to_string(&feed)?;
    
    Ok((
        [(header::CONTENT_TYPE, "application/atom+xml;profile=opds-catalog")],
        xml,
    ))
}
```

## Tauri Commands

```rust
// commands/book.rs
use tauri::State;

#[tauri::command]
async fn import_book(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<Book, String> {
    let book = state.library_service.import_book(file_path).await
        .map_err(|e| e.to_string())?;
    Ok(book)
}

#[tauri::command]
async fn search_books(
    query: SearchQuery,
    state: State<'_, AppState>,
) -> Result<SearchResult, String> {
    let result = state.search_service.search(query).await
        .map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command]
async fn start_opds_server(
    port: u16,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state.opds_service.start(port).await
        .map_err(|e| e.to_string())?;
    Ok(format!("http://localhost:{}", port))
}

#[tauri::command]
async fn run_scraper(
    plugin_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let job_id = state.scraper_orchestrator.run_plugin(plugin_id).await
        .map_err(|e| e.to_string())?;
    Ok(job_id)
}

// Register commands in main.rs
fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            import_book,
            search_books,
            start_opds_server,
            run_scraper,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Error Handling

```rust
// errors.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("Scraping error: {0}")]
    Scraping(String),
    
    #[error("Book not found: {0}")]
    BookNotFound(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::BookNotFound(id) => (StatusCode::NOT_FOUND, format!("Book not found: {}", id)),
            AppError::Scraping(msg) => (StatusCode::BAD_REQUEST, msg),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
```

## Main Entry Point

```rust
// main.rs
mod models;
mod commands;
mod services;
mod plugins;
mod errors;

use services::{LibraryService, SearchService, OpdsService, ScraperOrchestrator};
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    library_service: Arc<LibraryService>,
    search_service: Arc<SearchService>,
    opds_service: Arc<OpdsService>,
    scraper_orchestrator: Arc<ScraperOrchestrator>,
}

impl AppState {
    fn new() -> Result<Self> {
        let db = Arc::new(Database::new().await?);
        
        let storage_path = dirs::config_dir()
            .ok_or_else(|| anyhow!("Could not find config dir"))?
            .join("manga-manager");
        
        let storage = Arc::new(StorageService::new(storage_path)?);
        
        let library_service = Arc::new(LibraryService::new(db.clone(), storage.clone())?);
        let search_service = Arc::new(SearchService::new(db.clone())?);
        let opds_service = Arc::new(OpdsService::new(db.clone())?);
        let scraper_orchestrator = Arc::new(ScraperOrchestrator::new(
            db.clone(),
            storage.clone(),
        )?);
        
        Ok(Self {
            library_service,
            search_service,
            opds_service,
            scraper_orchestrator,
        })
    }
}

#[tokio::main]
async fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let state = AppState::new()?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::book::import_book,
            commands::book::search_books,
            commands::book::start_opds_server,
            commands::scraper::run_scraper,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
