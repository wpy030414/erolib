use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Database;
use crate::models::BookSource;
use crate::services::storage::StorageService;

type DownloadResult = std::result::Result<DownloadedArtwork, SkipReason>;

const PIXIV_BASE: &str = "https://www.pixiv.net";
const PIXIV_AJAX: &str = "https://www.pixiv.net/ajax";

/// Per-artwork progress event forwarded to the frontend through Tauri.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(crate = "serde", tag = "type", rename_all = "camelCase")]
pub enum PixivProgress {
    Phase {
        phase: String,
        message: String,
    },
    Fetched {
        total_bookmarks: u64,
    },
    WorkStart {
        index: u64,
        total: u64,
        illust_id: String,
        title: String,
    },
    WorkDone {
        illust_id: String,
        title: String,
        pages: u32,
    },
    WorkSkipped {
        illust_id: String,
        title: String,
        reason: String,
    },
    WorkFailed {
        illust_id: String,
        title: String,
        error: String,
    },
    Finished {
        downloaded: u64,
        skipped: u64,
        failed: u64,
    },
}

/// Callbacks that the downloader uses to emit progress. Kept trait-object so we
/// can inject the Tauri emitter in production and a no-op / logging sink in tests.
pub trait PixivProgressSink: Send + Sync {
    fn emit(&self, event: PixivProgress);
}

/// A single downloaded artwork result.
#[derive(Debug)]
struct DownloadedArtwork {
    pages: u32,
}

// --- Pixiv JSON models ---

#[derive(Debug, Deserialize)]
struct UserBookmarksResp {
    body: BookmarkBody,
}
#[derive(Debug, Deserialize)]
struct BookmarkBody {
    works: Vec<BookmarkWork>,
    #[serde(default)]
    total: u64,
}
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct BookmarkWork {
    id: String,
    title: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(rename = "illustType")]
    illust_type: Option<i32>,
    #[serde(default, rename = "userId")]
    user_id: Option<String>,
    #[serde(default, rename = "userName")]
    user_name: Option<String>,
    #[serde(default, rename = "createDate")]
    create_date: Option<String>,
    #[serde(default, rename = "url")]
    cover_url: Option<String>,
    #[serde(default, rename = "pageCount")]
    page_count: i32,
}

impl From<BookmarkWork> for UserWork {
    fn from(w: BookmarkWork) -> Self {
        Self {
            id: w.id,
            title: w.title,
            tags: w.tags,
            page_count: w.page_count,
            illust_type: w.illust_type,
            author: w.user_name,
            author_id: w.user_id,
            published_at: w.create_date,
            cover_url: w.cover_url,
        }
    }
}

impl From<UserWork> for BookmarkWork {
    fn from(w: UserWork) -> Self {
        Self {
            id: w.id,
            title: w.title,
            tags: w.tags,
            illust_type: w.illust_type,
            user_id: w.author_id,
            user_name: w.author,
            create_date: w.published_at,
            cover_url: w.cover_url,
            page_count: w.page_count,
        }
    }
}

#[derive(Debug, Deserialize)]
struct IllustPagesResp {
    body: Vec<IllustPageEntry>,
}
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct IllustPageEntry {
    pub(crate) urls: IllustUrls,
}
#[derive(Debug, Deserialize, Clone, Default)]
pub(crate) struct IllustUrls {
    #[serde(default)]
    pub(crate) original: String,
    #[serde(default)]
    pub(crate) regular: String,
}

/// Normalized artwork entry shared by the bookmark, user-works and following
/// paths. Serialized to the frontend (camelCase) for the browse grid.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserWork {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub page_count: i32,
    pub illust_type: Option<i32>,
    pub author: Option<String>,
    pub author_id: Option<String>,
    pub published_at: Option<String>,
    pub cover_url: Option<String>,
}

// --- Following feed (关注 tab): /ajax/follow_latest/illust ---

#[derive(Debug, Deserialize)]
struct FollowLatestResp {
    body: FollowLatestBody,
}

#[derive(Debug, Deserialize, Default)]
struct FollowLatestBody {
    #[serde(default)]
    thumbnails: FollowThumbs,
}

#[derive(Debug, Deserialize, Default)]
struct FollowThumbs {
    #[serde(default)]
    illust: Vec<FollowIllust>,
}

#[derive(Debug, Deserialize)]
struct FollowIllust {
    id: String,
    title: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, rename = "url")]
    cover_url: Option<String>,
    #[serde(default, rename = "userId")]
    user_id: Option<String>,
    #[serde(default, rename = "userName")]
    user_name: Option<String>,
    #[serde(default, rename = "pageCount")]
    page_count: i32,
    #[serde(default, rename = "illustType")]
    illust_type: Option<i32>,
    #[serde(default, rename = "createDate")]
    create_date: Option<String>,
}

