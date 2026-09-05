use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use crate::commands::cookies::{capture_all_cookies, has_ehentai_session};
use crate::services::{
    EhentaiClient, EhentaiDownloader, GalleryListItem, PixivProgress, PixivProgressSink,
};
use crate::AppState as LibState;

/// Persisted EHentai login record on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EhentaiSessionFile {
    pub cookie: String,
    pub saved_at: String,
}

/// Holds the captured e-hentai session cookie and the cancel flag shared with
/// the in-flight downloader. Mirrors the shape of `commands::pixiv::PixivSession`.
pub struct EhentaiSession {
    pub(crate) cookie: Mutex<Option<String>>,
    pub(crate) cancelled: AtomicBool,
    pub(crate) persist_path: Option<PathBuf>,
}

impl EhentaiSession {
    /// Build a session that reads any previously-saved cookie from `path` and
    /// writes it back on every successful login. Write failures degrade
    /// gracefully to in-memory only.
    pub fn with_persist(path: PathBuf) -> Self {
        let loaded = Self::read_file(&path);
        if loaded.is_some() {
            tracing::info!(target: "erolib::ehentai", ?path, "restored saved EHentai login");
        }
        Self {
            cookie: Mutex::new(loaded),
            cancelled: AtomicBool::new(false),
            persist_path: Some(path),
        }
    }

    fn read_file(path: &std::path::Path) -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice::<EhentaiSessionFile>(&bytes)
            .map_err(|e| {
                tracing::warn!(
                    target: "erolib::ehentai",
                    ?path,
                    %e,
                    "saved EHentai session file is corrupt; ignoring"
                );
                e
            })
            .ok()
            .map(|f| f.cookie)
    }

    fn write_file(&self, cookie: &str) {
        let Some(path) = &self.persist_path else {
            return;
        };
        let file = EhentaiSessionFile {
            cookie: cookie.to_string(),
            saved_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let buf = match serde_json::to_vec(&file) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(target: "erolib::ehentai", %e, "serialize EHentai login");
                return;
            }
        };
        if let Err(e) = std::fs::write(path, &buf) {
            tracing::warn!(
                target: "erolib::ehentai",
                ?path,
                %e,
                "failed to persist EHentai login"
            );
        } else {
            tracing::debug!(target: "erolib::ehentai", ?path, "saved EHentai login");
        }
    }

    pub fn set_cookie(&self, cookie: String) {
        self.write_file(&cookie);
        *self.cookie.lock().unwrap() = Some(cookie);
    }

    pub fn get_cookie(&self) -> Option<String> {
        self.cookie.lock().unwrap().clone()
    }

    /// Drop the in-memory cookie and remove the on-disk session file. The next
    /// `set_cookie` writes a fresh file.
    pub fn clear_cookie(&self) {
        if let Some(path) = &self.persist_path {
            let _ = std::fs::remove_file(path);
        }
        *self.cookie.lock().unwrap() = None;
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn reset_cancel(&self) {
        self.cancelled.store(false, Ordering::Relaxed);
    }
}

/// Return the persisted EHentai cookie so the frontend can restore its login UI.
#[tauri::command]
pub fn ehentai_get_login(session: State<'_, Arc<EhentaiSession>>) -> Result<Option<String>, String> {
    Ok(session.get_cookie())
}

/// Forget the stored EHentai cookie — in memory, on disk, and the in-app
/// browser's cookie memory for e-hentai.org / exhentai.org so the next login
/// window starts fresh. This is the logout action.
#[tauri::command]
pub async fn ehentai_clear_login(
    session: State<'_, Arc<EhentaiSession>>,
) -> Result<(), String> {
    session.clear_cookie();
    let _ = tauri::async_runtime::spawn_blocking(|| {
        crate::commands::cookies::clear_section_cookies(&["e-hentai.org", "exhentai.org"])
    })
    .await;
    Ok(())
}

/// Manually set the EHentai session cookie (e.g. pasted by the user).
#[tauri::command]
pub fn ehentai_set_login(
    cookie: String,
    session: State<'_, Arc<EhentaiSession>>,
) -> Result<(), String> {
    if cookie.trim().is_empty() {
        return Err("cookie is required".into());
    }
    if !crate::commands::cookies::has_ehentai_session(&cookie) {
        return Err("cookie does not appear to be a valid EHentai session (missing ipb_member_id/ipb_pass_hash)".into());
    }
    session.set_cookie(cookie.trim().to_string());
    Ok(())
}

