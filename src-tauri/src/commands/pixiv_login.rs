use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;
use tauri::{AppHandle, Manager, State, Url, WebviewUrl, WindowEvent};

use crate::commands::cookies::{capture_all_cookies, has_pixiv_session};
use crate::commands::pixiv::{PixivLogin, PixivSession};
use crate::services::pixiv::PixivClient;

/// Result of a successful in-app Pixiv login.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PixivLoginResult {
    pub user_id: String,
    pub user_name: Option<String>,
    pub cookie: String,
}

/// Open an embedded browser window pointed at the Pixiv login page.
///
/// Polling strategy (simplified):
/// 1. Wait until the webview navigates to any `www.pixiv.net` page that is NOT
///    `/login/*` — this covers 2FA (security keys), password auth, and
///    already-logged-in sessions.
/// 2. Navigate webview to `/setting_user.php` which Pixiv 302-redirects to
///    `/users/<id>/setting`, giving us the numeric user id from the URL.
/// 3. Call `capture_all_cookies()` which on macOS reads the shared
///    WKWebView cookie store natively (→ HttpOnly PHPSESSID!), falling back
///    to JS eval for non-HttpOnly cookies.
/// 4. Persist the login and emit `pixiv://login` so the frontend updates.
#[tauri::command]
pub async fn pixiv_open_login_window(
    app_handle: AppHandle,
    session: State<'_, Arc<PixivSession>>,
) -> Result<(), String> {
    let login_url: Url = "https://accounts.pixiv.net/login?return_to=https%3A%2F%2Fwww.pixiv.net%2F"
        .parse()
        .map_err(|e| format!("bad login url: {e}"))?;

    let window = tauri::WebviewWindowBuilder::new(
        &app_handle,
        "pixiv-login",
        WebviewUrl::External(login_url),
    )
    .title("Login to Pixiv")
    .inner_size(520.0, 760.0)
    .center()
    .resizable(true)
    // Disable spell-check/autocorrect on every input. On macOS 26 WKWebView's
    // auto-correction panel (NSCorrectionPanel) is shown as a sheet child
    // window and hits an NSRemoteView assertion → crash the moment the user
    // types in a field. With spellcheck off WebCore never calls
    // showCorrectionPanel, sidestepping the bug.
    .initialization_script(r#"(function(){function s(){document.querySelectorAll('input,textarea,[contenteditable]').forEach(function(e){e.setAttribute('spellcheck','false');e.setAttribute('autocorrect','off');e.setAttribute('autocomplete','off')})}s();if(document.body){new MutationObserver(s).observe(document.body,{childList:true,subtree:true})}else{document.addEventListener('DOMContentLoaded',s)}})();"#)
    .build()
    .map_err(|e| format!("open login window: {e}"))?;

    let app_for_poll = app_handle.clone();
    let session_for_poll = session.inner().clone();
    let win_for_poll = window.clone();
    let win_label = window.label().to_string();

    tauri::async_runtime::spawn(async move {
        let mut ticks: u32 = 0;

        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            ticks += 1;
            if ticks > 1200 {
                break; // 10 min timeout
            }

            let win = match app_for_poll.get_webview_window(&win_label) {
                Some(w) => w,
                None => return,
            };

            // Wait until the user has landed on www.pixiv.net past the login
            // flow (covers password / 2FA / already-logged-in sessions). We do
            // NOT navigate the window anywhere — cookies are captured directly
            // from the webview's data store and the user id is resolved via the
            // API, so the window stays on whatever page the user landed on
            // (typically the homepage), never the account-settings page.
            let url = match win.url() {
                Ok(u) => u,
                Err(_) => continue,
            };
            let host = url.host_str().unwrap_or("");
            if host != "www.pixiv.net" {
                continue;
            }
            let path = url.path();
            if path.contains("/login") || path.contains("accounts.pixiv.net") {
                continue;
            }

            // Capture cookies directly from the webview store (HttpOnly
            // PHPSESSID included). Retry each tick until the session lands.
            let app_clone = app_for_poll.clone();
            let cap = tauri::async_runtime::spawn_blocking(move || {
                capture_all_cookies(&app_clone)
            })
            .await
            .ok()
            .flatten();
            let cookie = match cap {
                Some(ref c) if has_pixiv_session(c) => c.clone(),
                ref other => {
                    tracing::info!(
                        target: "erolib::pixiv_login",
                        path = %path,
                        captured = other.is_some(),
                        has_session = other.as_deref().map(has_pixiv_session).unwrap_or(false),
                        len = other.as_deref().map(|c| c.len()).unwrap_or(0),
                        "capture not ready; retrying"
                    );
                    continue;
                }
            };

            // Resolve the numeric user id from the cookie via the API (no
            // setting-page navigation). Not ready yet → retry next tick.
            let user_id = match PixivClient::fetch_current_user_id(&cookie).await {
                Ok(id) if !id.is_empty() => id,
                ref e => {
                    tracing::info!(
                        target: "erolib::pixiv_login",
                        error = ?e,
                        "fetch_current_user_id not ready; retrying"
                    );
                    continue;
                }
            };

            // Best-effort display name; failure is non-fatal.
            let user_name = match PixivClient::new(&cookie) {
                Ok(c) => c.fetch_user_name(&user_id).await.ok(),
                Err(_) => None,
            };

            let result = PixivLoginResult {
                user_id,
                user_name,
                cookie,
            };
            persist(&session_for_poll, &result);
            let _ = app_for_poll.emit("pixiv://login", &result);
            win.close().ok();
            return;
        }

        // Timed out — try a last-chance native capture + API-based user id.
        let last = try_capture_fallback(&app_for_poll, &session_for_poll).await;
        if let Some(login) = last {
            let _ = app_for_poll.emit("pixiv://login", &login);
        }
        win_for_poll.close().ok();
    });

    // Destroyed handler
    let session_for_close = session.inner().clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            let app = app_handle.clone();
            let sess = session_for_close.clone();
            if sess.get_login().is_some() {
                return;
            }
            tauri::async_runtime::spawn(async move {
                if let Some(login) = try_capture_fallback(&app, &sess).await {
                    let _ = app.emit("pixiv://login", login);
                }
            });
        }
    });

    Ok(())
}

