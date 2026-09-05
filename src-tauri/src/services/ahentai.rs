use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;

pub const AHENTAI_BASE: &str = "https://asmhentai.com";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// One row of an asmhentai.com listing (homepage or search results).
/// Serialized camelCase for the frontend browse grid.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AhentaiGalleryItem {
    /// Numeric gallery ID extracted from the `/g/{id}/` URL.
    pub id: String,
    pub title: String,
    /// Full HTTPS thumbnail URL (the `data-src` of `img.lazy`, protocol-relative
    /// URLs are upgraded to `https:`).
    pub thumb_url: String,
    /// Page count is not available on listing pages; always 0 until the gallery
    /// detail page is fetched during download.
    pub page_count: i32,
    /// Uploader/artist name extracted from the title's leading `[Author]` or
    /// `(Circle) [Author]` bracket convention.
    pub uploader: Option<String>,
    /// Category display name, e.g. "Doujinshi", "Manga".
    pub category: String,
}

/// Metadata scraped from a gallery detail page (`/g/{id}/`).
#[derive(Debug, Default, Clone)]
pub struct AhentaiGalleryMeta {
    pub title: String,
    pub page_count: i32,
    pub load_dir: String,
    pub tags: Vec<String>,
    pub artists: Vec<String>,
    pub groups: Vec<String>,
    pub languages: Vec<String>,
    pub category: String,
    pub parodies: Vec<String>,
}

/// Unauthenticated asmhentai.com client. No login is required to browse or
/// search — the site serves public content without a session.
pub struct AhentaiClient {
    http: Client,
}