impl From<FollowIllust> for UserWork {
    fn from(f: FollowIllust) -> Self {
        Self {
            id: f.id,
            title: f.title,
            tags: f.tags,
            page_count: f.page_count,
            illust_type: f.illust_type,
            author: f.user_name,
            author_id: f.user_id,
            published_at: f.create_date,
            cover_url: f.cover_url,
        }
    }
}


#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct FollowingUserResp {
    pub user_id: String,
    pub user_name: String,
    #[serde(default)]
    pub profile_image_url: String,
}

#[derive(Debug, Deserialize)]
struct FollowingListResp {
    body: FollowingBody,
}

#[derive(Debug, Deserialize)]
struct FollowingBody {
    pub users: Vec<FollowingUserResp>,
    #[serde(default)]
    pub total: u64,
}

#[derive(Debug, Deserialize)]
struct IllustDetailResp {
    body: IllustDetail,
}

#[derive(Debug, Deserialize)]
struct IllustDetail {
    id: String,
    title: String,
    #[serde(default)]
    tags: IllustTags,
    #[serde(default, rename = "pageCount")]
    page_count: i32,
    #[serde(rename = "illustType")]
    illust_type: Option<i32>,
    #[serde(default, rename = "userId")]
    user_id: Option<String>,
    #[serde(default, rename = "userName")]
    user_name: Option<String>,
    #[serde(default, rename = "createDate")]
    create_date: Option<String>,
    /// Display thumbnails (regular/small/original/…). Used as the cover for
    /// ugoira works, whose cb7 holds frame jpgs rather than a nice still cover.
    #[serde(default)]
    urls: IllustUrls,
}

#[derive(Debug, Deserialize, Default)]
struct IllustTags {
    #[serde(default)]
    tags: Vec<IllustTag>,
}

#[derive(Debug, Deserialize)]
struct IllustTag {
    #[serde(default)]
    tag: String,
}

// --- Ugoira (動画作, illustType==2): /ajax/illust/{id}/ugoira_meta ---
// The zip at `original_src` holds one jpg per frame (000000.jpg, 000001.jpg, …);
// `frames[].delay` is the per-frame hold time in milliseconds.
#[derive(Debug, Deserialize)]
struct UgoiraMetaResp {
    body: UgoiraMeta,
}

#[derive(Debug, Deserialize)]
pub struct UgoiraMeta {
    #[serde(rename = "originalSrc")]
    pub original_src: String,
    pub frames: Vec<UgoiraFrame>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UgoiraFrame {
    pub file: String,
    /// Hold time until the next frame, in milliseconds.
    pub delay: u32,
}

#[derive(Debug, Deserialize)]
struct UserAllWorksResp {
    body: UserAllWorksBody,
}

#[derive(Debug, Deserialize, Default)]
struct UserAllWorksBody {
    #[serde(default)]
    illusts: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    manga: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorResp {
    error: Option<bool>,
    message: Option<String>,
}

/// Authenticated Pixiv client.
pub struct PixivClient {
    http: Client,
    cookie_str: String,
}

impl PixivClient {
    pub fn new(cookie_str: &str) -> Result<Self> {
        let http = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
            // Without a total timeout a stalled Pixiv connection (rate-limit,
            // half-open TLS, hung socket) blocks the whole download forever with
            // zero network activity. Cap each request at 30s so it fails loudly.
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("build reqwest client")?;
        Ok(Self {
            http,
            cookie_str: cookie_str.trim().to_string(),
        })
    }

    fn check_json_error(text: &str) -> Result<()> {
        if let Ok(err) = serde_json::from_str::<ApiErrorResp>(text) {
            if err.error == Some(true) {
                return Err(anyhow::anyhow!(
                    "Pixiv API error: {}",
                    err.message.as_deref().unwrap_or("unknown")
                ));
            }
        }
        Ok(())
    }

