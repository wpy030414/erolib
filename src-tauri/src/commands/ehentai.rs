use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, Url, WebviewUrl, WindowEvent};

use crate::commands::cookies::{capture_all_cookies, has_ehentai_session};
use crate::services::{EhentaiClient, EhentaiDownloader, PixivProgress, PixivProgressSink};
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

/// Open an in-app browser pointed at the e-hentai forums login page. The session
/// cookies (`ipb_member_id` / `ipb_pass_hash`) are then available from the shared
/// WKWebView cookie store. We capture as soon as the window navigates onto a
/// post-login `e-hentai.org` / `exhentai.org` page; if the user closes early we
/// still attempt a best-effort capture.
///
/// The frontend calls this command, then listens for the `ehentai://login`
/// event carrying `{ cookie }`.
#[tauri::command]
pub async fn ehentai_open_login_window(
    app_handle: AppHandle,
    session: State<'_, Arc<EhentaiSession>>,
) -> Result<(), String> {
    let login_url: Url = "https://forums.e-hentai.org/index.php?act=Login"
        .parse()
        .map_err(|e| format!("bad login url: {e}"))?;

    let window = tauri::WebviewWindowBuilder::new(
        &app_handle,
        "ehentai-login",
        WebviewUrl::External(login_url),
    )
    .title("Login to e-hentai")
    .inner_size(560.0, 760.0)
    .center()
    .resizable(true)
    .build()
    .map_err(|e| format!("open login window: {e}"))?;

    let win_label = window.label().to_string();
    let app_for_poll = app_handle.clone();
    let session_for_poll = session.inner().clone();
    let win_for_poll = window.clone();

    // Poll until the window lands on a post-login e-hentai/exhentai page, then
    // capture cookies. Best effort: if the cookies don't look logged in yet we
    // keep polling briefly — forum cookies may take a moment to distribute.
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
            let host = url.host_str().unwrap_or("");
            let on_eh =
                host == "e-hentai.org" || host.ends_with(".e-hentai.org") || host == "exhentai.org"
                    || host.ends_with(".exhentai.org");
            let path = url.path();
            if !on_eh || path.contains("act=Login") {
                continue;
            }
            if let Some(cookie) = ehentai_try_capture(&app_for_poll, &session_for_poll) {
                captured = true;
                let _ = app_for_poll
                    .emit("ehentai://login", serde_json::json!({ "cookie": cookie }));
                win.close().ok();
                break;
            }
        }
        if !captured {
            win_for_poll.close().ok();
        }
    });

    // Destroyed handler: best-effort capture in case nothing captured yet.
    let session_for_close = session.inner().clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            let app = app_handle.clone();
            let sess = session_for_close.clone();
            if sess.get_cookie().is_some() {
                return;
            }
            tauri::async_runtime::spawn(async move {
                if let Some(cookie) = ehentai_try_capture(&app, &sess) {
                    let _ = app.emit("ehentai://login", serde_json::json!({ "cookie": cookie }));
                }
            });
        }
    });

    Ok(())
}

fn ehentai_try_capture(app: &AppHandle, session: &EhentaiSession) -> Option<String> {
    let cookie = capture_all_cookies(app)?;
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
    let pages = EhentaiClient::new(&cookie)
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
