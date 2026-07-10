use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;
use tauri::{AppHandle, Manager, State, Url, WebviewUrl, WindowEvent};

use crate::commands::cookies::capture_all_cookies;
use crate::commands::pixiv::{PixivLogin, PixivSession};

/// Result of a successful in-app Pixiv login.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PixivLoginResult {
    pub user_id: String,
    pub cookie: String,
}

/// Open an embedded browser window pointed at the Pixiv login page. The user
/// logs in there; we poll the window's URL and, the moment it redirects back to
/// a `www.pixiv.net` page (the post-login return_to target is the homepage,
/// whose URL does NOT contain `/users/<id>`), we grab the session cookies
/// (including the HttpOnly `PHPSESSID`) straight from the shared WKWebView
/// cookie store, resolve the account's numeric user id, store the login in
/// `PixivSession`, and close the window.
///
/// We previously required the URL to match `/users/<id>/...` before capturing,
/// which meant the window was never closed early and the user id was left empty.
///
/// The frontend calls this command, then listens for the `pixiv://login` event
/// carrying `{ user_id, cookie }`.
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
    .build()
    .map_err(|e| format!("open login window: {e}"))?;

    let app_for_poll = app_handle.clone();
    let _app_for_navigate = app_handle.clone();
    let session_for_poll = session.inner().clone();
    let win_for_poll = window.clone();
    let win_label = window.label().to_string();

    // We can't read the user id from the post-login URL: after a successful
    // login Pixiv redirects to the homepage (`/`) which has no `/users/<id>`
    // segment. So we drive the already-authenticated webview to
    // `/setting_user.php`, which Pixiv 302-redirects (inside the webview) to
    // `/users/<id>/setting`. Since this uses the webview's own cookie jar rather
    // than re-issuing the request from a fresh reqwest client, the redirect
    // reliably reflects the logged-in account. We read the window URL after the
    // redirect and extract the id. Timeout after ~10 minutes.
    tauri::async_runtime::spawn(async move {
        let mut sent_navigate = false;
        let mut ticks: u32 = 0;
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

            // Phase 1: wait until the user has finished logging in (the
            // accounts.pixiv.net/login page has redirected back onto
            // www.pixiv.net). The landing page is the homepage with no user id.
            if !is_logged_in_pxiv_page(&url) {
                continue;
            }

            // Phase 2: kick the webview over to the settings endpoint if we
            // haven't yet. It 302-redirects to /users/<id>/setting.
            if !sent_navigate {
                if let Ok(target) = Url::parse("https://www.pixiv.net/setting_user.php") {
                    win.navigate(target).ok();
                }
                sent_navigate = true;
                continue;
            }

            // Phase 3: after the redirect, the URL is /users/<id>/setting;
            // capture the cookie and finish. If capture fails (cookies not yet
            // flushed) keep polling briefly on this page.
            if let Some(user_id) = user_id_from_pixiv_url(&url) {
                if user_id.is_empty() {
                    continue;
                }
                if let Some(login) = try_capture(&app_for_poll, &session_for_poll, &user_id).await
                {
                    let _ = app_for_poll.emit("pixiv://login", login);
                    win.close().ok();
                    return;
                }
            }
        }
        // Timed out without ever resolving. Leave the window to its destroy
        // handler for a best-effort capture.
        win_for_poll.close().ok();
    });

    // If the user closes the window, also do a best-effort capture then stop.
    // We may not have a /users/<id> URL to read, so fall back to resolving the
    // id from the cookie via Pixiv's API. Importantly, if the polling task
    // already captured a login and persisted it to the session (and emitted the
    // event) before the window closed, closing the window fires this same
    // Destroyed event — but we must NOT emit a second time with a worse result
    // (an empty user id resolved via the fallback API) and overwrite the good
    // login the user just saw. So only run the fallback when nothing is stored.
    let session_for_close = session.inner().clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            let app = app_handle.clone();
            let sess = session_for_close.clone();
            if sess.get_login().is_some() {
                return;
            }
            tauri::async_runtime::spawn(async move {
                if let Some(login) = try_capture_unknown(&app, &sess).await {
                    let _ = app.emit("pixiv://login", login);
                }
            });
        }
    });

    Ok(())
}

/// Best-effort capture when we have no `/users/<id>` URL (window closed before
/// we could read the redirect). Captures the cookie and resolves the user id
/// from Pixiv's API as a last resort.
async fn try_capture_unknown(
    app: &AppHandle,
    session: &PixivSession,
) -> Option<PixivLoginResult> {
    let cookie = capture_all_cookies(app)?;
    if !has_session_cookie(&cookie) {
        return None;
    }
    let user_id = crate::services::pixiv::PixivClient::fetch_current_user_id(&cookie)
        .await
        .unwrap_or_default();
    let result = PixivLoginResult { user_id, cookie };
    persist(session, &result);
    Some(result)
}

/// True once the embedded window has navigated off the Pixiv login/accounts
/// host back onto a `www.pixiv.net` page — the signal that login succeeded. The
/// `return_to` we pass is the Pixiv homepage, so the landing URL has no
/// `/users/<id>` segment; we can't rely on `user_id_from_pixiv_url` alone.
fn is_logged_in_pxiv_page(url: &Url) -> bool {
    if url.host_str() != Some("www.pixiv.net") {
        return false;
    }
    let path = url.path();
    // Exclude artifacts that are not post-login pages
    !path.contains("/login")
}

/// Extract a Pixiv numeric user id from a profile URL like
/// `https://www.pixiv.net/users/12345678/...`, if present.
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

/// Capture the session cookie from the shared WKWebView cookie store and build
/// a login result for the already-resolved `user_id`. Returns `None` if we
/// couldn't get a usable session cookie yet.
async fn try_capture(
    app: &AppHandle,
    session: &PixivSession,
    user_id: &str,
) -> Option<PixivLoginResult> {
    let cookie = capture_all_cookies(app)?;
    if !has_session_cookie(&cookie) {
        return None;
    }
    let result = PixivLoginResult {
        user_id: user_id.to_string(),
        cookie,
    };
    persist(session, &result);
    Some(result)
}

fn persist(session: &PixivSession, login: &PixivLoginResult) {
    session.set_login(PixivLogin {
        cookie: login.cookie.clone(),
        user_id: login.user_id.clone(),
    });
}

/// True if the cookie string contains a Pixiv `PHPSESSID`.
fn has_session_cookie(cookie: &str) -> bool {
    cookie.split(';').any(|p| {
        let p = p.trim();
        p.starts_with("PHPSESSID=") || p.starts_with("PHPSESSID =")
    })
}