    /// Fetch all of a user's public bookmarks (paginated). The limit param caps
    /// the number of works fetched (0 = all).
    pub async fn fetch_all_bookmarks(
        &self,
        user_id: &str,
        limit: u64,
        cancelled: &AtomicBool,
    ) -> Result<Vec<BookmarkWork>> {
        let mut works = Vec::new();
        let mut seen = HashSet::new();
        let mut offset: u64 = 0;
        let page_size: u64 = 48;
        let mut total: Option<u64> = None;

        loop {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }

            let url = format!(
                "{}/user/{}/illusts/bookmarks?tag=&offset={}&limit={}&rest=show",
                PIXIV_AJAX, user_id, offset, page_size
            );
            let body_str = self
                .http
                .get(&url)
                .header("Cookie", &self.cookie_str)
                .header("Accept", "application/json")
                .header("Referer", &format!("{}/users/{}/bookmarks/artworks", PIXIV_BASE, user_id))
                .send()
                .await
                .context("request bookmark list")?
                .text()
                .await
                .context("read bookmark list body")?;

            Self::check_json_error(&body_str)?;
            let resp: UserBookmarksResp =
                serde_json::from_str(&body_str).context("parse bookmark list")?;

            if let Some(t) = total {
                // keep going
                let _ = t;
            } else {
                total = Some(resp.body.total);
            }

            let batch_len = resp.body.works.len() as u64;
            if batch_len == 0 {
                break;
            }

            for w in resp.body.works {
                if seen.insert(w.id.clone()) {
                    works.push(w);
                }
            }

            if let Some(t) = total {
                if works.len() as u64 >= t {
                    break;
                }
            }
            if limit > 0 && works.len() as u64 >= limit {
                works.truncate(limit as usize);
                break;
            }

            offset += batch_len;
            // Be gentle on Pixiv's servers.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        Ok(works)
    }

    /// Fetch a single page of a user's public bookmarks for browse-mode lazy
    /// loading. Returns `(works, total)` so the caller can decide whether more
    /// pages remain. Unlike `fetch_all_bookmarks` this makes exactly one
    /// request (no pagination loop, no dedup, no sleep).
    pub async fn fetch_bookmarks_page(
        &self,
        user_id: &str,
        offset: u64,
        limit: u64,
    ) -> Result<(Vec<BookmarkWork>, u64)> {
        let url = format!(
            "{}/user/{}/illusts/bookmarks?tag=&offset={}&limit={}&rest=show",
            PIXIV_AJAX, user_id, offset, limit
        );
        let body_str = self
            .http
            .get(&url)
            .header("Cookie", &self.cookie_str)
            .header("Accept", "application/json")
            .header(
                "Referer",
                &format!("{}/users/{}/bookmarks/artworks", PIXIV_BASE, user_id),
            )
            .send()
            .await
            .context("request bookmark page")?
            .text()
            .await
            .context("read bookmark page body")?;
        Self::check_json_error(&body_str)?;
        let resp: UserBookmarksResp =
            serde_json::from_str(&body_str).context("parse bookmark page")?;
        Ok((resp.body.works, resp.body.total))
    }

    /// Get image URLs for every page of a manga-type (or single-image) illust.
    pub async fn fetch_pages(&self, illust_id: &str) -> Result<Vec<IllustPageEntry>> {
        let url = format!("{}/illust/{}/pages", PIXIV_AJAX, illust_id);
        let body_str = self
            .http
            .get(&url)
            .header("Cookie", &self.cookie_str)
            .header("Accept", "application/json")
            .header("Referer", &format!("{}/artworks/{}", PIXIV_BASE, illust_id))
            .send()
            .await
            .context("request illust pages")?
            .text()
            .await
            .context("read illust pages body")?;
        Self::check_json_error(&body_str)?;
        let resp: IllustPagesResp =
            serde_json::from_str(&body_str).context("parse illust pages")?;
        Ok(resp.body)
    }

    /// Fetch the ugoira (動画作) frame manifest + original-resolution zip URL.
    /// Only valid for works with `illustType == 2`. The caller downloads the
    /// zip, extracts the per-frame jpgs, and records the per-frame delays.
    pub async fn fetch_ugoira_meta(&self, illust_id: &str) -> Result<UgoiraMeta> {
        let url = format!("{}/illust/{}/ugoira_meta", PIXIV_AJAX, illust_id);
        let body_str = self
            .get_json_ajax(&url, &format!("{}/artworks/{}", PIXIV_BASE, illust_id))
            .await?;
        let resp: UgoiraMetaResp =
            serde_json::from_str(&body_str).context("parse ugoira meta")?;
        Ok(resp.body)
    }

    /// Download an image honoring Pixiv's hotlink protection (Referer header).
    pub async fn download_image(&self, url: &str) -> Result<Vec<u8>> {
        let bytes = self
            .http
            .get(url)
            .header("Referer", &format!("{}/", PIXIV_BASE))
            .send()
            .await
            .context("download image")?
            .bytes()
            .await
            .context("read image bytes")?
            .to_vec();
        if bytes.len() < 100 {
            anyhow::bail!("suspiciously small image from {}", url);
        }
        Ok(bytes)
    }

