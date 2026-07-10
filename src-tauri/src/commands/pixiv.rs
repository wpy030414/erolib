use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tauri::{AppHandle, Emitter, State};

use crate::services::pixiv::FollowingUserResp;
use crate::services::{PixivDownloader, PixivProgress, PixivProgressSink};
use crate::AppState as LibState;

/// Persisted Pixiv login (cookie + the account's own user id).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PixivLogin {
    pub cookie: String,
    pub user_id: String,
}

/// Persistence record on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixivSessionFile {
    pub cookie: String,
    pub user_id: String,
    pub saved_at: String,
}

/// Holds the currently-running downloader so we can cancel it, and the active
/// Pixiv login captured from the in-app browser. When `persist_path` is set,
/// logins are written there on set and loaded from it at construction time so a
/// manual re-login is the only way to change credentials.
pub struct PixivSession {
    pub(crate) downloader: Mutex<Option<Arc<PixivDownloader>>>,
    pub(crate) relay: Mutex<Option<JoinHandle<()>>>,
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
            downloader: Mutex::new(None),
            relay: Mutex::new(None),
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
        }
    }
}

/// A progress sink that forwards events over a std channel. The consuming
/// async task drains the receiver and relays each event to the frontend by
/// calling `AppHandle::emit` on the main runtime of the spawned command.
pub struct ChannelProgressSink {
    pub tx: mpsc::Sender<PixivProgress>,
}

impl PixivProgressSink for ChannelProgressSink {
    fn emit(&self, event: PixivProgress) {
        let _ = self.tx.send(event);
    }
}