/// Open an in-app browser pointed at the e-hentai forums login page. The session
/// cookies (`ipb_member_id` / `ipb_pass_hash`) are then captured via JS eval
/// from the shared webview. We poll the URL and attempt capture as soon as the
/// window navigates onto a post-login `e-hentai.org` / `exhentai.org` page; if
/// the user closes early we still attempt a best-effort capture.
///
/// The frontend calls this command, then listens for the `ehentai://login`
/// event carrying `{ cookie }`. If cookie is empty, the user should paste it
/// manually via the frontend's manual entry field.
#[tauri::command]
pub async fn ehentai_open_login_window(
    app_handle: AppHandle,
    session: State<'_, Arc<EhentaiSession>>,
) -> Result<(), String> {
    let window = crate::commands::cookies::adapter::EHENTAI
        .open_login_window(&app_handle)
        .map_err(|e| format!("open ehentai login window: {e}"))?;

    let win_label = window.label().to_string();
    let app_for_poll = app_handle.clone();
    let session_for_poll = session.inner().clone();
    let win_for_poll = window.clone();

    // Poll until the window lands on a post-login e-hentai/exhentai page, then
    // capture cookies. Try JS eval capture immediately when we see the right host.
    tauri::async_runtime::spawn(async move {
        let mut ticks: u32 = 0;
        let mut captured = false;
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            ticks += 1;
            if ticks > 1200 {
                break;
            }
            let win = match app_for_poll.get_webview_window(&win_label) {
                Some(w) => w,
                None => return,
            };
            let url = match win.url() {
                Ok(u) => u,
                Err(_) => continue,
            };
            if !crate::commands::cookies::adapter::EHENTAI.is_post_login(&url) {
                continue;
            }

            // Try native cookie capture from the webview's own data store
            // (captures HttpOnly cookies too, though EHentai's are not HttpOnly).
            let app_clone = app_for_poll.clone();
            if let Ok(Some(c)) = tauri::async_runtime::spawn(async move {
                capture_all_cookies(&app_clone).await
            })
            .await
            {
                if has_ehentai_session(&c) {
                    captured = true;
                    session_for_poll.set_cookie(c.clone());
                    let _ = app_for_poll
                        .emit("ehentai://login", serde_json::json!({ "cookie": c }));
                    win.close().ok();
                    break;
                }
            }
        }
        if !captured {
            win_for_poll.close().ok();
        }
    });

    // Destroyed handler: best-effort native capture in case nothing captured yet.
    let session_for_close = session.inner().clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            let app = app_handle.clone();
            let sess = session_for_close.clone();
            if sess.get_cookie().is_some() {
                return;
            }
            tauri::async_runtime::spawn(async move {
                let app_clone = app.clone();
                if let Ok(Some(cookie)) = tauri::async_runtime::spawn(async move {
                    capture_all_cookies(&app_clone).await
                })
                .await
                {
                    if has_ehentai_session(&cookie) {
                        sess.set_cookie(cookie.clone());
                        let _ = app.emit("ehentai://login", serde_json::json!({ "cookie": cookie }));
                    }
                }
            });
        }
    });

    Ok(())
}

#[allow(dead_code)]
async fn ehentai_try_capture(app: &AppHandle, session: &EhentaiSession) -> Option<String> {
    let cookie = capture_all_cookies(app).await?;
    if !has_ehentai_session(&cookie) {
        return None;
    }
    session.set_cookie(cookie.clone());
    Some(cookie)
}

/// Download a single gallery by URL into the local library, emitting progress
/// over the `ehentai://progress` event (same shape as the Pixiv downloader's
/// `pixiv://progress`).
#[tauri::command]
pub async fn ehentai_download_gallery(
    gallery_url: String,
    state: State<'_, LibState>,
    session: State<'_, Arc<EhentaiSession>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let cookie = session.get_cookie().ok_or("not logged in to e-hentai")?;
    let (gid, tok) = EhentaiClient::parse_gallery_url(&gallery_url).map_err(|e| e.to_string())?;

    session.reset_cancel();
    let downloader = EhentaiDownloader::new(&cookie).map_err(|e| e.to_string())?;

    // Fetch the page list up front so we know the total and surface auth issues
    // before spawning the long task.
    let pages = EhentaiClient::new(&cookie, false)
        .map_err(|e| e.to_string())?
        .fetch_gallery_pages(&gid, &tok)
        .await
        .map_err(|e| e.to_string())?;

    let (tx, rx) = mpsc::channel::<PixivProgress>();
    let sink: Arc<Mutex<dyn PixivProgressSink>> = Arc::new(Mutex::new(
        crate::commands::pixiv::ChannelProgressSink { tx },
    ));

    let app_for_relay = app_handle.clone();
    let relay_handle = tokio::spawn(async move {
        while let Ok(event) = rx.recv() {
            let _ = app_for_relay.emit("ehentai://progress", event);
        }
    });

    let library = state.library_service.clone();
    let db = state.db.clone();
    let storage = state.storage.clone();
    let cancel_session = session.inner().clone();
    let url = gallery_url.clone();
    tokio::spawn(async move {
        let gallery_title = url;
        let res = downloader
            .download_gallery(
                &gallery_title,
                "e-hentai gallery",
                &[],
                pages,
                sink,
                &cancel_session.cancelled,
                &library,
                db,
                storage,
            )
            .await;
        if let Err(e) = res {
            tracing::warn!(target: "erolib::ehentai", %e, "gallery download finished with errors");
        }
        relay_handle.abort();
    });

    Ok(())
}