    async fn get_json_ajax(&self, url: &str, referer: &str) -> Result<String> {
        let resp = self
            .http
            .get(url)
            .header("Cookie", &self.cookie_str)
            .header("Accept", "application/json")
            .header("Referer", referer)
            .send()
            .await
            .with_context(|| format!("request {url}"))?;
        let status = resp.status();
        let body_str = resp
            .text()
            .await
            .with_context(|| format!("read body {url}"))?;
        if !status.is_success() {
            let preview = body_str.chars().take(200).collect::<String>();
            return Err(anyhow::anyhow!(
                "Pixiv returned HTTP {status} for {url}: {preview}"
            ));
        }
        Self::check_json_error(&body_str)?;
        Ok(body_str)
    }

    /// Fetch the list of users the given account follows (paginated). Used to
    /// power the "关注 (following)" download tab. `limit` caps the number of
    /// followings returned (0 = all).
    pub async fn fetch_followings(
        &self,
        user_id: &str,
        limit: u64,
        cancelled: &AtomicBool,
    ) -> Result<Vec<FollowingUserResp>> {
        let mut users = Vec::new();
        let mut offset: u64 = 0;
        let page_size: u64 = 100;
        let referer = format!("{}/users/{}/following", PIXIV_BASE, user_id);
        let mut total: Option<u64> = None;

        loop {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            // Pixiv's endpoint returns an error (404) once offset runs past the
            // last available entry rather than an empty list, so bail out as soon
            // as we know we've paged through the advertised total.
            if let Some(t) = total {
                if offset >= t {
                    break;
                }
            }
            let url = format!(
                "{}/user/{}/following?offset={}&limit={}&rest=show",
                PIXIV_AJAX, user_id, offset, page_size
            );
            let body_str = match self.get_json_ajax(&url, &referer).await {
                Ok(s) => s,
                Err(e) => {
                    // An error here is almost always a pagination-past-end 404;
                    // surface whatever followings we already collected.
                    if users.is_empty() {
                        return Err(e);
                    }
                    break;
                }
            };
            let resp: FollowingListResp =
                match serde_json::from_str(&body_str) {
                    Ok(r) => r,
                    Err(e) => {
                        let preview = body_str.chars().take(200).collect::<String>();
                        return Err(anyhow::anyhow!(
                            "parse following list: {e}; body preview: {preview}"
                        ));
                    }
                };
            if resp.body.users.is_empty() {
                break;
            }
            if total.is_none() {
                total = Some(resp.body.total);
            }
            for u in &resp.body.users {
                if users.iter().all(|existing: &FollowingUserResp| existing.user_id != u.user_id) {
                    users.push(u.clone());
                }
                if limit > 0 && users.len() as u64 >= limit {
                    break;
                }
            }
            if limit > 0 && users.len() as u64 >= limit {
                break;
            }
            if let Some(t) = total {
                if users.len() as u64 >= t {
                    break;
                }
            }
            offset += resp.body.users.len() as u64;
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        Ok(users)
    }

    /// Fetch one page of the logged-in user's following feed (关注 tab) via the
    /// private `/ajax/follow_latest/illust` endpoint. There is no user id in the
    /// path — the session cookie identifies the user. `page` is 1-based; each
    /// page returns ~60 recent works from followed creators.
    pub async fn fetch_follow_latest(&self, page: u64) -> Result<Vec<UserWork>> {
        let url = format!("{}/follow_latest/illust?p={}&mode=all", PIXIV_AJAX, page);
        let body_str = self
            .http
            .get(&url)
            .header("Cookie", &self.cookie_str)
            .header("Accept", "application/json")
            .header("Referer", &format!("{}/", PIXIV_BASE))
            .send()
            .await
            .context("request follow_latest")?
            .text()
            .await
            .context("read follow_latest body")?;
        Self::check_json_error(&body_str)?;
        let resp: FollowLatestResp =
            serde_json::from_str(&body_str).context("parse follow_latest")?;
        Ok(resp
            .body
            .thumbnails
            .illust
            .into_iter()
            .map(UserWork::from)
            .collect())
    }

    /// Fetch the home recommendation feed (随便看看 tab) — the works Pixiv
    /// pushes on the logged-in homepage based on the user's taste. Uses the
    /// `/ajax/top/illust` landing endpoint (the same one PixivFE reads): it
    /// returns the whole landing batch in one shot, so `page` is ignored —
    /// browse mode renders the single batch. The response shape mirrors
    /// follow_latest (`body.thumbnails.illust`), so the parser is reused.
    pub async fn fetch_recommended(&self, _page: u64) -> Result<Vec<UserWork>> {
        let url = format!("{}/top/illust?mode=all", PIXIV_AJAX);
        let body_str = self
            .get_json_ajax(&url, &format!("{}/", PIXIV_BASE))
            .await
            .context("request top/illust")?;
        let resp: FollowLatestResp =
            serde_json::from_str(&body_str).context("parse top/illust")?;
        Ok(resp
            .body
            .thumbnails
            .illust
            .into_iter()
            .map(UserWork::from)
            .collect())
    }

    /// Search illustrations by keyword (搜索框). `page` is 1-based (~60 per
    /// page). Uses `/ajax/search/artworks/{keyword}`; the result list lives at
    /// `body.illustManga.data[]` (per the greasyfork/Pixiv-Infinite-Scroll
    /// production scraper — not `thumbnails.illust`). That array can include
    /// ad/placeholder entries with a null `id`, so parse defensively and skip
    /// them. Element fields match `FollowIllust`, which is reused.
    pub async fn fetch_search(&self, keyword: &str, page: u64) -> Result<Vec<UserWork>> {
        let encoded = urlencoding::encode(keyword);
        let url = format!(
            "{}/search/artworks/{}?word={}&mode=all&s_mode=s_tag&type=all&order=date_d&p={}",
            PIXIV_AJAX, encoded, encoded, page
        );
        let referer = format!("{}/search.php?s_mode=s_tag&type=all&word={}", PIXIV_BASE, encoded);
        let body_str = self
            .get_json_ajax(&url, &referer)
            .await
            .context("request search artworks")?;
        let value: serde_json::Value =
            serde_json::from_str(&body_str).context("parse search artworks")?;
        let data = value
            .get("body")
            .and_then(|b| b.get("illustManga"))
            .and_then(|im| im.get("data"))
            .and_then(|d| d.as_array());
        let mut works = Vec::new();
        if let Some(arr) = data {
            for entry in arr {
                // Skip ad/placeholder rows whose id is null/empty.
                let has_id = entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false);
                if has_id {
                    if let Ok(item) = serde_json::from_value::<FollowIllust>(entry.clone()) {
                        works.push(UserWork::from(item));
                    }
                }
            }
        }
        Ok(works)
    }

