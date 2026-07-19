use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::services::pixiv::{PixivClient, UserWork};
use crate::AppState as LibState;

/// Persisted Pixiv login (cookie + the account's own user id).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PixivLogin {
    pub cookie: String,
    pub user_id: String,
    #[serde(default)]
    pub user_name: Option<String>,
}

/// Persistence record on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixivSessionFile {
    pub cookie: String,
    pub user_id: String,
    #[serde(default)]
    pub user_name: Option<String>,
    pub saved_at: String,
}

/// Holds the currently-running downloader so we can cancel it, and the active
/// Pixiv login captured from the in-app browser. When `persist_path` is set,
/// logins are written there on set and loaded from it at construction time so a
/// manual re-login is the only way to change credentials.
pub struct PixivSession {
    pub(crate) login: Mutex<Option<PixivLogin>>,
    pub(crate) persist_path: Option<PathBuf>,
}

impl PixivSession {
    /// Build a session that reads any previously-saved login from `path` and on
    /// every `set_login` writes the new credentials back to it. Write failures
    /// degrade gracefully to in-memory only.
    pub fn with_persist(path: PathBuf) -> Self {
        let loaded = Self::read_file(&path).map(PixivLogin::from);
        if loaded.is_some() {
            tracing::info!(target: "erolib::pixiv", ?path, "restored saved Pixiv login");
        }
        Self {
            login: Mutex::new(loaded),
            persist_path: Some(path),
        }
    }

    fn read_file(path: &std::path::Path) -> Option<PixivSessionFile> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice::<PixivSessionFile>(&bytes)
            .map_err(|e| {
                tracing::warn!(
                    target: "erolib::pixiv",
                    ?path,
                    %e,
                    "saved Pixiv session file is corrupt; ignoring"
                );
                e
            })
            .ok()
    }

    fn write_file(&self, login: &PixivLogin) {
        let Some(path) = &self.persist_path else {
            return;
        };
        let file = PixivSessionFile {
            cookie: login.cookie.clone(),
            user_id: login.user_id.clone(),
            user_name: login.user_name.clone(),
            saved_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let buf = match serde_json::to_vec(&file) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(target: "erolib::pixiv", %e, "serialize Pixiv login");
                return;
            }
        };
        if let Err(e) = std::fs::write(path, &buf) {
            tracing::warn!(
                target: "erolib::pixiv",
                ?path,
                %e,
                "failed to persist Pixiv login"
            );
        } else {
            tracing::debug!(target: "erolib::pixiv", ?path, "saved Pixiv login");
        }
    }

    pub fn set_login(&self, login: PixivLogin) {
        self.write_file(&login);
        *self.login.lock().unwrap() = Some(login);
    }

    pub fn get_login(&self) -> Option<PixivLogin> {
        self.login.lock().unwrap().clone()
    }

    /// Drop the in-memory login and remove the on-disk session file. The next
    /// `set_login` writes a fresh file.
    pub fn clear_login(&self) {
        if let Some(path) = &self.persist_path {
            let _ = std::fs::remove_file(path);
        }
        *self.login.lock().unwrap() = None;
    }
}

impl From<PixivSessionFile> for PixivLogin {
    fn from(f: PixivSessionFile) -> Self {
        Self {
            cookie: f.cookie,
            user_id: f.user_id,
            user_name: f.user_name,
        }
    }
}

/// Return the stored Pixiv login (cookie + user_id), if the user has logged in
/// via the in-app browser.
#[tauri::command]
pub async fn pixiv_get_login(
    session: State<'_, Arc<PixivSession>>,
) -> Result<Option<PixivLogin>, String> {
    Ok(session.get_login())
}

