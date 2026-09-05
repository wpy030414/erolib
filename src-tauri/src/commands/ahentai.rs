use serde::Serialize;
use tauri::State;

use crate::services::AhentaiClient;
use crate::AppState as LibState;

/// Search asmhentai.com — homepage when `keyword` is None/empty, keyword
/// search otherwise. `page` is the source page number (1-based, 20 items
/// per page). No login is required.
#[tauri::command]
pub async fn ahentai_search(
    keyword: Option<String>,
    page: Option<u32>,
) -> Result<Vec<crate::services::ahentai::AhentaiGalleryItem>, String> {
    let client = AhentaiClient::new().map_err(|e| e.to_string())?;
    let p = page.unwrap_or(1);
    match keyword.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(kw) => client.fetch_search(kw, p).await.map_err(|e| e.to_string()),
        None => client.fetch_homepage(p).await.map_err(|e| e.to_string()),
    }
}

/// Proxy a thumbnail image through the backend so the frontend can render
/// covers without hitting cross-origin / Referer issues.
#[tauri::command]
pub async fn ahentai_proxy_thumb(url: String) -> Result<Vec<u8>, String> {
    let client = AhentaiClient::new().map_err(|e| e.to_string())?;
    client.proxy_thumb(&url).await.map_err(|e| e.to_string())
}

/// Per-gallery browse status: already in library, currently downloading, or
/// new — so the AHentai grid can render the right card state.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AhentaiBrowseStatus {
    pub gallery_id: String,
    pub local_book_id: Option<String>,
    pub task_id: Option<String>,
    pub task_status: Option<String>,
    pub progress_current: i64,
    pub progress_total: i64,
}

/// Resolve the local state of a batch of gallery IDs in one call.
/// Matching is by source_url containing `asmhentai.com/g/{id}/`.
#[tauri::command]
pub async fn ahentai_browse_status(
    gallery_ids: Vec<String>,
    state: State<'_, LibState>,
) -> Result<Vec<AhentaiBrowseStatus>, String> {
    use std::collections::HashMap;
    let pool = &state.db_inner().pool;

    // 1. Local asmhentai books keyed by gallery ID extracted from source_url.
    let mut book_by_id: HashMap<String, String> = HashMap::new();
    let book_rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, source_url FROM books WHERE source_plugin = 'asmhentai'")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;
    for (book_id, source_url) in book_rows {
        if let Some(url) = source_url {
            if let Some(gid) = extract_gallery_id(&url) {
                book_by_id.insert(gid, book_id);
            }
        }
    }

    // 2. Active asmhentai tasks keyed by the payload's gallery ID.
    let mut task_by_id: HashMap<String, (String, String, i64, i64)> = HashMap::new();
    let task_rows: Vec<(String, String, i64, i64, String)> = sqlx::query_as(
        "SELECT id, status, progress_current, progress_total, payload FROM tasks \
         WHERE source = 'asmhentai' AND status IN ('pending','running','paused')",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    for (id, status, cur, total, payload_str) in task_rows {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&payload_str) {
            if let Some(gid) = payload.get("gallery_id").and_then(|v| v.as_str()) {
                task_by_id
                    .entry(gid.to_string())
                    .or_insert((id, status, cur, total));
            }
        }
    }

    // 3. Assemble (downloaded > in-flight > new).
    let result = gallery_ids
        .into_iter()
        .map(|gid| {
            if let Some(book_id) = book_by_id.get(&gid) {
                AhentaiBrowseStatus {
                    gallery_id: gid,
                    local_book_id: Some(book_id.clone()),
                    task_id: None,
                    task_status: None,
                    progress_current: 0,
                    progress_total: 0,
                }
            } else if let Some((tid, st, cur, total)) = task_by_id.get(&gid) {
                AhentaiBrowseStatus {
                    gallery_id: gid,
                    local_book_id: None,
                    task_id: Some(tid.clone()),
                    task_status: Some(st.clone()),
                    progress_current: *cur,
                    progress_total: *total,
                }
            } else {
                AhentaiBrowseStatus {
                    gallery_id: gid,
                    local_book_id: None,
                    task_id: None,
                    task_status: None,
                    progress_current: 0,
                    progress_total: 0,
                }
            }
        })
        .collect();
    Ok(result)
}

/// Extract the numeric gallery ID from an asmhentai source URL like
/// `https://asmhentai.com/g/660080/`.
fn extract_gallery_id(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let segs: Vec<&str> = parsed
        .path_segments()
        .map(|s| s.collect())
        .unwrap_or_default();
    if segs.len() >= 2 && segs[0] == "g" {
        let id = segs[1];
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            return Some(id.to_string());
        }
    }
    None
}
