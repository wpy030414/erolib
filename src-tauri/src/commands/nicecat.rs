use std::collections::HashMap;

use serde::Serialize;
use tauri::State;

use crate::AppState as LibState;

/// Proxy a cover thumbnail (uses shared reqwest client, no WebView needed).
#[tauri::command]
pub async fn nicecat_proxy_thumb(url: String) -> Result<Vec<u8>, String> {
    let client = crate::services::NicecatClient::new().map_err(|e| e.to_string())?;
    client.proxy_thumb(&url).await.map_err(|e| e.to_string())
}

/// Per-comic browse status.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NicecatBrowseStatus {
    pub comic_id: String,
    pub local_book_id: Option<String>,
    pub task_id: Option<String>,
    pub task_status: Option<String>,
    pub progress_current: i64,
    pub progress_total: i64,
}

#[tauri::command]
pub async fn nicecat_browse_status(
    comic_ids: Vec<String>,
    state: State<'_, LibState>,
) -> Result<Vec<NicecatBrowseStatus>, String> {
    use std::collections::HashMap as StdHashMap;
    let pool = &state.db_inner().pool;

    let mut book_by_id: StdHashMap<String, String> = StdHashMap::new();
    let book_rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, source_post_id FROM books WHERE source_plugin = 'nicecat'")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;
    for (book_id, source_post_id) in book_rows {
        if let Some(pid) = source_post_id {
            if !pid.is_empty() {
                book_by_id.insert(pid, book_id);
            }
        }
    }

    let mut task_by_id: StdHashMap<String, (String, String, i64, i64)> = StdHashMap::new();
    let task_rows: Vec<(String, String, i64, i64, String)> = sqlx::query_as(
        "SELECT id, status, progress_current, progress_total, payload FROM tasks \
         WHERE source = 'nicecat' AND status IN ('pending','running','paused')",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    for (id, status, cur, total, payload_str) in task_rows {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&payload_str) {
            if let Some(cid) = payload.get("comic_id").and_then(|v| v.as_str()) {
                task_by_id
                    .entry(cid.to_string())
                    .or_insert((id, status, cur, total));
            }
        }
    }

    let result = comic_ids
        .into_iter()
        .map(|cid| {
            if let Some(book_id) = book_by_id.get(&cid) {
                NicecatBrowseStatus { comic_id: cid, local_book_id: Some(book_id.clone()), task_id: None, task_status: None, progress_current: 0, progress_total: 0 }
            } else if let Some((tid, st, cur, total)) = task_by_id.get(&cid) {
                NicecatBrowseStatus { comic_id: cid, local_book_id: None, task_id: Some(tid.clone()), task_status: Some(st.clone()), progress_current: *cur, progress_total: *total }
            } else {
                NicecatBrowseStatus { comic_id: cid, local_book_id: None, task_id: None, task_status: None, progress_current: 0, progress_total: 0 }
            }
        })
        .collect();
    Ok(result)
}

/// Main API proxy — delegates to pure HTTP client (no WebView needed).
#[tauri::command]
pub async fn nicecat_fetch_api(
    path: String,
    form_fields: HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    crate::services::nicecat::run_api_call(&path, &form_fields).await
}
