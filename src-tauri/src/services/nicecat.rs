use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{Datelike, Local, TimeZone};
use rand::Rng;
use reqwest::Client;
use serde_json::Value;
use sha2::{Digest, Sha256};

const NICECAT_BASE: &str = "https://ncmm.cc";
const GXXA_BASE: &str = "https://gxxa.fun";
const UA: &str ="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

// ---------------------------------------------------------------------------
// RC4 token generation
// ---------------------------------------------------------------------------

/// Hardcoded RC4 key — extracted from getSecurityCode, same as in the JS client.
const RC4_KEY: &str = "Zo1Eq4V2mr269K4doL9U4093U25acjMQ";
/// Hardcoded authentication secret.
const AUTH: &str = "ec8be430bc634535b258b3591a414a67";

/// Allowed characters for the per-request random UID.
const UID_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// RC4 / ArcFour stream cipher.  Returns `key` XOR `data`.
fn rc4_encrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: Vec<u8> = (0..=255).collect();
    let mut j: usize = 0;
    for i in 0..256 {
        j = (j + s[i] as usize + key[i % key.len()] as usize) & 0xff;
        s.swap(i, j);
    }
    let mut i: usize = 0;
    j = 0;
    let mut out = Vec::with_capacity(data.len());
    for &b in data {
        i = (i + 1) & 0xff;
        j = (j + s[i] as usize) & 0xff;
        s.swap(i, j);
        let k = s[(s[i] as usize + s[j] as usize) & 0xff];
        out.push(b ^ k);
    }
    out
}

/// Generate a fresh N-SECURITY-CERTIFICATIONS token (new random uid each time).
fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    let uid: String = (0..48)
        .map(|_| {
            let idx = rng.gen_range(0..UID_ALPHABET.len());
            UID_ALPHABET[idx] as char
        })
        .collect();
    let payload = serde_json::json!({
        "uid": uid,
        "authentication": AUTH,
    })
    .to_string();
    let cipher = rc4_encrypt(RC4_KEY.as_bytes(), payload.as_bytes());
    BASE64.encode(&cipher)
}

// ---------------------------------------------------------------------------
// Shared reqwest client (for thumbnail proxy)
// ---------------------------------------------------------------------------

static SHARED_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::default())
        .build()
        .expect("build shared reqwest client")
});

pub struct NicecatClient {
    http: Client,
}

impl NicecatClient {
    pub fn new() -> Result<Self> {
        Ok(Self { http: SHARED_CLIENT.clone() })
    }

    pub async fn proxy_thumb(&self, url: &str) -> Result<Vec<u8>> {
        let bytes = self.http
            .get(url)
            .header("Referer", &format!("{}/", NICECAT_BASE))
            .header("Origin", NICECAT_BASE)
            .send().await.context("proxy thumb")?
            .bytes().await.context("read thumb bytes")?
            .to_vec();
        if bytes.len() < 200 {
            anyhow::bail!("suspiciously small image from {url}");
        }
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------
// Pure-HTTP API client (replaces the persistent WebView)
// ---------------------------------------------------------------------------

/// Stateless HTTP client that calls the gxxa.fun API directly using an
/// RC4-generated N-SECURITY-CERTIFICATIONS token — no headless browser needed.
pub(crate) struct NicecatApiClient {
    http: Client,
}

impl NicecatApiClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::default())
            .build()
            .context("build nicecat api client")?;
        Ok(Self { http })
    }

    /// POST with a fresh token every call.
    /// **Important**: each token is one-shot — reuse returns 403.
    pub async fn api_post(
        &self,
        path: &str,
        form_fields: &std::collections::HashMap<String, String>,
    ) -> Result<Value> {
        let token = generate_token();
        let url = format!("{}{}", GXXA_BASE, path);

        tracing::debug!(target: "erolib::nicecat", %path, "api_post start");

        let mut req = self.http
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded;charset=UTF-8")
            .header("N-SECURITY-CERTIFICATIONS", &token)
            .header("N-Application-Type", "WEB")
            .header("Api-Version", "1.0.0")
            .header("Origin", NICECAT_BASE)
            .header("Referer", &format!("{}/", NICECAT_BASE))
            .header("Accept", "application/json");

        if !form_fields.is_empty() {
            req = req.form(form_fields);
        }

        let resp = req.send().await.context("gxxa api call")?;
        let status = resp.status();
        let text = resp.text().await.context("read api response")?;

        tracing::debug!(target: "erolib::nicecat", %path, %status, len = text.len(), "api_post done");

        if !status.is_success() {
            anyhow::bail!(
                "gxxa API {} returned HTTP {}: {}",
                path, status,
                &text[..text.len().min(200)]
            );
        }

        let json: Value = serde_json::from_str(&text)
            .with_context(|| format!("parse gxxa JSON from {} ({} bytes)", path, text.len()))?;
        Ok(json)
    }
}