/// Fallback: try native capture + resolve user ID from the Pixiv API.
async fn try_capture_fallback(
    app: &AppHandle,
    session: &PixivSession,
) -> Option<PixivLoginResult> {
    let app_clone = app.clone();
    let cookie =
        tauri::async_runtime::spawn_blocking(move || capture_all_cookies(&app_clone))
            .await
            .ok()??;
    if !has_pixiv_session(&cookie) {
        return None;
    }
    let user_id = PixivClient::fetch_current_user_id(&cookie)
        .await
        .unwrap_or_default();
    if user_id.is_empty() {
        return None;
    }
    let user_name = match PixivClient::new(&cookie) {
        Ok(c) => c.fetch_user_name(&user_id).await.ok(),
        Err(_) => None,
    };
    let result = PixivLoginResult { user_id, user_name, cookie };
    persist(session, &result);
    Some(result)
}

/// Extract a Pixiv numeric user id from a profile URL like
/// `https://www.pixiv.net/users/12345678/...`, if present.
#[allow(dead_code)]
fn user_id_from_pixiv_url(url: &Url) -> Option<String> {
    if url.host_str() != Some("www.pixiv.net") {
        return None;
    }
    let segments: Vec<&str> = url.path_segments()?.collect();
    if segments.first() == Some(&"users") {
        let id = segments.get(1)?;
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            return Some(id.to_string());
        }
    }
    None
}

fn persist(session: &PixivSession, login: &PixivLoginResult) {
    session.set_login(PixivLogin {
        cookie: login.cookie.clone(),
        user_id: login.user_id.clone(),
        user_name: login.user_name.clone(),
    });
}
