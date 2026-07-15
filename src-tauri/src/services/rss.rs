use std::sync::Arc;

use crate::db::Database;
use crate::errors::AppError;
use crate::models::Book;

use super::feed::xml_escape;

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
        let article = format!("{}/article/{}", base, book.id);
        // Rich per-field metadata blurb, shared with OPDS via the `feed` module,
        // CDATA-wrapped so the structural <br>/<a> stay intact.
        let description = format!("<![CDATA[{}]]>", super::feed::book_metadata_blurb(&book));
        let enclosure_type = match book.format.to_lowercase().as_str() {
            "cbz" => "application/x-cbz",
            "cbr" => "application/x-cbr",
            "pdf" => "application/pdf",
            _ => "application/x-cb7",
        };

        // Full-page image strip embedded as the article body so RSS readers
        // render the book as an image sequence. Without `<content:encoded>`,
        // readers fall back to fetching `<link>` — formerly the raw cb7 — and
        // showed garbled bytes. `<link>` now points at the HTML gallery route.
        let mut content = String::new();
        for n in 0..book.page_count.max(1) as usize {
            content.push_str(&format!(
                r#"<img src="{}/pages/{}/{}" alt="page {}" loading="lazy" style="max-width:100%;height:auto;display:block"/>"#,
                base, book.id, n, n + 1,
            ));
        }

        items.push_str(&format!(
            r#"<item>
      <title>{}</title>
      <link>{}</link>
      <guid isPermaLink="false">urn:uuid:{}</guid>
      <pubDate>{}</pubDate>
      <description>{}</description>
      <content:encoded><![CDATA[{}]]></content:encoded>
      <enclosure url="{}" length="{}" type="{}"/>
      <media:thumbnail url="{}" type="image/jpeg"/>
    </item>"#,
            xml_escape(&book.title),
            article,
            book.id,
            pub_date,
            description,
            content,
            download,
            book.file_size,
            enclosure_type,
            cover,
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:media="http://search.yahoo.com/mrss/" xmlns:content="http://purl.org/rss/1.0/modules/content/">
  <channel>
    <title>EroLib</title>
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