// ---------------------------------------------------------------------------
// Shared API client (persistent — avoids new TCP/TLS handshake per call)
// ---------------------------------------------------------------------------

static API_CLIENT: LazyLock<NicecatApiClient> = LazyLock::new(|| {
    NicecatApiClient::new().expect("build nicecat api client")
});

// ---------------------------------------------------------------------------
// Public API — called from commands/nicecat.rs
// ---------------------------------------------------------------------------

/// Execute a NiceCat API call via pure HTTP (no WebView).
///
/// # Search flow (token rotation + cursor paging)
///
/// **Critical**: each N-SECURITY-CERTIFICATIONS token is one-shot — reusing
/// the same token for a second API call returns 403.  However, the `searchId`
/// cursor returned by `searchTag` is NOT token-bound: a fresh token can use
/// the same searchId to advance the cursor.
///
/// Therefore every API call uses a **new random token**.
///
/// ## Single-tag model
///
/// For page 1 we do a two-phase lookup.  `ComicSearch/search` resolves a
/// keyword to several tags in `tagData.data[]`; we pick the SINGLE best one
/// — the tag with the largest `comic_number` — and pass only that one's `uid`
/// to `searchTag`.  (The old aggregate approach — sending every tag as a JSON
/// array — was wrong and has been reverted.)
///
/// ## Cursor paging (mirrors EHentai)
///
/// `searchTag` returns a `searchId` cursor alongside the first page.
/// `map_search_tag_response` extracts it into `nextCursor`.  The frontend
/// passes that `nextCursor` back as the `cursor` form field; page > 1 then
/// calls `searchTag` with `searchId=<cursor>` to advance.  The cursor is
/// exhausted when the page returns 0 items (we then return the end envelope
/// `4000200 { list: [], nextCursor: "" }`).  End-of-feed on the frontend is
/// independent (page granularity 60); the Rust side just serves pages.
///
/// **Page 1** (keyword → best tag → results):
/// 1. `ComicSearch/search` (token A) → pick single best tag_uid
/// 2. `ComicSearch/searchTag` with `tagUid=[uid]` (token B) → page 1 + searchId
///
/// **Page N > 1** (cursor advance):
/// 1. `ComicSearch/searchTag` with `searchId` (token N+1) → page N
///
/// The frontend composes these into its own 48-item page unit; the Rust side
/// returns each cursor page (up to 60 items) as-is.
pub async fn run_api_call(
    path: &str,
    form_fields: &std::collections::HashMap<String, String>,
) -> Result<Value, String> {
    let client: &NicecatApiClient = &API_CLIENT;

    let is_search = path.contains("ComicSearch") || path.contains("search");
    if !is_search {
        let raw = client.api_post(path, form_fields).await.map_err(|e| e.to_string())?;
        return Ok(map_homepage_response(&raw));
    }

    // ---- Search ----
    // Frontend sends a `cursor` field: "" means page 1 (keyword → searchTag),
    // non-empty means advance the existing searchId cursor.
    let search_cursor = form_fields.get("cursor")
        .map(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    // Page > 1: advance cursor with searchId.
    // The searchId cursor returned by searchTag is valid for one more page
    // (searchTag P1 returns a searchId; P2 uses it but returns empty searchId,
    // meaning the cursor is exhausted).  If we have a non-empty cursor, always
    // try searchTag first.
    if !search_cursor.is_empty() {
        let raw = client
            .api_post(
                "/api/ComicSearch/searchTag",
                &[("searchId".into(), search_cursor.clone())].into_iter().collect(),
            )
            .await
            .map_err(|e| e.to_string())?;
        let list_len = raw.get("data")
            .and_then(|d| d.get("comic"))
            .and_then(|c| c.get("data"))
            .and_then(|arr| arr.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        if list_len > 0 {
            return Ok(map_search_tag_response(&raw));
        }
        // Cursor exhausted (empty results) — return empty list to signal end.
        return Ok(serde_json::json!({
            "code": "4000200",
            "data": { "list": [], "total": null, "nextCursor": "" }
        }));
    }

    // Page 2: try to continue with stored cursor (keyword → searchTag page 2)

    // Page 1: two-phase (search → best tag → searchTag)
    // Phase 1: ComicSearch/search to discover tag candidates; pick ONE best
    // tag_uid — the one with the largest comic_number.
    let raw_search = client
        .api_post(path, form_fields)
        .await
        .map_err(|e| e.to_string())?;

    let tag_uid = match find_best_tag_uid(&raw_search) {
        Some(uid) => uid,
        None => return Ok(map_search_response(&raw_search)),
    };
    let tag_uid_json = serde_json::json!([&tag_uid]).to_string();

    // Phase 2: searchTag with fresh token → page 1 + searchId.
    // `tagUid` is a single-element JSON array of the chosen tag uid.
    let raw_tag = client
        .api_post(
            "/api/ComicSearch/searchTag",
            &[("tagUid".into(), tag_uid_json)].into_iter().collect(),
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(map_search_tag_response(&raw_tag))
}

// ---------------------------------------------------------------------------
// Date-key + download helpers (pure-HTTP replacement for the WebView interceptor)
// ---------------------------------------------------------------------------

/// Compute the per-day `dateKey` used by `ComicOrder/getComicOrder`.
///
/// Mirrors the JS: `new Date(); setHours(0,0,0,0); base64(sha256(getTime()))`
/// where `getTime()` is local-midnight expressed as epoch milliseconds.
fn compute_date_key() -> String {
    let now = Local::now();
    let midnight_local = Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .unwrap();
    let ms = midnight_local.timestamp_millis();
    let hash = Sha256::digest(ms.to_string().as_bytes());
    BASE64.encode(&hash)
}

/// Fetch comic metadata via `POST /api/ComicInfo/info` (form field `uid`).
/// Returns the FULL response JSON as a string (the Rust client has no
/// JS interceptor to unwrap `.data`, so callers parse the envelope).
pub async fn fetch_comic_info_raw(comic_id: &str) -> Result<String, String> {
    let client = &API_CLIENT;
    let mut form = HashMap::new();
    form.insert("uid".to_string(), comic_id.to_string());
    let json = client
        .api_post("/api/ComicInfo/info", &form)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

/// Fetch the ordered image list via `POST /api/ComicOrder/getComicOrder`
/// (form fields `comicUid`, `sort=0`, `dateKey`).  Returns the FULL response
/// JSON as a string for the caller to parse.
pub async fn fetch_comic_order_raw(comic_id: &str) -> Result<String, String> {
    let client = &API_CLIENT;
    let mut form = HashMap::new();
    form.insert("comicUid".to_string(), comic_id.to_string());
    form.insert("sort".to_string(), "0".to_string());
    form.insert("dateKey".to_string(), compute_date_key());
    let json = client
        .api_post("/api/ComicOrder/getComicOrder", &form)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Response mappers
// ---------------------------------------------------------------------------

/// Pick the single best tag (max comic_number) from the `tagData.data[]` array.
///
/// `ComicSearch/search` resolves a keyword to multiple tags; each carries a
/// `uid` + `comic_number`.  We take the tag with the largest comic_number and
/// return only its uid — the real browse-feed flows on a SINGLE tag.  The old
/// aggregate approach (every tag at once) was a bug and has been reverted.
fn find_best_tag_uid(raw: &Value) -> Option<String> {
    raw.get("data")
        .and_then(|d| d.get("tagData"))
        .and_then(|td| td.get("data"))
        .and_then(|arr| arr.as_array())
        .and_then(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let uid = t.get("uid").and_then(|v| v.as_str())?;
                    let n = t.get("comic_number").and_then(|v| v.as_i64()).unwrap_or(0);
                    Some((uid.to_string(), n))
                })
                .max_by_key(|(_, n)| *n)
                .map(|(uid, _)| uid)
        })
}

/// Map a `searchTag` API response into the standardised result shape.
fn normalize_comic_item(mut item: Value) -> Value {
    // Upstream items have NO `name` field; the actual title lives in
    // `jp_name` / `en_name` / <unknown>.  The frontend's SourceCard requires
    // a non-undefined `title`, so we MUST always populate `name`: try
    // `name → jp_name → en_name → <uid tail>` in that order.  If every field
    // is missing, fall back to the comic's uid so the card still renders.
    let current = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if current.is_empty() {
        // name → jp_name → en_name → <uid tail>: always produce a String so
        // every item carries a non-empty name for the frontend's <SourceCard>.
        let fallback: String = item.get("jp_name").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from)
            .or_else(|| item.get("en_name").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from))
            .unwrap_or_else(|| item.get("uid").and_then(|v| v.as_str()).unwrap_or("").chars().take(12).collect());
        if fallback.is_empty() {
            item["name"] = Value::String("untitled".to_string());
        } else {
            item["name"] = Value::String(fallback);
        }
    }
    item
}