/// Store a Pixiv login captured from the in-app browser (cookie + user_id).
#[tauri::command]
pub async fn pixiv_set_login(
    cookie: String,
    user_id: String,
    session: State<'_, Arc<PixivSession>>,
) -> Result<(), String> {
    let cookie = cookie.trim().to_string();
    let user_id = user_id.trim().to_string();
    if cookie.is_empty() || user_id.is_empty() {
        return Err("cookie and user_id are required".into());
    }
    // Best-effort: resolve the display name so manual logins also show it.
    let user_name = match PixivClient::new(&cookie) {
        Ok(c) => c.fetch_user_name(&user_id).await.ok(),
        Err(_) => None,
    };
    session.set_login(PixivLogin {
        cookie,
        user_id,
        user_name,
    });
    Ok(())
}

/// Forget the stored Pixiv login — both in memory and on disk. The user then
/// re-authenticates via the in-app browser, which writes a fresh session. This
/// is the only path that replaces persisted credentials (stage 3). Also wipes
/// the in-app browser's cookie memory for pixiv.net so the next login window
/// doesn't auto-log-in from the stale session.
#[tauri::command]
pub async fn pixiv_clear_login(
    session: State<'_, Arc<PixivSession>>,
) -> Result<(), String> {
    session.clear_login();
    let _ = tauri::async_runtime::spawn_blocking(|| {
        crate::commands::cookies::clear_section_cookies(&["pixiv.net"])
    })
    .await;
    Ok(())
}

/// One page of browse results (收藏 tab lazy loading).
#[derive(Debug, Serialize)]
pub struct PixivBrowsePage {
    pub items: Vec<UserWork>,
    pub total: u64,
}

/// List one page of the logged-in user's bookmarks (收藏 tab). Returns the
/// page of works plus the total bookmark count so the caller knows when the
/// feed ends.
#[tauri::command]
pub async fn pixiv_list_bookmarks(
    offset: u64,
    limit: u64,
    session: State<'_, Arc<PixivSession>>,
) -> Result<PixivBrowsePage, String> {
    let login = session.get_login().ok_or("not logged in to pixiv")?;
    let client = PixivClient::new(&login.cookie).map_err(|e| e.to_string())?;
    let (works, total) = client
        .fetch_bookmarks_page(&login.user_id, offset, limit)
        .await
        .map_err(|e| e.to_string())?;
    let items = works.into_iter().map(UserWork::from).collect();
    Ok(PixivBrowsePage { items, total })
}

/// List one page of the logged-in user's following feed (关注 tab). `page`
/// is 1-based; the session cookie identifies the user (no user id in path).
#[tauri::command]
pub async fn pixiv_list_following_feed(
    page: u64,
    session: State<'_, Arc<PixivSession>>,
) -> Result<Vec<UserWork>, String> {
    let login = session.get_login().ok_or("not logged in to pixiv")?;
    let client = PixivClient::new(&login.cookie).map_err(|e| e.to_string())?;
    client
        .fetch_follow_latest(page)
        .await
        .map_err(|e| e.to_string())
}

/// List one page of Pixiv's home recommendation feed (随便看看 tab) — works
/// pushed to the logged-in homepage based on the user's taste.
#[tauri::command]
pub async fn pixiv_list_recommended(
    page: u64,
    session: State<'_, Arc<PixivSession>>,
) -> Result<Vec<UserWork>, String> {
    let login = session.get_login().ok_or("not logged in to pixiv")?;
    let client = PixivClient::new(&login.cookie).map_err(|e| e.to_string())?;
    client
        .fetch_recommended(page)
        .await
        .map_err(|e| e.to_string())
}

/// Search illustrations by keyword. `page` is 1-based and returns ~60 works.
#[tauri::command]
pub async fn pixiv_search_illusts(
    keyword: String,
    page: u64,
    session: State<'_, Arc<PixivSession>>,
) -> Result<Vec<UserWork>, String> {
    let login = session.get_login().ok_or("not logged in to pixiv")?;
    let client = PixivClient::new(&login.cookie).map_err(|e| e.to_string())?;
    client
        .fetch_search(&keyword, page)
        .await
        .map_err(|e| e.to_string())
}

