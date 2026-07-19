use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};




const PIXIV_BASE: &str = "https://www.pixiv.net";
const PIXIV_AJAX: &str = "https://www.pixiv.net/ajax";

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
