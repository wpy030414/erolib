use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;
use uuid::Uuid;

use crate::db::Database;
use crate::models::{BookMetadata, BookSource};
use crate::services::pixiv::{PixivProgress, PixivProgressSink};
use crate::services::storage::StorageService;
use crate::services::LibraryService;

const EHENTAI_BASE: &str = "https://e-hentai.org";

/// Gallery metadata scraped from the landing page (best-effort).
#[derive(Debug, Default, Clone)]
pub struct GalleryMeta {
    /// Gallery title from <h1 id="gn">.
    pub title: String,
    /// Site-local posted time, e.g. "2024-01-15 12:00".
    pub posted: Option<String>,
    /// Uploader display name (EHentai has no uploader id).
    pub uploader: Option<String>,
    /// Tag names (namespace stripped from "namespace:tag").
    pub tags: Vec<String>,
}
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// One row of an e-hentai/exhentai search result page. Serialized camelCase
/// for the frontend browse grid.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryListItem {
    pub gid: String,
    pub token: String,
    pub title: String,
    /// Real thumbnail URL (ehgt.org webp). Use the `data-src` attribute, not
    /// `src` (which is a 1x1 placeholder gif).
    pub thumb_url: String,
    pub page_count: i32,
    /// Category display name, e.g. "Doujinshi", "Manga".
    pub category: String,
    /// Uploader display name (e-hentai galleries have no author field).
    pub uploader: Option<String>,
}

/// Authenticated e-hentai client. Holds the session cookies captured from the
/// in-app browser.
pub struct EhentaiClient {
    http: Client,
    cookie_str: String,
    /// Origin used for every request: `https://e-hentai.org` (ex=false) or
    /// `https://exhentai.org` (ex=true). Gallery details and search share it.
    base: String,
}