    /// Resolve the numeric user id that `cookie_str` belongs to. Pixiv's
    /// `/setting_user.php` returns a 302 whose `Location` is `/users/<id>/setting`
    /// for a logged-in session. With redirects disabled we can read that header
    /// and pull the id out of it — the embedded login window lands on the
    /// homepage (no `/users/<id>` in the URL), so we can't rely on URL parsing
    /// alone.
    /// Parse the user id out of a PHPSESSID cookie value. Pixiv's PHPSESSID is
    /// shaped `{user_id}_{secret}`, so the numeric segment before the underscore
    /// is the logged-in user id — no network request needed. This is the
    /// preferred path: `/setting_user.php` now redirects to `/settings/account`
    /// (no id in the URL), so the legacy redirect scrape no longer works.
    fn user_id_from_phpsessid(cookie_str: &str) -> Option<String> {
        for part in cookie_str.split(';') {
            let part = part.trim();
            let Some(rest) = part.strip_prefix("PHPSESSID=") else {
                continue;
            };
            let id = rest.split('_').next().unwrap_or("");
            if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                return Some(id.to_string());
            }
        }
        None
    }

    pub async fn fetch_current_user_id(cookie_str: &str) -> Result<String> {
        if let Some(id) = Self::user_id_from_phpsessid(cookie_str) {
            return Ok(id);
        }
        let no_redirect = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("build reqwest client")?;
        let resp = no_redirect
            .get(format!("{}/setting_user.php", PIXIV_BASE))
            .header("Cookie", cookie_str.trim())
            .send()
            .await
            .context("request setting_user.php")?;
        let location = resp
            .headers()
            .get("location")
            .or_else(|| resp.headers().get("Location"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if let Some(id) = user_id_from_setting_location(location) {
            Ok(id)
        } else {
            Err(anyhow::anyhow!(
                "could not determine user id from redirect: {location:?}"
            ))
        }
    }
    /// exposes the full id set via /profile/all with no timestamps, so we sort
    /// by id descending (ids are monotonic, so this approximates newest-first)
    /// and pull each work's detail for title/tags/page count. `limit` caps the
    /// number of works returned (0 = all).
    pub async fn fetch_user_works(
        &self,
        user_id: &str,
        limit: u64,
        cancelled: &AtomicBool,
    ) -> Result<Vec<UserWork>> {
        let list_url = format!("{}/user/{}/profile/all", PIXIV_AJAX, user_id);
        let body_str = self
            .get_json_ajax(&list_url, &format!("{}/users/{}/posts", PIXIV_BASE, user_id))
            .await?;
        let resp: UserAllWorksResp =
            serde_json::from_str(&body_str).context("parse user profile/all")?;

        let mut ids: Vec<String> = resp
            .body
            .illusts
            .keys()
            .chain(resp.body.manga.keys())
            .cloned()
            .filter(|id| !id.is_empty())
            .collect();
        ids.sort_by(|a, b| b.cmp(a)); // newest (highest) ids first
        ids.dedup();

        if limit > 0 {
            ids.truncate(limit as usize);
        }

        let mut works = Vec::new();
        for id in &ids {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            if let Some(work) = self.fetch_illust_detail(id).await? {
                works.push(work);
            }
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }

        Ok(works)
    }

    /// Fetch a single illustration's metadata (title, tags, page count, type)
    /// from the Pixiv private API. Returns None if the id fails to parse.
    pub async fn fetch_illust_detail(&self, illust_id: &str) -> Result<Option<UserWork>> {
        let url = format!("{}/illust/{}", PIXIV_AJAX, illust_id);
        let body_str = self
            .get_json_ajax(
                &url,
                &format!("{}/artworks/{}", PIXIV_BASE, illust_id),
            )
            .await?;
        let resp: IllustDetailResp =
            serde_json::from_str(&body_str).context("parse illust detail")?;
        let d = resp.body;
        Ok(Some(UserWork {
            id: d.id,
            title: d.title,
            tags: d.tags.tags.iter().map(|t| t.tag.clone()).collect(),
            page_count: d.page_count,
            illust_type: d.illust_type,
            author: d.user_name,
            author_id: d.user_id,
            published_at: d.create_date,
            cover_url: if d.urls.regular.is_empty() {
                None
            } else {
                Some(d.urls.regular)
            },
        }))
    }

    /// Fetch a user's display name via the public user AJAX API
    /// (`/ajax/user/<id>` → `body.name`), for the "logged in as <name>" label.
    pub async fn fetch_user_name(&self, user_id: &str) -> Result<String> {
        let url = format!("{}/user/{}", PIXIV_AJAX, user_id);
        let body_str = self
            .get_json_ajax(&url, &format!("{}/users/{}", PIXIV_BASE, user_id))
            .await?;
        let v: serde_json::Value =
            serde_json::from_str(&body_str).context("parse user ajax response")?;
        v.get("body")
            .and_then(|b| b.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no name in user response"))
    }
}

/// High-level downloader that turns bookmarks into registered CB7 books.
pub struct PixivDownloader {
    client: PixivClient,
    db: Arc<Database>,
    storage: Arc<StorageService>,
    pub(crate) cancelled: Arc<AtomicBool>,
}

impl PixivDownloader {
    pub fn new(
        cookie_str: &str,
        db: Arc<Database>,
        storage: Arc<StorageService>,
    ) -> Result<Self> {
        let client = PixivClient::new(cookie_str)?;
        Ok(Self {
            client,
            db,
            storage,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
        }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub async fn run(
        &self,
        user_id: &str,
        limit: u64,
        tags_mode: TagsMode,
        source_plugin: &str,
        sink: Arc<std::sync::Mutex<dyn PixivProgressSink>>,
    ) -> Result<PixivRunSummary> {
        let library_service =
            crate::services::LibraryService::new(self.db.clone(), self.storage.clone());

        sink.lock().unwrap().emit(PixivProgress::Phase {
                phase: "listing".into(),
                message: "Fetching bookmark list from pixiv.net...".into(),
            });

 let works = self
            .client
            .fetch_all_bookmarks(user_id, limit, &self.cancelled)
            .await?;

        let total = works.len() as u64;

        sink.lock().unwrap().emit(PixivProgress::Fetched {
            total_bookmarks: total,
        });

        let mut downloaded = 0u64;
        let mut skipped = 0u64;
        let mut failed = 0u64;

        for (idx, work) in works.into_iter().enumerate() {
            if self.cancelled.load(Ordering::Relaxed) {
                break;
            }

            let index = idx as u64 + 1;
            sink.lock().unwrap().emit(PixivProgress::WorkStart {
                index,
                total,
                illust_id: work.id.clone(),
                title: work.title.clone(),
            });

            match self.process_work(&work, user_id, tags_mode, &library_service, source_plugin)
                .await
            {
                Ok(DownloadedArtwork { pages, .. }) => {
                    downloaded += 1;
                    sink.lock().unwrap().emit(PixivProgress::WorkDone {
                        illust_id: work.id,
                        title: work.title,
                        pages,
                    });
                }
                Err(SkipReason::Ugoira) => {
                    skipped += 1;
                    sink.lock().unwrap().emit(PixivProgress::WorkSkipped {
                        illust_id: work.id,
                        title: work.title,
                        reason: "ugoira (animated) not supported".into(),
                    });
                }
                Err(SkipReason::AlreadyExists) => {
                    skipped += 1;
                    sink.lock().unwrap().emit(PixivProgress::WorkSkipped {
                        illust_id: work.id,
                        title: work.title,
                        reason: "already in library".into(),
                    });
                }
                Err(SkipReason::Other(msg)) => {
                    failed += 1;
                    sink.lock().unwrap().emit(PixivProgress::WorkFailed {
                        illust_id: work.id,
                        title: work.title,
                        error: msg,
                    });
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let summary = PixivRunSummary {
            downloaded,
            skipped,
            failed,
            total,
        };
        sink.lock().unwrap().emit(PixivProgress::Finished {
            downloaded,
            skipped,
            failed,
        });
        Ok(summary)
    }

    /// Download a specific user's own works (illustrations + manga). Used for
    /// the "关注 (following)" tab where the user picks a creator to fetch.
    /// Reuses the same per-work pipeline as `run`, so smart-skip and progress
    /// reporting behave identically.
    pub async fn download_user_works(
        &self,
        user_id: &str,
        limit: u64,
        source_plugin: &str,
        sink: Arc<std::sync::Mutex<dyn PixivProgressSink>>,
    ) -> Result<PixivRunSummary> {
        let library_service =
            crate::services::LibraryService::new(self.db.clone(), self.storage.clone());

        tracing::info!(user_id, limit, "download_user_works: starting, emitting initial Phase");
        sink.lock().unwrap().emit(PixivProgress::Phase {
            phase: "listing".into(),
            message: "Fetching user works from pixiv.net...".into(),
        });

        let works = self
            .client
            .fetch_user_works(user_id, limit, &self.cancelled)
            .await?;

        let total = works.len() as u64;

        sink.lock().unwrap().emit(PixivProgress::Fetched {
            total_bookmarks: total,
        });

        let mut downloaded = 0u64;
        let mut skipped = 0u64;
        let mut failed = 0u64;

        for (idx, work) in works.into_iter().enumerate() {
            if self.cancelled.load(Ordering::Relaxed) {
                break;
            }
            let index = idx as u64 + 1;
            let illust_id = work.id.clone();
            let work_title = work.title.clone();
            sink.lock().unwrap().emit(PixivProgress::WorkStart {
                index,
                total,
                illust_id: illust_id.clone(),
                title: work_title.clone(),
            });

            match self
                .process_work(
                    &work.into(),
                    user_id,
                    Default::default(),
                    &library_service,
                    source_plugin,
                )
                .await
            {
                Ok(DownloadedArtwork { pages, .. }) => {
                    downloaded += 1;
                    sink.lock().unwrap().emit(PixivProgress::WorkDone {
                        illust_id,
                        title: work_title,
                        pages,
                    });
                }
                Err(SkipReason::Ugoira) => {
                    skipped += 1;
                    sink.lock().unwrap().emit(PixivProgress::WorkSkipped {
                        illust_id,
                        title: work_title,
                        reason: "ugoira (animated) not supported".into(),
                    });
                }
                Err(SkipReason::AlreadyExists) => {
                    skipped += 1;
                    sink.lock().unwrap().emit(PixivProgress::WorkSkipped {
                        illust_id,
                        title: work_title,
                        reason: "already in library".into(),
                    });
                }
                Err(SkipReason::Other(msg)) => {
                    failed += 1;
                    sink.lock().unwrap().emit(PixivProgress::WorkFailed {
                        illust_id,
                        title: work_title,
                        error: msg,
                    });
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let summary = PixivRunSummary {
            downloaded,
            skipped,
            failed,
            total,
        };
        sink.lock().unwrap().emit(PixivProgress::Finished {
            downloaded,
            skipped,
            failed,
        });
        Ok(summary)
    }

    async fn process_work(
        &self,
        work: &BookmarkWork,
        _user_id: &str,
        tags_mode: TagsMode,
        library: &crate::services::LibraryService,
        source_plugin: &str,
    ) -> DownloadResult {
        let illust_id = &work.id;

        // Skip ugoira (type 2 animations), we can't make a static CB7 from them.
        if work.illust_type == Some(2) {
            return Err(SkipReason::Ugoira);
        }

        // Get pages up front so we can compare the live page count against what
        // we have stored (smart skip: re-download if the work was updated).
        let pages = self
            .client
            .fetch_pages(illust_id)
            .await
            .map_err(|e| SkipReason::Other(format!("fetch pages failed: {}", e)))?;

        if pages.is_empty() {
            return Err(SkipReason::Other("no pages returned".into()));
        }
        let new_page_count = pages.len() as i32;

        // Smart skip: compare live metadata (page count + title) to the stored
        // record. Identical → skip. Differ (e.g. the author added pages) →
        // re-download, replacing the old CB7 under the same book_id.
        let source_url = format!("{}/artworks/{}", PIXIV_BASE, illust_id);
        let existing = find_existing_by_source(&self.db.pool, &source_url)
            .await
            .map_err(|e| SkipReason::Other(e.to_string()))?;

        let book_id = if let Some(prev) = &existing {
            if prev.page_count == new_page_count && prev.title == work.title {
                return Err(SkipReason::AlreadyExists);
            }
            // Updated work: drop the old record + file, reuse the id.
            library
                .remove_book(&prev.book_id)
                .await
                .map_err(|e| SkipReason::Other(format!("remove old failed: {}", e)))?;
            prev.book_id.clone()
        } else {
            Uuid::new_v4().to_string()
        };

        let mut images: Vec<Vec<u8>> = Vec::new();
        for page in &pages {
            let url = if page.urls.original.is_empty() {
                &page.urls.regular
            } else {
                &page.urls.original
            };
            if url.is_empty() {
                continue;
            }
            match self.client.download_image(url).await {
                Ok(bytes) => images.push(bytes),
                Err(e) => {
                    // try regular as fallback if original failed
                    if !page.urls.regular.is_empty() && page.urls.regular != *url {
                        if let Ok(b) = self.client.download_image(&page.urls.regular).await {
                            images.push(b);
                            continue;
                        }
                    }
                    return Err(SkipReason::Other(format!("download {} failed: {}", url, e)));
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        if images.is_empty() {
            return Err(SkipReason::Other("no images downloaded".into()));
        }

        // Build CB7
        let file_path = self
            .storage
            .create_cb7(
                &images,
                &crate::models::BookMetadata {
                    title: work.title.clone(),
                    tags: collect_tags(work, tags_mode),
                    ..Default::default()
                },
            )
            .map_err(|e| SkipReason::Other(format!("create_cb7 failed: {}", e)))?;

        let source = BookSource {
            plugin: source_plugin.into(),
            source_url,
            scraped_at: Some(chrono::Utc::now()),
            ..Default::default()
        };

        let page_count = images.len() as i32;
        library
            .register_stored_book(
                &book_id,
                &work.title,
                &file_path,
                page_count,
                Some(&source),
                &collect_tags(work, tags_mode),
                None,
            )
            .await
            .map_err(|e| SkipReason::Other(format!("register failed: {}", e)))?;

        Ok(DownloadedArtwork {
            pages: page_count as u32,
        })
    }
}

fn collect_tags(work: &BookmarkWork, mode: TagsMode) -> Vec<String> {
    match mode {
        TagsMode::None => Vec::new(),
        TagsMode::All => work.tags.clone(),
        TagsMode::LocalOnly => work.tags.clone(),
    }
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(crate = "serde")]
pub enum TagsMode {
    #[default]
    None,
    All,
    LocalOnly,
}

/// Parse a Pixiv numeric user id out of a `/setting_user.php` redirect
/// `Location` header, e.g. `/users/12345678/setting` (relative) or the full
/// URL `https://www.pixiv.net/users/12345678/setting` → `Some("12345678")`.
/// Searches the path for a `/users/<digits>` segment rather than assuming a
/// relative path, since HTTP Location headers routinely carry the full URL.
fn user_id_from_setting_location(location: &str) -> Option<String> {
    let path = location.split('?').next().unwrap_or(location);
    for window in path.split('/').collect::<Vec<&str>>().windows(2) {
        if window[0] == "users" {
            let id = window[1];
            if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                return Some(id.to_string());
            }
        }
    }
    None
}

#[derive(Debug)]
pub enum SkipReason {
    Ugoira,
    AlreadyExists,
    Other(String),
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(crate = "serde")]
pub struct PixivRunSummary {
    pub total: u64,
    pub downloaded: u64,
    pub skipped: u64,
    pub failed: u64,
}

/// Stored metadata for an artwork we already have locally, keyed by source_url.
/// Used to decide whether a re-download is needed (smart skip).
#[derive(Debug, Default)]
pub struct ExistingArtwork {
    pub book_id: String,
    pub page_count: i32,
    pub title: String,
}

/// Look up a locally-stored artwork by its Pixiv source URL. Returns `None` if
/// we have never downloaded it. The caller compares `page_count`/`title` against
/// the live Pixiv metadata to decide whether to re-download an updated work.
pub async fn find_existing_by_source(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    source_url: &str,
) -> Result<Option<ExistingArtwork>> {
    let row: Option<(String, i32, String)> = sqlx::query_as(
        "SELECT id, page_count, title FROM books WHERE source_url = ?",
    )
    .bind(source_url)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, page_count, title)| ExistingArtwork {
        book_id: id,
        page_count,
        title,
    }))
}
