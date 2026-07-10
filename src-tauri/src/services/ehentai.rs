use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;
use scraper::{Html, Selector};
use uuid::Uuid;

use crate::db::Database;
use crate::models::{BookMetadata, BookSource};
use crate::services::pixiv::{PixivProgress, PixivProgressSink};
use crate::services::storage::StorageService;
use crate::services::LibraryService;

const EHENTAI_BASE: &str = "https://e-hentai.org";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Authenticated e-hentai client. Holds the session cookies captured from the
/// in-app browser.
pub struct EhentaiClient {
    http: Client,
    cookie_str: String,
}

impl EhentaiClient {
    pub fn new(cookie_str: &str) -> Result<Self> {
        let http = Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(30))
            // e-hentai sets many cookies (ipb_member_id / ipb_pass_hash /
            // ipb_session_id + igneous); follow redirects normally.
            .redirect(reqwest::redirect::Policy::default())
            .build()
            .context("build reqwest client")?;
        Ok(Self {
            http,
            cookie_str: cookie_str.trim().to_string(),
        })
    }


    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.http
            .get(url)
            .header("Cookie", &self.cookie_str)
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Referer", &format!("{}/", EHENTAI_BASE))
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
        let url = format!("{}/g/{}/{}/", EHENTAI_BASE, gid, token);
        let html = self
            .get(&url)
            .send()
            .await
            .context("request gallery page")?
            .text()
            .await
            .context("read gallery page body")?;
        let doc = Html::parse_document(&html);

        // Thumbnail grid: each page link lives inside a <div class="gdtm">/<div
        // class="gdtl"> with an <a href=".../s/...">.
        let sel = Selector::parse("div.gdtm a, div.gdtl a").map_err(|e| {
            anyhow::anyhow!("build selector: {e:?}")
        })?;
        let page_sel = Selector::parse("a[href]").map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let mut pages: Vec<String> = Vec::new();
        for a in doc.select(&sel).flat_map(|d| d.select(&page_sel)) {
            if let Some(href) = a.value().attr("href") {
                if href.contains("/s/") {
                    pages.push(href.to_string());
                }
            }
        }
        if pages.is_empty() {
            anyhow::bail!("no page links found in gallery {url} (not logged in or deleted?)");
        }
        Ok(pages)
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

    /// Download an image honoring e-hentai's Referer requirement.
    pub async fn download_image(&self, url: &str) -> Result<Vec<u8>> {
        let bytes = self
            .http
            .get(url)
            .header("Referer", &format!("{}/", EHENTAI_BASE))
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

/// A gallery download carried out one page at a time, emitting progress through
/// a [PixivProgressSink] (reused verbatim — the frontends already render it).
pub struct EhentaiDownloader {
    client: EhentaiClient,
}

impl EhentaiDownloader {
    pub fn new(cookie_str: &str) -> Result<Self> {
        Ok(Self {
            client: EhentaiClient::new(cookie_str)?,
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

        library
            .register_stored_book(
                &book_id,
                title,
                &file_path,
                images.len() as i32,
                Some(&BookSource {
                    plugin: "ehentai".into(),
                    source_url: format!("{}/g/{}/{}/", EHENTAI_BASE, gid, token),
                    scraped_at: Some(chrono::Utc::now()),
                }),
                tags,
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