impl EhentaiClient {
    /// Build a client targeting either the public e-hentai.org site or the
    /// exhentai.org sister site (requires an `igneous` cookie). The base is
    /// reused for search, gallery pages, and per-page image downloads.
    pub fn new(cookie_str: &str, ex: bool) -> Result<Self> {
        let http = Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(30))
            // e-hentai sets many cookies (ipb_member_id / ipb_pass_hash /
            // ipb_session_id + igneous); follow redirects normally.
            .redirect(reqwest::redirect::Policy::default())
            .build()
            .context("build reqwest client")?;
        let base = if ex {
            "https://exhentai.org".to_string()
        } else {
            EHENTAI_BASE.to_string()
        };
        Ok(Self {
            http,
            cookie_str: cookie_str.trim().to_string(),
            base,
        })
    }


    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.http
            .get(url)
            .header("Cookie", &self.cookie_str)
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Referer", &format!("{}/", self.base))
    }

    /// Parse a gallery URL into (gid, token), or bail if malformed.
    pub fn parse_gallery_url(url: &str) -> Result<(String, String)> {
        let parsed = reqwest::Url::parse(url).context("parse gallery url")?;
        let segs: Vec<&str> = parsed.path_segments().map(|s| s.collect()).unwrap_or_default();
        // /g/<gid>/<token>/
        if segs.len() >= 3 && segs[0] == "g" {
            return Ok((segs[1].to_string(), segs[2].to_string()));
        }
        anyhow::bail!("not an e-hentai gallery url: {url}")
    }

    /// Extract every page-view link from a gallery landing page. Each is of the
    /// form https://e-hentai.org/s/<page_token>/<gid>-<page>.
    pub async fn fetch_gallery_pages(&self, gid: &str, token: &str) -> Result<Vec<String>> {
        // A gallery shows ~20 thumbs per page (?p=0,1,2…); paginate and
        // accumulate all /s/ links until a page contributes none.
        let sel = Selector::parse(r#"a[href*="/s/"]"#)
            .map_err(|e| anyhow::anyhow!("build selector: {e:?}"))?;
        let mut seen = std::collections::HashSet::new();
        let mut pages: Vec<String> = Vec::new();
        let mut p = 0u32;
        loop {
            let url = format!("{}/g/{}/{}/?p={}", self.base, gid, token, p);
            let html = self
                .get(&url)
                .send()
                .await
                .with_context(|| format!("request gallery page {url}"))?
                .text()
                .await
                .context("read gallery page body")?;
            let doc = Html::parse_document(&html);
            let mut found = 0u32;
            for a in doc.select(&sel) {
                if let Some(href) = a.value().attr("href") {
                    let href = href.to_string();
                    if href.contains("/s/") && seen.insert(href.clone()) {
                        pages.push(href);
                        found += 1;
                    }
                }
            }
            if found == 0 {
                break;
            }
            p += 1;
            if p > 5000 {
                break; // sanity cap
            }
        }
        if pages.is_empty() {
            anyhow::bail!(
                "no page links found in gallery {}/g/{}/{} (not logged in or deleted?)",
                self.base,
                gid,
                token
            );
        }
        Ok(pages)
    }

    /// Scrape the posted time + uploader from a gallery landing page.
    /// Best-effort; missing fields are left None.
    pub async fn fetch_gallery_meta(&self, gid: &str, token: &str) -> Result<GalleryMeta> {
        let url = format!("{}/g/{}/{}/", self.base, gid, token);
        let html = self
            .get(&url)
            .send()
            .await
            .context("request gallery meta")?
            .text()
            .await
            .context("read gallery meta body")?;
        let doc = Html::parse_document(&html);

        let uploader_sel =
            Selector::parse("#gdn a").map_err(|e| anyhow::anyhow!("build selector: {e:?}"))?;
        let uploader = doc
            .select(&uploader_sel)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        // #gdd's first td.gdt2 is the "Posted:" value, a site-local string like
        // "2024-01-15 12:00"; the frontend tolerates this partial format.
        let posted_sel =
            Selector::parse("#gdd td.gdt2").map_err(|e| anyhow::anyhow!("build selector: {e:?}"))?;
        let posted = doc
            .select(&posted_sel)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        // Title: <h1 id="gn">.
        let title_sel = Selector::parse("#gn").map_err(|e| anyhow::anyhow!("build selector: {e:?}"))?;
        let title = doc
            .select(&title_sel)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_default();

        // Tags: a[href*="/tag/"] → href ".../tag/namespace:tag" (the <a> has no
        // title attr; the namespace:tag lives in the URL / the id "ta_…"). Keep
        // the tag part after the colon.
        let tag_sel = Selector::parse(r#"a[href*="/tag/"]"#)
            .map_err(|e| anyhow::anyhow!("build selector: {e:?}"))?;
        let mut tags: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for a in doc.select(&tag_sel) {
            if let Some(href) = a.value().attr("href") {
                let rest = href.rsplit("/tag/").next().unwrap_or("");
                let tag = rest.rsplit(':').next().unwrap_or(rest).trim().to_string();
                if !tag.is_empty() && seen.insert(tag.clone()) {
                    tags.push(tag);
                }
            }
        }

        Ok(GalleryMeta { title, posted, uploader, tags })
    }

    /// Resolve the direct image URL for a single page view (<img id="img">).
    pub async fn fetch_page_image(&self, page_url: &str) -> Result<String> {
        let html = self
            .get(page_url)
            .send()
            .await
            .context("request page view")?
            .text()
            .await
            .context("read page view body")?;
        let doc = Html::parse_document(&html);
        let img = Selector::parse("img#img").map_err(|e| anyhow::anyhow!("{e:?}"))?;
        doc.select(&img)
            .next()
            .and_then(|n| n.value().attr("src"))
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no #img on page {page_url}"))
    }

    /// Query the search page and return one page of gallery results (25/page).
    /// `keyword` of None or empty string omits `f_search`; `category` is the
    /// e-hentai **path segment** (e.g. "doujinshi", "manga") — None = all;
    /// `next` is the pagination cursor (None = first page; pass the last
    /// gallery's gid of the previous page). e-hentai's `?page=N` does NOT
    /// paginate — `?next={gid}` does; `f_cats` also doesn't filter, hence the
    /// per-category path. Parsing is best-effort: rows missing a parseable
    /// gallery link are skipped.
    pub async fn fetch_search(
        &self,
        keyword: Option<&str>,
        category: Option<&str>,
        next: Option<&str>,
    ) -> Result<Vec<GalleryListItem>> {
        let base_path = match category {
            Some(c) => {
                let c = c.trim().trim_start_matches('/').trim_end_matches('/');
                if c.is_empty() {
                    format!("{}/", self.base)
                } else {
                    format!("{}/{}/", self.base, c)
                }
            }
            None => format!("{}/", self.base),
        };
        let mut url = reqwest::Url::parse(&base_path).context("parse search base url")?;
        {
            let mut q = url.query_pairs_mut();
            if let Some(kw) = keyword {
                let kw = kw.trim();
                if !kw.is_empty() {
                    q.append_pair("f_search", kw);
                }
            }
            if let Some(n) = next {
                let n = n.trim();
                if !n.is_empty() {
                    q.append_pair("next", n);
                }
            }
        }

        let html = self
            .get(url.as_str())
            .send()
            .await
            .context("request search page")?
            .text()
            .await
            .context("read search page body")?;
        let doc = Html::parse_document(&html);

        let tr_sel = Selector::parse("tr").map_err(|e| anyhow::anyhow!("tr selector: {e:?}"))?;
        let link_sel = Selector::parse(r#"a[href*="/g/"]"#)
            .map_err(|e| anyhow::anyhow!("gallery link selector: {e:?}"))?;
        let glink_sel =
            Selector::parse(".glink").map_err(|e| anyhow::anyhow!("glink selector: {e:?}"))?;
        let thumb_sel = Selector::parse(".glthumb img")
            .map_err(|e| anyhow::anyhow!("thumb selector: {e:?}"))?;
        let cat_sel = Selector::parse(".cn").map_err(|e| anyhow::anyhow!("cn selector: {e:?}"))?;
        let uploader_sel = Selector::parse(".gl4c a")
            .map_err(|e| anyhow::anyhow!("uploader selector: {e:?}"))?;
        let gl4c_sel = Selector::parse(".gl4c").map_err(|e| anyhow::anyhow!("gl4c selector: {e:?}"))?;
        let div_sel = Selector::parse("div").map_err(|e| anyhow::anyhow!("div selector: {e:?}"))?;

        let mut items: Vec<GalleryListItem> = Vec::new();
        for tr in doc.select(&tr_sel) {
            // gid/token: first parseable /g/<gid>/<token>/ link in the row.
            let (gid, token) = match tr
                .select(&link_sel)
                .find_map(|a| a.value().attr("href").and_then(Self::parse_gallery_url_ok))
            {
                Some(gt) => gt,
                None => continue,
            };

            let title = tr
                .select(&glink_sel)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            // Thumbnail: the real webp lives in data-src; src is a 1x1 gif.
            let thumb_url = tr
                .select(&thumb_sel)
                .next()
                .and_then(|n| {
                    n.value()
                        .attr("data-src")
                        .or_else(|| n.value().attr("src"))
                        .map(str::to_string)
                })
                .unwrap_or_default();

            // Page count from gl4c's "N pages" div ONLY. The whole gl4c text
            // concatenates the uploader div directly with the pages div (no
            // separator), so an uploader ending in digits (e.g. "JustSharing123")
            // would read as "…12374 pages" and inflate the count.
            let page_count = tr
                .select(&gl4c_sel)
                .next()
                .and_then(|gl4c| {
                    gl4c.select(&div_sel).find_map(|d| {
                        let t = d.text().collect::<String>();
                        if t.contains("pages") {
                            Some(extract_page_count(&t))
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or(0);

            let category = tr
                .select(&cat_sel)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let uploader = tr
                .select(&uploader_sel)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty());

            items.push(GalleryListItem {
                gid,
                token,
                title,
                thumb_url,
                page_count,
                category,
                uploader,
            });
        }
        Ok(items)
    }

    /// Wrap [parse_gallery_url] as an infallible filter for `find_map`.
    fn parse_gallery_url_ok(href: &str) -> Option<(String, String)> {
        Self::parse_gallery_url(href).ok()
    }

    /// Download an image honoring e-hentai's Referer requirement.
    pub async fn download_image(&self, url: &str) -> Result<Vec<u8>> {
        let bytes = self
            .http
            .get(url)
            .header("Referer", &format!("{}/", self.base))
            .send()
            .await
            .context("download image")?
            .bytes()
            .await
            .context("read image bytes")?
            .to_vec();
        if bytes.len() < 200 {
            anyhow::bail!("suspiciously small image from {url}");
        }
        Ok(bytes)
    }
}

/// Extract the page count from a gl4c cell's "N pages". Matches the plural
/// "pages" (not the bare substring "page", which appears inside some uploader
/// handles like "pageboy") and returns the first match.
fn extract_page_count(text: &str) -> i32 {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for (idx, _) in lower.match_indices(" pages") {
        let mut end = idx;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && bytes[start - 1].is_ascii_digit() {
            start -= 1;
        }
        if start < end {
            if let Ok(n) = lower[start..end].parse::<i32>() {
                return n;
            }
        }
    }
    0
}

/// A gallery download carried out one page at a time, emitting progress through
/// a [PixivProgressSink] (reused verbatim — the frontends already render it).
pub struct EhentaiDownloader {
    client: EhentaiClient,
}

impl EhentaiDownloader {
    pub fn new(cookie_str: &str) -> Result<Self> {
        Ok(Self {
            client: EhentaiClient::new(cookie_str, false)?,
        })
    }

    /// Download a gallery's images into bytes. `page_urls` is the ordered list
    /// from [EhentaiClient::fetch_gallery_pages]. Smart-skip behaviour (replace
    /// on update) is left to callers/gallery-id source tracking as in Pixiv.
    pub async fn download_gallery(
        &self,
        gallery_url: &str,
        title: &str,
        tags: &[String],
        page_urls: Vec<String>,
        sink: Arc<std::sync::Mutex<dyn PixivProgressSink>>,
        cancelled: &AtomicBool,
        library: &LibraryService,
        _db: Arc<Database>,
        storage: Arc<StorageService>,
    ) -> Result<()> {
        let (gid, token) = EhentaiClient::parse_gallery_url(gallery_url)?;
        sink.lock().unwrap().emit(PixivProgress::Phase {
            phase: "eh-fetched".into(),
            message: format!("{} pages for {}", page_urls.len(), title),
        });

        let mut images: Vec<Vec<u8>> = Vec::new();
        for (i, page_url) in page_urls.iter().enumerate() {
            if cancelled.load(Ordering::Relaxed) {
                return Err(anyhow::anyhow!("cancelled"));
            }
            sink.lock().unwrap().emit(PixivProgress::WorkStart {
                index: i as u64,
                total: page_urls.len() as u64,
                illust_id: format!("p{i}"),
                title: format!("Page {}", i + 1),
            });
            // Resolve the image URL + download, with a gentle throttle.
            match self.client.fetch_page_image(page_url).await {
                Ok(img_url) => match self.client.download_image(&img_url).await {
                    Ok(bytes) => {
                        images.push(bytes);
                        sink.lock().unwrap().emit(PixivProgress::WorkDone {
                            illust_id: format!("p{i}"),
                            title: format!("Page {}", i + 1),
                            pages: 1,
                        });
                    }
                    Err(e) => {
                        sink.lock().unwrap().emit(PixivProgress::WorkFailed {
                            illust_id: format!("p{i}"),
                            title: format!("Page {}", i + 1),
                            error: e.to_string(),
                        });
                    }
                },
                Err(e) => {
                    sink.lock().unwrap().emit(PixivProgress::WorkFailed {
                        illust_id: format!("p{i}"),
                        title: format!("Page {}", i + 1),
                        error: e.to_string(),
                    });
                }
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }

        if images.is_empty() {
            anyhow::bail!("no images downloaded from {gallery_url}");
        }

        sink.lock().unwrap().emit(PixivProgress::Phase {
            phase: "eh-building".into(),
            message: "building CB7".into(),
        });
        let book_id = Uuid::new_v4().to_string();
        let file_path = storage.create_cb7(
            &images,
            &BookMetadata {
                title: title.to_string(),
                tags: tags.to_owned(),
                ..Default::default()
            },
        )?;

        let source_url = format!("{}/g/{}/{}/", self.client.base, gid, token);
        let is_ex = self.client.base.contains("exhentai");
        let source = BookSource {
            plugin: (if is_ex { "exhentai" } else { "e-hentai" }).into(),
            source_url: source_url.clone(),
            scraped_at: Some(chrono::Utc::now()),
            ..Default::default()
        };

        library
            .register_stored_book(
                &book_id,
                title,
                &file_path,
                images.len() as i32,
                Some(&source),
                tags,
                None,
            )
            .await?;

        sink.lock().unwrap().emit(PixivProgress::WorkDone {
            illust_id: book_id,
            title: title.into(),
            pages: images.len() as u32,
        });
        Ok(())
    }
}
