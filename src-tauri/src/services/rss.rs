use std::sync::Arc;

use crate::db::Database;
use crate::errors::AppError;
use crate::models::Book;

/// Generates an RSS 2.0 feed from the local library. The running HTTP server
/// (spawned by `start_rss_server`) uses this to render the `/rss` route.
pub struct RssService {
    pub(crate) db: Arc<Database>,
    base_url: std::sync::Mutex<String>,
}

impl RssService {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            base_url: std::sync::Mutex::new("http://localhost:8081".to_string()),
        }
    }

    pub fn set_base_url(&self, url: String) {
        if let Ok(mut guard) = self.base_url.lock() {
            *guard = url;
        }
    }

    pub async fn feed(&self) -> Result<String, AppError> {
        let books = sqlx::query_as::<_, Book>(
            "SELECT * FROM books ORDER BY created_at DESC",
        )
        .fetch_all(&self.db.pool)
        .await
        .map_err(AppError::Db)?;

        let base = self.base_url.lock().map(|s| s.clone()).unwrap_or_default();
        Ok(render_feed(&base, &books))
    }
}

fn render_feed(base: &str, books: &[Book]) -> String {
    let now = chrono::Utc::now();
    let build_date = now.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let mut items = String::new();

    for book in books {
        let pub_date = book
            .created_at
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let download = format!("{}/download/{}", base, book.id);
        let cover = format!("{}/covers/{}", base, book.id);
        let description = format!(
            "{} pages · {}",
            book.page_count,
            book.format.to_uppercase()
        );
        let enclosure_type = match book.format.to_lowercase().as_str() {
            "cbz" => "application/x-cbz",
            "cbr" => "application/x-cbr",
            "pdf" => "application/pdf",
            _ => "application/x-cb7",
        };

        items.push_str(&format!(
            r#"<item>
      <title>{}</title>
      <link>{}</link>
      <guid isPermaLink="false">urn:uuid:{}</guid>
      <pubDate>{}</pubDate>
      <description>{}</description>
      <enclosure url="{}" length="{}" type="{}"/>
      <media:thumbnail url="{}"/>
    </item>"#,
            xml_escape(&book.title),
            download,
            book.id,
            pub_date,
            xml_escape(&description),
            download,
            book.file_size,
            enclosure_type,
            cover,
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:media="http://search.yahoo.com/mrss/">
  <channel>
    <title>EroLib Library</title>
    <link>{}</link>
    <description>EroLib 本地书库 RSS 订阅</description>
    <language>zh-cn</language>
    <lastBuildDate>{}</lastBuildDate>
    <generator>EroLib</generator>
    {}
  </channel>
</rss>"#,
        base, build_date, items
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