/// Cancel any in-flight gallery download. Takes effect after the current page.
#[tauri::command]
pub async fn ehentai_cancel_download(
    session: State<'_, Arc<EhentaiSession>>,
) -> Result<(), String> {
    session.cancel();
    Ok(())
}

/// Search the e-hentai/exhentai gallery index. `ex` selects the site
/// (exhentai.org vs e-hentai.org); `category` is the path segment (e.g.
/// "doujinshi", "manga") or None for all; `next` is the pagination cursor
/// (None = first page; the gid of the last gallery of the previous page) —
/// e-hentai uses `?next={gid}`, not `?page=N`. 25 results per page.
#[tauri::command]
pub async fn ehentai_search(
    keyword: Option<String>,
    category: Option<String>,
    next: Option<String>,
    ex: bool,
    session: State<'_, Arc<EhentaiSession>>,
) -> Result<Vec<GalleryListItem>, String> {
    let cookie = session.get_cookie().ok_or("not logged in to e-hentai")?;
    let client = EhentaiClient::new(&cookie, ex).map_err(|e| e.to_string())?;
    client
        .fetch_search(keyword.as_deref(), category.as_deref(), next.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Proxy a thumbnail (ehgt.org webp) through the backend so the frontend can
/// render covers. ehgt thumbs need no Referer; the cookie + Referer that
/// [EhentaiClient::download_image] attaches are harmless if present.
#[tauri::command]
pub async fn ehentai_proxy_thumb(
    url: String,
    session: State<'_, Arc<EhentaiSession>>,
) -> Result<Vec<u8>, String> {
    let cookie = session.get_cookie().ok_or("not logged in to e-hentai")?;
    let client = EhentaiClient::new(&cookie, false).map_err(|e| e.to_string())?;
    client.download_image(&url).await.map_err(|e| e.to_string())
}

/// Per-gallery browse status: already in library, currently downloading, or
/// new — so the EHentai grid can render the right card state.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EhentaiBrowseStatus {
    pub gallery_url: String,
    pub local_book_id: Option<String>,
    pub task_id: Option<String>,
    pub task_status: Option<String>,
    pub progress_current: i64,
    pub progress_total: i64,
}

/// Resolve the local state of a batch of gallery URLs in one call:
/// - `local_book_id`: a book already imported from this gallery.
/// - `task_id`/`task_status`/`progress_*`: an active (pending/running/paused)
///   download task for this gallery, if any.
/// A downloaded book takes priority over an in-flight task. Matching is by
/// (gid, token), so the exhentai vs e-hentai host difference between the
/// search base and the stored source_url does not cause false misses.
#[tauri::command]
pub async fn ehentai_browse_status(
    gallery_urls: Vec<String>,
    state: State<'_, LibState>,
) -> Result<Vec<EhentaiBrowseStatus>, String> {
    use crate::services::task::TaskPayload;
    use std::collections::HashMap;
    let pool = &state.db_inner().pool;

    // 1. Local ehentai books keyed by "gid/token" (host-agnostic).
    let mut book_by_key: HashMap<String, String> = HashMap::new();
    let book_rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT id, source_url FROM books WHERE source_plugin IN ('e-hentai','exhentai')",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    for (book_id, source_url) in book_rows {
        if let Some(url) = source_url {
            if let Ok((gid, token)) = EhentaiClient::parse_gallery_url(&url) {
                book_by_key.insert(format!("{gid}/{token}"), book_id);
            }
        }
    }

    // 2. Active ehentai tasks keyed by the payload's gid/token.
    let mut task_by_key: HashMap<String, (String, String, i64, i64)> = HashMap::new();
    let task_rows: Vec<(String, String, i64, i64, String)> = sqlx::query_as(
        "SELECT id, status, progress_current, progress_total, payload FROM tasks \
         WHERE source = 'ehentai' AND status IN ('pending','running','paused')",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    for (id, status, cur, total, payload_str) in task_rows {
        if let Ok(TaskPayload::EhentaiGallery { gid, token, .. }) =
            serde_json::from_str::<TaskPayload>(&payload_str)
        {
            task_by_key
                .entry(format!("{gid}/{token}"))
                .or_insert((id, status, cur, total));
        }
    }

    // 3. Assemble (downloaded > in-flight > new).
    let result = gallery_urls
        .into_iter()
        .map(|url| {
            let key = EhentaiClient::parse_gallery_url(&url)
                .map(|(g, t)| format!("{g}/{t}"))
                .unwrap_or_else(|_| url.clone());
            if let Some(book_id) = book_by_key.get(&key) {
                EhentaiBrowseStatus {
                    gallery_url: url,
                    local_book_id: Some(book_id.clone()),
                    task_id: None,
                    task_status: None,
                    progress_current: 0,
                    progress_total: 0,
                }
            } else if let Some((tid, st, cur, total)) = task_by_key.get(&key) {
                EhentaiBrowseStatus {
                    gallery_url: url,
                    local_book_id: None,
                    task_id: Some(tid.clone()),
                    task_status: Some(st.clone()),
                    progress_current: *cur,
                    progress_total: *total,
                }
            } else {
                EhentaiBrowseStatus {
                    gallery_url: url,
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
