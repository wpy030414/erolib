/// Shared WKWebView cookie capture. Used by both the Pixiv and EHentai in-app
/// login flows to extract session cookies (incl. HttpOnly ones) straight out of
/// the shared WKWebView cookie store on macOS.
///
/// Returns all cookies joined as `name=value; name=value`, or `None` if capture
/// fails or we're not on macOS.
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use std::ptr::NonNull;

/// Read every cookie from the default WKWebsiteDataStore and join them. Spans a
/// sync→main→(ObjC completion)→async handoff via a oneshot channel.
#[cfg(target_os = "macos")]
pub fn capture_all_cookies(app: &AppHandle) -> Option<String> {
    use objc2::MainThreadMarker;
    use objc2_foundation::NSHTTPCookie;
    use objc2_web_kit::WKWebsiteDataStore;

    let main = app.get_webview_window("main")?;

    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    let tx = Arc::new(Mutex::new(std::cell::Cell::new(Some(tx))));

    main.with_webview(move |_platform| {
        let tx = tx.clone();
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let store = unsafe { WKWebsiteDataStore::defaultDataStore(mtm) };
        let cookie_store = unsafe { store.httpCookieStore() };

        let handler = block2::StackBlock::new(
            move |cookies: NonNull<objc2_foundation::NSArray<NSHTTPCookie>>| {
                let cookies = unsafe { cookies.as_ref() };
                let mut parts: Vec<String> = Vec::with_capacity(cookies.count() as usize);
                for i in 0..cookies.count() {
                    let c = cookies.objectAtIndex(i);
                    parts.push(format!("{}={}", c.name(), c.value()));
                }
                if let Some(tx) = tx.lock().unwrap().take() {
                    let _ = tx.send(Some(parts.join("; ")));
                }
            },
        );
        unsafe { cookie_store.getAllCookies(&handler) };
    })
    .ok()?;

    // The ObjC completion fires later on the main run loop; wait briefly.
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            match tokio::time::timeout(Duration::from_secs(5), rx).await {
                Ok(Ok(s)) => s,
                _ => None,
            }
        })
    })
}

#[cfg(not(target_os = "macos"))]
pub fn capture_all_cookies(_app: &AppHandle) -> Option<String> {
    None
}

/// Does a cookie string look like an authenticated e-hentai/exhentai session?
pub fn has_ehentai_session(cookie: &str) -> bool {
    let has = |name: &str| {
        cookie
            .split(';')
            .any(|p| p.trim_start().starts_with(&format!("{name}=")))
    };
    has("ipb_member_id") && has("ipb_pass_hash")
}