/// Proxy a Pixiv image (i.pximg.net) through the backend so the frontend can
/// render covers — the image host requires a `Referer: https://www.pixiv.net/`
/// header that browsers forbid setting on `<img>`.
#[tauri::command]
pub async fn pixiv_proxy_image(
    url: String,
    session: State<'_, Arc<PixivSession>>,
) -> Result<Vec<u8>, String> {
    let cookie = session.get_login().map(|l| l.cookie).unwrap_or_default();
    let client = PixivClient::new(&cookie).map_err(|e| e.to_string())?;
    client.download_image(&url).await.map_err(|e| e.to_string())
}

/// Per-work browse status: whether it's already in the library, currently
/// downloading, or neither — so the Pixiv grid can render the right card state.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PixivBrowseStatus {
    pub work_id: String,
    pub local_book_id: Option<String>,
    pub task_id: Option<String>,
    pub task_status: Option<String>,
    pub progress_current: i64,
    pub progress_total: i64,
}

/// Resolve the local state of a batch of Pixiv work ids in one call:
/// - `local_book_id`: a book already imported from this work (`source_url` match).
/// - `task_id`/`task_status`/`progress_*`: an active (pending/running/paused)
///   download task for this work, if any.
/// A downloaded book takes priority over an in-flight task.
#[tauri::command]
pub async fn pixiv_browse_status(
    work_ids: Vec<String>,
    state: State<'_, LibState>,
) -> Result<Vec<PixivBrowseStatus>, String> {
    use crate::services::task::TaskPayload;
    use std::collections::HashMap;
    let pool = &state.db_inner().pool;

    // 1. Local books by source_url = .../artworks/{work_id}.
    let mut book_by_work: HashMap<String, String> = HashMap::new();
    if !work_ids.is_empty() {
        let placeholders = (0..work_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, source_url FROM books WHERE source_url IN ({})",
            placeholders
        );
        let mut q = sqlx::query_as::<_, (String, Option<String>)>(&sql);
        for id in &work_ids {
            q = q.bind(format!("https://www.pixiv.net/artworks/{}", id));
        }
        let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
        for (book_id, source_url) in rows {
            if let Some(url) = source_url {
                if let Some(wid) = url.rsplit('/').next() {
                    book_by_work.insert(wid.to_string(), book_id);
                }
            }
        }
    }

    // 2. Active pixiv tasks — parse payload JSON for the work_id.
    let mut task_by_work: HashMap<String, (String, String, i64, i64)> = HashMap::new();
    let rows: Vec<(String, String, i64, i64, String)> = sqlx::query_as(
        "SELECT id, status, progress_current, progress_total, payload FROM tasks \
         WHERE source = 'pixiv' AND status IN ('pending','running','paused')",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    for (id, status, cur, total, payload_str) in rows {
        if let Ok(TaskPayload::PixivSingleWork { work_id, .. }) =
            serde_json::from_str::<TaskPayload>(&payload_str)
        {
            task_by_work.entry(work_id).or_insert((id, status, cur, total));
        }
    }

    // 3. Assemble (downloaded > in-flight > not-yet).
    let result = work_ids
        .into_iter()
        .map(|wid| {
            if let Some(book_id) = book_by_work.get(&wid) {
                PixivBrowseStatus {
                    work_id: wid,
                    local_book_id: Some(book_id.clone()),
                    task_id: None,
                    task_status: None,
                    progress_current: 0,
                    progress_total: 0,
                }
            } else if let Some((tid, st, cur, total)) = task_by_work.get(&wid) {
                PixivBrowseStatus {
                    work_id: wid,
                    local_book_id: None,
                    task_id: Some(tid.clone()),
                    task_status: Some(st.clone()),
                    progress_current: *cur,
                    progress_total: *total,
                }
            } else {
                PixivBrowseStatus {
                    work_id: wid,
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