/// `searchTag` returns `data.comic.data[]` (up to 60 per page) + `data.searchId`.
/// We inject `nextCursor` so the frontend's `useBrowseFeed` can pass it back
/// as the cursor for the next page.
fn map_search_tag_response(raw: &Value) -> Value {
    let list: Vec<Value> = raw.get("data")
        .and_then(|d| d.get("comic"))
        .and_then(|c| c.get("data"))
        .and_then(|arr| arr.as_array())
        .map(|arr| arr.iter().cloned().map(normalize_comic_item).collect())
        .unwrap_or_default();

    let next_cursor = raw.get("data")
        .and_then(|d| d.get("searchId"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    serde_json::json!({
        "code": "4000200",
        "data": { "list": list, "total": null, "nextCursor": next_cursor }
    })
}

fn map_homepage_response(raw: &Value) -> Value {
    let data = raw.get("data");

    let sections: Vec<Value> = data
        .and_then(|d| d.get("comic_data"))
        .and_then(|arr| arr.as_array())
        .map(|arr| {
            arr.iter()
                .map(|sec| {
                    serde_json::json!({
                        "ViewName": sec.get("name").cloned().unwrap_or(Value::Null),
                        "ViewDataArray": sec.get("data").cloned().unwrap_or(serde_json::json!([])),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let tag_data = data.and_then(|d| d.get("tag_data")).cloned().unwrap_or(serde_json::json!([]));
    let recommend_data = data.and_then(|d| d.get("recommend_data")).cloned();

    serde_json::json!({
        "code": "4000200",
        "data": { "homeData": sections, "tagData": tag_data, "recommend": recommend_data }
    })
}

/// Fallback: extract comics from a `ComicSearch/search` response
/// (used when searchTag is unavailable or no tags exist).
fn map_search_response(raw: &Value) -> Value {
    let raw_list = find_comic_list(raw);
    let list: Vec<Value> = raw_list.into_iter().map(normalize_comic_item).collect();

    serde_json::json!({
        "code": "4000200",
        "data": { "list": list, "total": null }
    })
}

fn find_comic_list(v: &Value) -> Vec<Value> {
    fn walk(v: &Value, depth: u32) -> Option<Vec<Value>> {
        if depth > 6 { return None; }
        if let Some(obj) = v.as_object() {
            for key in &["comic", "comicData"] {
                if let Some(cd) = obj.get(*key) {
                    if let Some(arr) = cd.get("data").and_then(|d| d.as_array()) {
                        if !arr.is_empty() { return Some(arr.clone()); }
                    }
                }
            }
            for (_k, child) in obj {
                if let found @ Some(_) = walk(child, depth + 1) { return found; }
            }
        }
        None
    }
    walk(v, 0).unwrap_or_default()
}
