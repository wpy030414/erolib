use std::sync::Arc;

use crate::db::Database;
use crate::errors::AppError;
use crate::models::Book;

/// Generates OPDS Atom feeds from the book library. The running HTTP server
/// (spawned by the `start_opds_server` command) uses this to render responses.
pub struct OpdsService {
    pub(crate) db: Arc<Database>,
    base_url: std::sync::Mutex<String>,
}

impl OpdsService {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            base_url: std::sync::Mutex::new("http://localhost:8080".to_string()),
        }
    }

    pub fn set_base_url(&self, url: String) {
        if let Ok(mut guard) = self.base_url.lock() {
            *guard = url;
        }
    }

    pub async fn root_feed(&self) -> Result<String, AppError> {
        let books = sqlx::query_as::<_, Book>(
            "SELECT * FROM books ORDER BY created_at DESC",
        )
        .fetch_all(&self.db.pool)
        .await
        .map_err(AppError::Db)?;

        let base = self.base_url.lock().map(|s| s.clone()).unwrap_or_default();
        Ok(render_feed(&base, &books, "EroLib Library", "/opds"))
    }

    pub async fn search_feed(&self, query: &str) -> Result<String, AppError> {
        let pattern = format!("%{}%", query);
        let books = sqlx::query_as::<_, Book>(
            "SELECT * FROM books WHERE title LIKE ? OR original_filename LIKE ? ORDER BY created_at DESC",
        )
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(&self.db.pool)
        .await
        .map_err(AppError::Db)?;

        let base = self.base_url.lock().map(|s| s.clone()).unwrap_or_default();
        Ok(render_feed(&base, &books, &format!("Search: {}", query), "/opds/search"))
    }
}

fn render_feed(base: &str, books: &[Book], title: &str, self_path: &str) -> String {
    let now = chrono::Utc::now().to_rfc3339();
    let mut entries = String::new();
    for book in books {
        let updated = book.updated_at.to_rfc3339();
        let download = format!("{}/download/{}", base, book.id);
        let cover = format!("{}/covers/{}", base, book.id);
        let summary_xml = book
            .original_filename
            .as_deref()
            .unwrap_or("");
        entries.push_str(&format!(
            r#"<entry>
  <title>{}</title>
  <id>urn:uuid:{}</id>
  <updated>{}</updated>
  <link rel="http://opds-spec.org/acquisition" href="{}" type="application/x-cb7"/>
  <link rel="http://opds-spec.org/image/thumbnail" href="{}" type="image/jpeg"/>
  <summary>{}</summary>
</entry>"#,
            xml_escape(&book.title),
            book.id,
            updated,
            download,
            cover,
            xml_escape(summary_xml),
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>urn:uuid:9f3c7a2b-4e1d-4a6b-8c72-3e9f0d1a5b6c</id>
  <title>{}</title>
  <updated>{}</updated>
  <link rel="self" href="{}{}" type="application/atom+xml;profile=opds-catalog"/>
  <link rel="start" href="{}/opds" type="application/atom+xml;profile=opds-catalog"/>
  <link rel="search" href="{}/opds/search/{{searchTerms}}" type="application/atom+xml;profile=opds-catalog"/>
  {}
</feed>"#,
        xml_escape(title),
        now,
        base,
        self_path,
        base,
        base,
        entries,
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