#[tauri::command]
pub async fn pixiv_download_bookmarks(
    cookie: String,
    user_id: String,
    limit: u64,
    state: State<'_, LibState>,
    session: State<'_, Arc<PixivSession>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    if cookie.trim().is_empty() || user_id.trim().is_empty() {
        return Err("cookie and user_id are required".into());
    }

    // Cancel any in-flight run.
    {
        let mut guard = session.downloader.lock().unwrap();
        if let Some(prev) = guard.take() {
            prev.cancel();
        }
    }
    {
        let mut guard = session.relay.lock().unwrap();
        if let Some(h) = guard.take() {
            h.abort();
        }
    }

    let downloader = Arc::new(
        PixivDownloader::new(&cookie, state.db_inner().clone(), state.storage_inner().clone())
            .map_err(|e| e.to_string())?,
    );
    {
        let mut guard = session.downloader.lock().unwrap();
        *guard = Some(downloader.clone());
    }

    let (tx, rx) = mpsc::channel::<PixivProgress>();
    let sink: Arc<Mutex<dyn PixivProgressSink>> = Arc::new(Mutex::new(ChannelProgressSink { tx }));

    // Relay task: drain the channel and emit via Tauri.
    let app_for_relay = app_handle.clone();
    let relay_handle = tokio::spawn(async move {
        while let Ok(event) = rx.recv() {
            let _ = app_for_relay.emit("pixiv://progress", event);
        }
    });
    {
        let mut guard = session.relay.lock().unwrap();
        *guard = Some(relay_handle);
    }

    let session_clone = session.inner().clone();
    tokio::spawn(async move {
        let _res = downloader
            .run(
                &user_id,
                limit,
                Default::default(),
                "pixiv-bookmark",
                sink,
            )
            .await;
        // Drop the sender so the relay task's rx.recv() returns Err and exits.
        {
            let mut guard = session_clone.downloader.lock().unwrap();
            if let Some(current) = guard.as_ref() {
                if Arc::ptr_eq(current, &downloader) {
                    *guard = None;
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn pixiv_cancel_download(
    session: State<'_, Arc<PixivSession>>,
) -> Result<(), String> {
    let guard = session.downloader.lock().unwrap();
    if let Some(d) = guard.as_ref() {
        d.cancel();
        Ok(())
    } else {
        Err("no active download".into())
    }
}

#[tauri::command]
pub async fn pixiv_test_cookie(cookie: String) -> Result<serde_json::Value, String> {
    if cookie.trim().is_empty() {
        return Err("cookie is empty".into());
    }
    let has_phpsessid = cookie.split(';').any(|p| {
        let p = p.trim();
        p.starts_with("PHPSESSID=") || p.starts_with("PHPSESSID =")
    });
    Ok(serde_json::json!({
        "ok": true,
        "has_phpsessid": has_phpsessid,
        "cookie_length": cookie.len(),
    }))
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
    if cookie.trim().is_empty() || user_id.trim().is_empty() {
        return Err("cookie and user_id are required".into());
    }
    session.set_login(PixivLogin {
        cookie: cookie.trim().to_string(),
        user_id: user_id.trim().to_string(),
    });
    Ok(())
}

/// Forget the stored Pixiv login — both in memory and on disk. The user then
/// re-authenticates via the in-app browser, which writes a fresh session. This
/// is the only path that replaces persisted credentials (stage 3).
#[tauri::command]
pub async fn pixiv_clear_login(
    session: State<'_, Arc<PixivSession>>,
) -> Result<(), String> {
    session.clear_login();
    Ok(())
}

#[tauri::command]
pub async fn pixiv_fetch_followings(
    limit: u64,
    session: State<'_, Arc<PixivSession>>,
) -> Result<Vec<FollowingUserResp>, String> {
    let login = session.get_login().ok_or("not logged in to pixiv")?;
    let client = crate::services::pixiv::PixivClient::new(&login.cookie)
        .map_err(|e| e.to_string())?;
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    client
        .fetch_followings(&login.user_id, limit, &cancelled)
        .await
        .map_err(|e| e.to_string())
}

/// Download the latest works of a specific Pixiv user (e.g. a creator the
/// logged-in account follows). Registered books are tagged
/// `source_plugin = "pixiv-following"` so collections/filters can tell them
/// apart from bookmarks; smart-skip applies identically.
#[tauri::command]
pub async fn pixiv_download_user_works(
    target_user_id: String,
    limit: u64,
    state: State<'_, LibState>,
    session: State<'_, Arc<PixivSession>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    tracing::info!(target_user_id = %target_user_id, limit, "pixiv_download_user_works command invoked");
    let login = session.get_login().ok_or("not logged in to pixiv")?;
    spawn_downloader(
        target_user_id,
        limit,
        login.cookie,
        "pixiv-following",
        state,
        session,
        app_handle,
        /*user_works=*/ true,
    )
    .await
}

/// Shared spawn logic for both download modes. When `user_works` is true we
/// enumerate the target user's own works; otherwise their bookmarks.
async fn spawn_downloader(
    user_id: String,
    limit: u64,
    cookie: String,
    source_plugin: &'static str,
    state: State<'_, LibState>,
    session: State<'_, Arc<PixivSession>>,
    app_handle: AppHandle,
    user_works: bool,
) -> Result<(), String> {
    if user_id.trim().is_empty() {
        return Err("user_id is required".into());
    }

    // Cancel any in-flight run.
    {
        let mut guard = session.downloader.lock().unwrap();
        if let Some(prev) = guard.take() {
            prev.cancel();
        }
    }
    {
        let mut guard = session.relay.lock().unwrap();
        if let Some(h) = guard.take() {
            h.abort();
        }
    }

    let downloader = Arc::new(
        PixivDownloader::new(&cookie, state.db_inner().clone(), state.storage_inner().clone())
            .map_err(|e| e.to_string())?,
    );
    {
        let mut guard = session.downloader.lock().unwrap();
        *guard = Some(downloader.clone());
    }

    let (tx, rx) = mpsc::channel::<PixivProgress>();
    let sink: Arc<Mutex<dyn PixivProgressSink>> = Arc::new(Mutex::new(ChannelProgressSink { tx }));

    let app_for_relay = app_handle.clone();
    let relay_handle = tokio::spawn(async move {
        while let Ok(event) = rx.recv() {
            let _ = app_for_relay.emit("pixiv://progress", event);
        }
    });
    {
        let mut guard = session.relay.lock().unwrap();
        *guard = Some(relay_handle);
    }

    let session_clone = session.inner().clone();
    let plugin = source_plugin.to_string();
    let mode = if user_works { "user_works" } else { "bookmarks" };
    tracing::info!(%user_id, limit, mode, "spawning download task");
    tokio::spawn(async move {
        let res = if user_works {
            downloader
                .download_user_works(&user_id, limit, &plugin, sink)
                .await
        } else {
            downloader
                .run(&user_id, limit, Default::default(), &plugin, sink)
                .await
        };
        let _ = res;
        {
            let mut guard = session_clone.downloader.lock().unwrap();
            if let Some(current) = guard.as_ref() {
                if Arc::ptr_eq(current, &downloader) {
                    *guard = None;
                }
            }
        }
    });

    Ok(())
}