impl AhentaiClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::default())
            .build()
            .context("build reqwest client")?;
        Ok(Self { http })
    }

    /// Every GET request carries a Referer header pointing at the site root.
    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.http
            .get(url)
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Referer", &format!("{}/", AHENTAI_BASE))
    }

    /// Fetch the homepage (or a specific page via `?page=N`). Page 1 uses the
    /// bare URL without a query parameter. Returns up to 20 items per source
    /// page parsed from `div.preview_item` elements.
    pub async fn fetch_homepage(&self, page: u32) -> Result<Vec<AhentaiGalleryItem>> {
        let url = if page <= 1 {
            format!("{}/", AHENTAI_BASE)
        } else {
            format!("{}/?page={}", AHENTAI_BASE, page)
        };
        self.fetch_listing(&url).await
    }

    /// Search galleries. The trailing slash after `/search/` is **required** —
    /// `/search?q=...` (no slash) returns a 404 page.
    pub async fn fetch_search(&self, keyword: &str, page: u32) -> Result<Vec<AhentaiGalleryItem>> {
        let kw = keyword.trim();
        let url = if page <= 1 {
            format!("{}/search/?q={}", AHENTAI_BASE, urlencoding(kw))
        } else {
            format!(
                "{}/search/?q={}&page={}",
                AHENTAI_BASE,
                urlencoding(kw),
                page
            )
        };
        self.fetch_listing(&url).await
    }

    /// Shared listing parser for both homepage and search results.
    /// Both use the same `.preview_item` card structure inside `.ov_item`.
    /// Page count and uploader are always 0 / None here — listing cards don't
    /// carry that information.  They are populated later during download via
    /// `fetch_gallery_meta`.
    async fn fetch_listing(&self, url: &str) -> Result<Vec<AhentaiGalleryItem>> {
        let html = self
            .get(url)
            .send()
            .await
            .with_context(|| format!("request {}", url))?
            .text()
            .await
            .context("read listing body")?;
        let doc = Html::parse_document(&html);

        let item_sel =
            Selector::parse("div.preview_item").map_err(|e| anyhow::anyhow!("item selector: {e:?}"))?;
        let link_sel = Selector::parse(r#"a[href^="/g/"]"#)
            .map_err(|e| anyhow::anyhow!("link selector: {e:?}"))?;
        let caption_sel =
            Selector::parse("h2.caption").map_err(|e| anyhow::anyhow!("caption selector: {e:?}"))?;
        let thumb_sel =
            Selector::parse("img.lazy").map_err(|e| anyhow::anyhow!("thumb selector: {e:?}"))?;
        let cat_sel = Selector::parse(".cl h3 a")
            .map_err(|e| anyhow::anyhow!("category selector: {e:?}"))?;

        let mut items: Vec<AhentaiGalleryItem> = Vec::new();
        for card in doc.select(&item_sel) {
            // Gallery ID from the `/g/{id}/` link.
            let id = card
                .select(&link_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .and_then(|href| {
                    let trimmed = href.trim_start_matches('/').trim_end_matches('/');
                    trimmed.strip_prefix("g/").map(|s| s.to_string())
                })
                .unwrap_or_default();
            if id.is_empty() {
                continue;
            }

            let title = card
                .select(&caption_sel)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            // Category from `.cl h3 a`.
            let category = card
                .select(&cat_sel)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            // Thumbnails are lazy-loaded: the real URL is in `data-src`, not
            // `src` (which is a placeholder). Protocol-relative URLs
            // (`//images.asmhentai.com/...`) get `https:` prepended.
            let thumb_url = card
                .select(&thumb_sel)
                .next()
                .and_then(|img| {
                    img.value()
                        .attr("data-src")
                        .or_else(|| img.value().attr("src"))
                })
                .map(|raw| {
                    if raw.starts_with("//") {
                        format!("https:{}", raw)
                    } else if raw.starts_with('/') {
                        format!("https://asmhentai.com{}", raw)
                    } else {
                        raw.to_string()
                    }
                })
                .unwrap_or_default();

            items.push(AhentaiGalleryItem {
                id,
                title,
                thumb_url,
                page_count: 0,
                uploader: None,
                category,
            });
        }
        Ok(items)
    }

    /// Proxy-download a thumbnail image. Adds a Referer so the CDN serves the
    /// image (the site hotlinks images from `images.asmhentai.com`).
    pub async fn proxy_thumb(&self, url: &str) -> Result<Vec<u8>> {
        let bytes = self
            .http
            .get(url)
            .header("Referer", &format!("{}/", AHENTAI_BASE))
            .send()
            .await
            .context("proxy thumb")?
            .bytes()
            .await
            .context("read thumb bytes")?
            .to_vec();
        if bytes.len() < 200 {
            anyhow::bail!("suspiciously small image from {url}");
        }
        Ok(bytes)
    }

    /// Scrape the gallery detail page (`/g/{id}/`) for metadata: title, page
    /// count, tags, artists, CDN load_dir, and more.
    pub async fn fetch_gallery_meta(&self, gallery_id: &str) -> Result<AhentaiGalleryMeta> {
        let url = format!("{}/g/{}/", AHENTAI_BASE, gallery_id);
        let html = self
            .get(&url)
            .send()
            .await
            .with_context(|| format!("request gallery meta {url}"))?
            .text()
            .await
            .context("read gallery meta body")?;
        let doc = Html::parse_document(&html);

        // Title: `.book_page .right .info h1`
        let title = doc
            .select(&Selector::parse(".book_page .right .info h1").map_err(|e| anyhow::anyhow!("{e:?}"))?)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        // Page count: `.pages h3` → "Pages: 35"
        let page_count = doc
            .select(&Selector::parse(".pages h3").map_err(|e| anyhow::anyhow!("{e:?}"))?)
            .next()
            .map(|n| n.text().collect::<String>())
            .and_then(|t| {
                t.split("Pages:")
                    .nth(1)
                    .and_then(|s| s.trim().parse::<i32>().ok())
            })
            .unwrap_or(0);

        // Load dir from hidden input: `input#load_dir`
        let load_dir = doc
            .select(
                &Selector::parse("input#load_dir").map_err(|e| anyhow::anyhow!("{e:?}"))?,
            )
            .next()
            .and_then(|n| n.value().attr("value"))
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Tags: h3 containing "Tags" + adjacent .tag_list a span.tag
        let tags = extract_tag_section(&doc, "Tags");
        let artists = extract_tag_section(&doc, "Artists");
        let groups = extract_tag_section(&doc, "Groups");
        let languages = extract_tag_section(&doc, "Languages");
        let category = extract_tag_section(&doc, "Category")
            .first()
            .cloned()
            .unwrap_or_default();
        let parodies = extract_tag_section(&doc, "Parodies");

        Ok(AhentaiGalleryMeta {
            title,
            page_count,
            load_dir,
            tags,
            artists,
            groups,
            languages,
            category,
            parodies,
        })
    }

}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip the trailing gallery-count suffix from tag/artist/etc. names.
/// asmhentai nests `<span class="gallery_count">(1,234)</span>` inside every
/// `<span class="badge tag">`, so `text()` returns e.g. `"lolicon (107,462)"`.
fn strip_tag_count(raw: &str) -> String {
    let s = raw.trim();
    // Match " (digits)" or " (digits,digits)" at the very end.
    if let Some(open) = s.rfind(" (") {
        let before = &s[..open];
        let inside = &s[open + 2..];
        if inside.ends_with(')') {
            let count_part = &inside[..inside.len() - 1];
            if !count_part.is_empty()
                && count_part.chars().all(|c| c.is_ascii_digit() || c == ',')
            {
                return before.to_string();
            }
        }
    }
    s.to_string()
}

/// Extract tags from a section like `<h3>Tags:</h3><div class="tag_list"><a>...`
fn extract_tag_section(doc: &Html, heading: &str) -> Vec<String> {
    // Find all `.tags` blocks, then the one whose h3 text starts with `heading`.
    let tags_sel = match Selector::parse(".tags") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let h3_sel = match Selector::parse("h3") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let tag_sel = match Selector::parse("a span.tag") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    for block in doc.select(&tags_sel) {
        let h3_text = block
            .select(&h3_sel)
            .next()
            .map(|n| n.text().collect::<String>())
            .unwrap_or_default();
        if h3_text.trim_start().starts_with(heading) {
            return block
                .select(&tag_sel)
                .map(|span| strip_tag_count(&span.text().collect::<String>()))
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    Vec::new()
}

/// Percent-encode a search keyword for the query string.
/// Spaces become `+`; reserved characters are percent-encoded.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b' ' => out.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}
