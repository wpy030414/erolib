/// Cookie capture for the in-app browser login flows.
///
/// # macOS native capture (for HttpOnly cookies like Pixiv PHPSESSID)
///
/// We use **raw `objc_msgSend` / `objc_getClass` FFI** to talk to
/// `WKHTTPCookieStore.getAllCookies:` — no objc2 type
/// requirements, no trait constraints, and no block2 integration issues.
///
/// The completion block is built manually as a `Block_literal` with the real
/// `_NSConcreteStackBlock` isa and `flags = 0` (no copy/dispose, no captures).
/// We use `defaultDataStore` — the singleton that both the main window and
/// login windows share (unlike `nonPersistentDataStore` which would trigger
/// an `allDataStores` assertion off the main thread).
///
/// # JS eval fallback (for non-HttpOnly cookies like EHentai)
///
/// Inject `location.href = 'about:blank#' + encodeURIComponent(document.cookie)`
/// into the webview, then read the URL fragment from the polling loop.

use tauri::Manager;

pub mod adapter;

// ---------------------------------------------------------------------------
// macOS native cookie capture — raw libc/objc FFI, no objc2 dependency
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod native {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    // ObjC runtime functions / symbols (linked by libobjc on macOS).
    extern "C" {
        fn objc_getClass(name: *const i8) -> *mut std::ffi::c_void;
        fn objc_msgSend();
        fn sel_registerName(name: *const i8) -> *mut std::ffi::c_void;
        // The real Objective-C class object backing stack blocks.
        static _NSConcreteStackBlock: [u8; 0];
    }

    static CAPTURED: Mutex<Option<String>> = Mutex::new(None);
    static DONE: AtomicBool = AtomicBool::new(false);
    /// The httpCookieStore pointer the delete handler deletes from (set before
    /// `getAllCookies:` fires, since the completion block can't capture state).
    static STORE_PTR: AtomicUsize = AtomicUsize::new(0);
    /// Domain suffixes (lowercased, no leading dot) to match for deletion.
    static DELETE_DOMAINS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Minimal block descriptor (no copy/dispose, no signature).
    /// The runtime only needs `size` to memcpy the block on `_Block_copy`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct BlockDescriptor {
        reserved: usize,
        size: usize,
    }

    /// Apple's Block_literal layout (see Block_private.h).
    ///   isa        → &_NSConcreteStackBlock
    ///   flags      → 0 (no captures needing copy/dispose)
    ///   invoke     → fn pointer called as invoke(block_self, args...)
    ///   descriptor → &BlockDescriptor
    #[repr(C)]
    struct BlockLiteral {
        isa: *mut std::ffi::c_void,
        flags: i32,
        reserved: i32,
        invoke: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void),
        descriptor: *const BlockDescriptor,
    }

    static BLOCK_DESCRIPTOR: BlockDescriptor = BlockDescriptor {
        reserved: 0,
        size: std::mem::size_of::<BlockLiteral>(),
    };

    /// The block's invoke function: `void (^)(NSArray<NSHTTPCookie *> *)`.
    /// First arg is the block itself, second is the cookies array.
    unsafe extern "C" fn handler_invoke(
        _block: *mut std::ffi::c_void,
        cookies: *mut std::ffi::c_void,
    ) {
        if cookies.is_null() {
            DONE.store(true, Ordering::SeqCst);
            return;
        }
        let count_sel = sel_registerName(c"count".as_ptr());
        let obj_at_idx_sel = sel_registerName(c"objectAtIndex:".as_ptr());
        let name_sel = sel_registerName(c"name".as_ptr());
        let value_sel = sel_registerName(c"value".as_ptr());
        let utf8_sel = sel_registerName(c"UTF8String".as_ptr());

        type Send0RetISize =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> isize;
        type Send1RetPtr = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            isize,
        ) -> *mut std::ffi::c_void;
        type SendRetPtr =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;

        let count: isize = {
            let f: Send0RetISize = std::mem::transmute(objc_msgSend as *const ());
            f(cookies, count_sel)
        };

        let mut pairs: Vec<String> = Vec::new();
        for i in 0..count {
            let cookie = {
                let f: Send1RetPtr = std::mem::transmute(objc_msgSend as *const ());
                f(cookies, obj_at_idx_sel, i)
            };
            let name_ptr = {
                let f: SendRetPtr = std::mem::transmute(objc_msgSend as *const ());
                f(cookie, name_sel)
            };
            let value_ptr = {
                let f: SendRetPtr = std::mem::transmute(objc_msgSend as *const ());
                f(cookie, value_sel)
            };
            let n = nsstring_utf8(name_ptr, utf8_sel);
            let v = nsstring_utf8(value_ptr, utf8_sel);
            if let (Some(n), Some(v)) = (n, v) {
                if !n.is_empty() {
                    pairs.push(format!("{n}={v}"));
                }
            }
        }
        let result = if pairs.is_empty() {
            String::new()
        } else {
            pairs.join("; ")
        };
        *CAPTURED.lock().unwrap() = Some(result);
        DONE.store(true, Ordering::SeqCst);
    }

    unsafe fn nsstring_utf8(
        s: *mut std::ffi::c_void,
        utf8_sel: *mut std::ffi::c_void,
    ) -> Option<String> {
        if s.is_null() {
            return None;
        }
        type SendRetPtr =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        let f: SendRetPtr = std::mem::transmute(objc_msgSend as *const ());
        let utf8_ptr = f(s, utf8_sel) as *const i8;
        if utf8_ptr.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr(utf8_ptr)
            .to_str()
            .ok()
            .map(|s| s.to_string())
    }

    /// Reset the shared completion state.
    fn reset_state() {
        *CAPTURED.lock().unwrap() = None;
        DONE.store(false, Ordering::SeqCst);
    }

    /// Fetch all cookies from the given `WKHTTPCookieStore *`.  Blocks the
    /// calling thread (spin-wait) until the completion handler fires or 2.5s
    /// elapses.  The handler runs on WKWebView's internal dispatch queue, so
    /// blocking here is safe — it does NOT need the main run loop pumped
    /// (completions are GCD-dispatched, not run-loop-scheduled).
    unsafe fn fetch_from_store(cookie_store: *mut std::ffi::c_void) -> Option<String> {
        if cookie_store.is_null() {
            return None;
        }

        let block = BlockLiteral {
            isa: std::ptr::addr_of!(_NSConcreteStackBlock) as *mut std::ffi::c_void,
            flags: 0,
            reserved: 0,
            invoke: handler_invoke,
            descriptor: &BLOCK_DESCRIPTOR,
        };

        let get_all_sel = sel_registerName(c"getAllCookies:".as_ptr());
        type SendBlock = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *const BlockLiteral,
        );
        let f: SendBlock = std::mem::transmute(objc_msgSend as *const ());
        f(cookie_store, get_all_sel, &block);

        for _ in 0..250 {
            if DONE.load(Ordering::SeqCst) {
                return CAPTURED.lock().unwrap().clone();
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        tracing::warn!(target: "erolib::cookies", "native capture timed out");
        None
    }

    /// Walk a WKWebView pointer to its WKHTTPCookieStore:
    ///   wkWebView → configuration → websiteDataStore → httpCookieStore
    unsafe fn store_from_webview(wkwebview: *mut std::ffi::c_void) -> *mut std::ffi::c_void {
        if wkwebview.is_null() {
            return std::ptr::null_mut();
        }
        type SendRetPtr =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;

        let config_sel = sel_registerName(c"configuration".as_ptr());
        let f: SendRetPtr = std::mem::transmute(objc_msgSend as *const ());
        let config = f(wkwebview, config_sel);
        if config.is_null() {
            return std::ptr::null_mut();
        }

        let store_sel = sel_registerName(c"websiteDataStore".as_ptr());
        let data_store = f(config, store_sel);
        if data_store.is_null() {
            return std::ptr::null_mut();
        }

        let http_sel = sel_registerName(c"httpCookieStore".as_ptr());
        f(data_store, http_sel)
    }

    /// Capture cookies from the cookie store backing the given WKWebView.
    pub fn capture_from_webview(wkwebview: *mut std::ffi::c_void) -> Option<String> {
        reset_state();
        unsafe {
            let store = store_from_webview(wkwebview);
            fetch_from_store(store)
        }
    }

    /// No-op completion block for `deleteCookie:completionHandler:` (1-arg:
    /// block self only). Transmuted to the 2-arg invoke field type at the call
    /// site — same pointer size, the runtime only calls it with the block ptr.
    unsafe extern "C" fn noop_completion_invoke(_block: *mut std::ffi::c_void) {}

    /// Completion handler for the delete pass: enumerate every cookie the store
    /// returns, and `deleteCookie:completionHandler:` those whose domain matches
    /// one of `DELETE_DOMAINS` on the store held in `STORE_PTR`. No captures
    /// (flags=0). Note: WKHTTPCookieStore has NO `deleteCookie:` — it is always
    /// `deleteCookie:completionHandler:`, with a required completion block.
    unsafe extern "C" fn delete_handler_invoke(
        _block: *mut std::ffi::c_void,
        cookies: *mut std::ffi::c_void,
    ) {
        let store = STORE_PTR.load(Ordering::Acquire) as *mut std::ffi::c_void;
        if cookies.is_null() || store.is_null() {
            DONE.store(true, Ordering::SeqCst);
            return;
        }
        let count_sel = sel_registerName(c"count".as_ptr());
        let obj_at_idx_sel = sel_registerName(c"objectAtIndex:".as_ptr());
        let domain_sel = sel_registerName(c"domain".as_ptr());
        let utf8_sel = sel_registerName(c"UTF8String".as_ptr());
        let delete_sel = sel_registerName(c"deleteCookie:completionHandler:".as_ptr());

        type Send0RetISize =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> isize;
        type Send1RetPtr = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            isize,
        ) -> *mut std::ffi::c_void;
        type SendRetPtr =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        type SendDelete = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *const BlockLiteral,
        );

        // Reusable no-op completion block (deleteCookie:completionHandler:).
        let completion_block = BlockLiteral {
            isa: std::ptr::addr_of!(_NSConcreteStackBlock) as *mut std::ffi::c_void,
            flags: 0,
            reserved: 0,
            invoke: std::mem::transmute::<
                unsafe extern "C" fn(*mut std::ffi::c_void),
                unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void),
            >(noop_completion_invoke),
            descriptor: &BLOCK_DESCRIPTOR,
        };

        let count: isize = {
            let f: Send0RetISize = std::mem::transmute(objc_msgSend as *const ());
            f(cookies, count_sel)
        };
        let suffixes = DELETE_DOMAINS.lock().unwrap();
        let mut deleted = 0usize;
        for i in 0..count {
            let cookie = {
                let f: Send1RetPtr = std::mem::transmute(objc_msgSend as *const ());
                f(cookies, obj_at_idx_sel, i)
            };
            let domain_ptr = {
                let f: SendRetPtr = std::mem::transmute(objc_msgSend as *const ());
                f(cookie, domain_sel)
            };
            let dom_raw = nsstring_utf8(domain_ptr, utf8_sel).unwrap_or_default();
            let dom = dom_raw.trim_start_matches('.').to_ascii_lowercase();
            let matches = suffixes
                .iter()
                .any(|s| dom == s.as_str() || dom.ends_with(&format!(".{s}")));
            if matches {
                let f: SendDelete = std::mem::transmute(objc_msgSend as *const ());
                f(store, delete_sel, cookie, &completion_block);
                deleted += 1;
            }
        }
        tracing::info!(target: "erolib::cookies", deleted, "deleted matching cookies");
        DONE.store(true, Ordering::SeqCst);
    }

    /// Delete every cookie whose domain matches one of `suffixes` (lowercased,
    /// no leading dot) from the shared `defaultDataStore`. Blocks the calling
    /// thread until the `getAllCookies:` completion fires or 2.5s elapse. The
    /// actual `deleteCookie:` calls are fire-and-forget on the store's queue.
    pub fn delete_cookies_for(suffixes: Vec<String>) -> bool {
        {
            let mut guard = DELETE_DOMAINS.lock().unwrap();
            guard.clear();
            guard.extend(suffixes.into_iter().map(|s| s.to_ascii_lowercase()));
        }
        reset_state();
        unsafe {
            let cls = objc_getClass(c"WKWebsiteDataStore".as_ptr());
            if cls.is_null() {
                return false;
            }
            type SendRetPtr =
                unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
            let f: SendRetPtr = std::mem::transmute(objc_msgSend as *const ());
            let default_sel = sel_registerName(c"defaultDataStore".as_ptr());
            let store = f(cls, default_sel);
            if store.is_null() {
                return false;
            }
            let http_sel = sel_registerName(c"httpCookieStore".as_ptr());
            let cookie_store = f(store, http_sel);
            if cookie_store.is_null() {
                return false;
            }
            // deleteCookie: lives on WKHTTPCookieStore, NOT WKWebsiteDataStore —
            // storing the dataStore here crashes with an unrecognized selector.
            STORE_PTR.store(cookie_store as usize, Ordering::Release);

            let block = BlockLiteral {
                isa: std::ptr::addr_of!(_NSConcreteStackBlock) as *mut std::ffi::c_void,
                flags: 0,
                reserved: 0,
                invoke: delete_handler_invoke,
                descriptor: &BLOCK_DESCRIPTOR,
            };
            let get_all_sel = sel_registerName(c"getAllCookies:".as_ptr());
            type SendBlock = unsafe extern "C" fn(
                *mut std::ffi::c_void,
                *mut std::ffi::c_void,
                *const BlockLiteral,
            );
            let f: SendBlock = std::mem::transmute(objc_msgSend as *const ());
            f(cookie_store, get_all_sel, &block);

            for _ in 0..250 {
                if DONE.load(Ordering::SeqCst) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            tracing::warn!(target: "erolib::cookies", "native delete timed out");
            false
        }
    }

    /// Capture cookies from the shared `defaultDataStore` (WKWebView's singleton
    /// cookie store that both login windows and the main window share).
    /// Inlines `objc_msgSend(cls, "defaultDataStore")` directly without a
    /// main-thread hop — the singleton accessor does not trigger the
    /// `allDataStores` assertion that `nonPersistentDataStore` does.
    pub fn capture() -> Option<String> {
        reset_state();
        unsafe {
            let cls = objc_getClass(c"WKWebsiteDataStore".as_ptr());
            if cls.is_null() {
                return None;
            }
            type SendRetPtr =
                unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
            let f: SendRetPtr = std::mem::transmute(objc_msgSend as *const ());
            let default_sel = sel_registerName(c"defaultDataStore".as_ptr());
            let store = f(cls, default_sel);
            if store.is_null() {
                return None;
            }
            let http_sel = sel_registerName(c"httpCookieStore".as_ptr());
            let cookie_store = f(store, http_sel);
            if cookie_store.is_null() {
                return None;
            }
            if let Some(c) = fetch_from_store(cookie_store) {
                if !c.trim().is_empty() {
                    tracing::info!(
                        target: "erolib::cookies",
                        len = c.len(),
                        "captured cookies via native WKHTTPCookieStore"
                    );
                    return Some(c);
                }
            }
        }
        None
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)] // ponytail: stub for non-macOS targets; macOS path lives above.
mod native {
    pub fn capture() -> Option<String> {
        None
    }
    pub fn delete_cookies_for(_suffixes: Vec<String>) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Windows native cookie capture — READ-ONLY sqlite on the WebView2 cookie DB.
//
// On Chromium 124+ WebView2 enforces App-Bound Encryption which stores every
// session cookie as a blob in `encrypted_value` (plaintext `value` is empty
// for sensitive cookies like PHPSESSID). We cannot decrypt the blob in-process
// without an elevated Edge handshake, so this path returns only the
// non-encrypted cookies — enough for the eHentai session (ipb_* are NOT
// HttpOnly) but NOT enough for Pixiv's HttpOnly PHPSESSID. The Pixiv path
// now relies on a manual-paste hint in the UI; see `PixivDownload.vue`
// `manualPasteHint` and `tests/bdd/MIGRATION.md`.
//
// An ICoreWebView2_2.CookieManager.GetCookies COM path was prototyped in
// this module's earlier `native_windows_com` block but caused the WebView2
// host process to deadlock (`wait_for_async_operation` blocks the main
// thread that the COM completion must dispatch on). Reverted to keep the
// app responsive. ponytail: a working manual paste ships today; revisit
// COM via a dedicated async worker (not `with_webview`) when needed.
// ---------------------------------------------------------------------------
//
// WebView2 stores its cookie database at:
//   <user_data>/Default/Network/Cookies
// The user_data folder defaults to `<APPLOCALDATA>/EBWebView` for Tauri 2.x
// apps (and may be overridden via `WebviewWindowBuilder::data_directory`).
// The Cookies file is an unencrypted SQLite database (Chromium-derived
// schema), with HttpOnly cookies stored in plaintext alongside non-HttpOnly
// ones — `is_httponly` is just a flag in the `cookies` table, so JS-eval
// blind spots are gone as soon as we read from here directly.
//
// rusqlite gives us a synchronous read; the SELECT is fast (<5ms) and we
// already hold the only writer (the WebView2 process) as a sibling, so a
// plain `Connection::open` with a busy-timeout is enough.
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod native_windows {
    use std::path::{Path, PathBuf};

    use rusqlite::{Connection, OpenFlags};

    /// Tauri 2.x's default WebView2 user data folder name (under
    /// `<app_local_data_dir>/EBWebView`). Confirmed in tauri-wry 2.11.
    const EBWEBVIEW_DIR: &str = "EBWebView";

    /// Path of the WebView2 cookie SQLite for the given app data dir.
    fn cookie_db_path(app_local_data: &Path) -> Option<PathBuf> {
        let p = app_local_data
            .join(EBWEBVIEW_DIR)
            .join("Default")
            .join("Network")
            .join("Cookies");
        p.exists().then_some(p)
    }

    /// Read every cookie whose `host_key` matches one of `host_suffixes`
    /// (e.g. "pixiv.net", "e-hentai.org"). Returns the cookies joined in
    /// `name=value; name=value` form (the format PixivClient / EhentaiClient
    /// already expect — see `commands::pixiv::PixivLogin.cookie`).
    pub fn capture_from_db(app_local_data: &Path, host_suffixes: &[&str]) -> Option<String> {
        let db_path = cookie_db_path(app_local_data)?;
        let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        conn.busy_timeout(std::time::Duration::from_secs(2)).ok()?;

        if host_suffixes.is_empty() {
            return None;
        }
        // Chromium's host_key is normalised (lowercase, may carry a leading
        // dot for domain cookies). Match both forms per suffix, with one
        // distinct positional placeholder per suffix so each binds its own
        // value (SQLite's `?1` aliasing would otherwise collapse them).
        let conds: Vec<String> = host_suffixes
            .iter()
            .enumerate()
            .map(|(i, _)| format!("(host_key = ?{n} OR host_key = '.' || ?{n})", n = i + 1))
            .collect();
        let sql = format!(
            "SELECT name, value FROM cookies \
             WHERE {} \
             ORDER BY host_key, name",
            conds.join(" OR "),
        );
        let mut stmt = conn.prepare(&sql).ok()?;
        let binds: Vec<&dyn rusqlite::ToSql> = host_suffixes
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();
        let mut rows = stmt.query(rusqlite::params_from_iter(binds)).ok()?;

        let mut pairs: Vec<String> = Vec::new();
        while let Some(row) = rows.next().ok()? {
            let name: String = row.get(0).ok()?;
            let value: String = row.get(1).ok()?;
            // Skip empty values: when App-Bound Encryption is on, WebView2
            // stores the real cookie in `encrypted_value` (BLOB) and leaves
            // `value` empty. Surfacing `PHPSESSID=` (no value) downstream
            // would pass `has_pixiv_session` (prefix-only check) and yield a
            // bogus empty user_id — see the
            // `capture_returns_none_when_value_empty_encrypted` BDD scenario.
            if !name.is_empty() && !value.is_empty() {
                pairs.push(format!("{name}={value}"));
            }
        }
        if pairs.is_empty() {
            return None;
        }
        let out = pairs.join("; ");
        tracing::info!(
            target: "erolib::cookies",
            len = out.len(),
            count = pairs.len(),
            "captured cookies via WebView2 SQLite"
        );
        Some(out)
    }

    /// Delete every cookie whose `host_key` matches `host_suffixes` from the
    /// WebView2 cookie SQLite. No-op if the DB is absent.
    #[allow(dead_code)] // wired up when clear_section_cookies takes &AppHandle.
    pub fn delete_cookies_for_db(app_local_data: &Path, host_suffixes: &[&str]) -> bool {
        let Some(db_path) = cookie_db_path(app_local_data) else {
            return true;
        };
        if host_suffixes.is_empty() {
            return true;
        }
        let Ok(conn) = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        else {
            return false;
        };
        let _ = conn.busy_timeout(std::time::Duration::from_secs(2));
        let conds: Vec<String> = host_suffixes
            .iter()
            .enumerate()
            .map(|(i, _)| format!("(host_key = ?{n} OR host_key = '.' || ?{n})", n = i + 1))
            .collect();
        let sql = format!("DELETE FROM cookies WHERE {}", conds.join(" OR "));
        let Ok(mut stmt) = conn.prepare(&sql) else {
            return false;
        };
        let binds: Vec<&dyn rusqlite::ToSql> = host_suffixes
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();
        stmt.execute(rusqlite::params_from_iter(binds)).is_ok()
    }
}

// ---------------------------------------------------------------------------
// JS eval redirect trick — works for non-HttpOnly cookies on any platform
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------

pub fn inject_cookie_redirect(window: &tauri::WebviewWindow) -> bool {
    window
        .eval(
            r#"document.location.href = 'about:blank#' + encodeURIComponent(document.cookie);"#,
        )
        .is_ok()
}

pub fn extract_cookie_from_url(window: &tauri::WebviewWindow) -> Option<String> {
    let url = window.url().ok()?;
    if url.scheme() != "about" || url.path() != "blank" {
        return None;
    }
    let frag = url.fragment()?;
    urlencoding::decode(frag).ok().map(|s| s.trim().to_string())
}

// ---------------------------------------------------------------------------
// Windows native cookie capture — async COM path (ICoreWebView2_2.CookieManager)
//
// Why async: the synchronous `wait_for_async_operation` API blocks the calling
// thread until WebView2's completion callback fires. WebView2 dispatches its
// COM callbacks on the same thread that called `GetCookies` — which for us is
// the WebView2 main thread. Blocking it deadlocks the WebView2 message pump
// and freezes the whole app.
//
// The async path: call `GetCookies` from a `tauri::async_runtime::spawn_blocking`
// thread (Tauri's blocking pool, NOT the WebView2 main thread), route the
// completion result through a `tokio::sync::oneshot` channel, and `await` the
// channel receiver on the caller side. The oneshot receiver `.await`s the
// result without ever blocking the main thread.
//
// CURRENTLY DISABLED — see the doc comment at the original Method 2c block.
// The async COM GetCookies path requires `webview2-com` + `windows-core`
// direct deps, but those conflict with the `windows` crate's transitive
// 0.61 pin and break the macOS / Linux CI build. Until that conflict is
// resolved (windows-rs version bump, repo-wide `[patch.crates-io]`, or
// migrating the entire project to windows-core 0.62), Method 2b (SQLite
// plaintext) + Method 3 (JS eval) are the only cookie capture paths.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Main cookie capture entry point.
///
/// Capture order (most reliable first):
/// 1. **The login window's own WKHtt

pub async fn capture_all_cookies(app: &impl Manager<tauri::Wry>) -> Option<String> {
    use adapter::ALL_ADAPTERS;

    // Iterate every registered adapter; first match wins. The order is
    // significant only when the same cookie string satisfies multiple
    // services (e.g. a wildcard cookie on a shared CDN), which doesn't
    // happen in practice.
    for &adapter in ALL_ADAPTERS {
        let label = adapter.window_label();

        // Method 1: the login window's own WKWebView data store (macOS).
        if let Some(window) = app.get_webview_window(label) {
            if let Some(wkptr) = get_wkwebview_ptr(&window) {
                let addr = wkptr as usize;
                #[cfg(target_os = "macos")]
                {
                    let handle = std::thread::spawn(move || {
                        native::capture_from_webview(addr as *mut std::ffi::c_void)
                    });
                    if let Ok(Some(c)) = handle.join() {
                        if !c.trim().is_empty() {
                            tracing::info!(
                                target: "erolib::cookies",
                                service = label,
                                len = c.len(),
                                "captured cookies via webview dataStore"
                            );
                            return Some(c);
                        }
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = addr;
                }
            }
        }
    }

    // Method 2: shared data stores (macOS).
    #[cfg(target_os = "macos")]
    if let Some(c) = native::capture() {
        return Some(c);
    }

    // Method 2b: Windows WebView2 cookie SQLite.
    #[cfg(target_os = "windows")]
    {
        if let Ok(root) = app.path().app_local_data_dir() {
            // Union of every adapter's host suffixes, deduplicated.
            let mut suffixes: Vec<&str> = Vec::new();
            for &a in ALL_ADAPTERS {
                for s in a.cookie_host_suffixes() {
                    if !suffixes.contains(s) {
                        suffixes.push(*s);
                    }
                }
            }
            for sub in ["EBWebView", "EBWebView-login-pixiv", "EBWebView-login-ehentai"] {
                let dir = root.join(sub);
                if let Some(c) = native_windows::capture_from_db(&dir, &suffixes) {
                    if !c.trim().is_empty() {
                        return Some(c);
                    }
                }
            }
        }
    }


    // Method 3: JS eval fallback (non-HttpOnly cookies — both services).
    for &adapter in ALL_ADAPTERS {
        let label = adapter.window_label();
        if let Some(window) = app.get_webview_window(label) {
            let _ = inject_cookie_redirect(&window);
            std::thread::sleep(std::time::Duration::from_millis(300));
            if let Some(c) = extract_cookie_from_url(&window) {
                if !c.is_empty() {
                    return Some(c);
                }
            }
        }
    }
    None
}

/// Delete cookies whose domain matches one of `host_suffixes` from the shared
/// WKWebView data store (macOS). This wipes the in-app browser's "remember me"
/// state for the given site without touching the main window's localStorage or
/// the *other* source's cookies. No-op (returns `true`) on non-macOS.
pub fn clear_section_cookies(host_suffixes: &[&str]) -> bool {
    #[cfg(target_os = "macos")]
    {
        native::delete_cookies_for(host_suffixes.iter().map(|s| (*s).to_string()).collect())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = host_suffixes;
        true
    }
}

/// Run `with_webview` synchronously on the main thread and capture the native
/// WKWebView pointer it exposes.  Returns `None` on platforms that don't
/// expose a raw handle (or if the window isn't ready).
fn get_wkwebview_ptr(window: &tauri::WebviewWindow) -> Option<*mut std::ffi::c_void> {
    // Raw pointers aren't `Send`; pass the address through as a `usize`.
    let (tx, rx) = std::sync::mpsc::channel::<Option<usize>>();
    window
        .with_webview(move |_webview| {
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            {
                let ptr = _webview.inner();
                let addr = ptr as usize;
                let _ = tx.send(Some(addr));
            }
            #[cfg(not(any(target_os = "macos", target_os = "ios")))]
            {
                let _ = tx.send(None);
            }
        })
        .ok()?;
    rx.recv()
        .ok()
        .flatten()
        .map(|addr| addr as *mut std::ffi::c_void)
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

pub fn has_pixiv_session(cookie: &str) -> bool {
    cookie.split(';').any(|p| {
        let p = p.trim();
        p.starts_with("PHPSESSID=")
    })
}

pub fn has_ehentai_session(cookie: &str) -> bool {
    let has = |name: &str| {
        cookie
            .split(';')
            .any(|p| p.trim_start().starts_with(&format!("{name}=")))
    };
    has("ipb_member_id") && has("ipb_pass_hash")
}

// ---------------------------------------------------------------------------
// BDD tests — mirror scenarios in tests/bdd/features/*.feature
// ---------------------------------------------------------------------------

#[cfg(test)]
mod bdd {
    use super::{has_ehentai_session, has_pixiv_session};

    // ---- Pure predicates (no I/O) ----

    #[test]
    fn has_pixiv_session_accepts_phpsessid() {
        // Scenario: A captured cookie string with PHPSESSID parses to a user id
        assert!(has_pixiv_session("PHPSESSID=12345_abc; yuid_b=foo"));
    }

    #[test]
    fn has_pixiv_session_rejects_when_missing() {
        // Scenario: A cookie without PHPSESSID cannot authenticate
        assert!(!has_pixiv_session("yuid_b=foo; p_ab_id=bar"));
    }

    #[test]
    fn has_ehentai_session_requires_both_cookies() {
        // Scenario: EHentai requires both ipb cookies
        assert!(has_ehentai_session(
            "ipb_member_id=42; ipb_pass_hash=deadbeef; igneous=x"
        ));
        assert!(!has_ehentai_session("ipb_member_id=42; igneous=x"));
        assert!(!has_ehentai_session("ipb_pass_hash=deadbeef; igneous=x"));
        assert!(!has_ehentai_session("igneous=x"));
    }

    // ---- WebView2 SQLite capture (Windows-only) ----

    /// Build a minimal WebView2-shaped cookies SQLite at the path that
    /// `native_windows::capture_from_db` expects. Returns the directory to
    /// pass as `app_local_data`.
    #[cfg(target_os = "windows")]
    fn fixture_sqlite(tag: &str, rows: &[(&str, &str, &str, i64, usize)]) -> std::path::PathBuf {
        use rusqlite::Connection;
        let dir = std::env::temp_dir()
            .join(format!("erolib-bdd-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir
            .join("EBWebView")
            .join("Default")
            .join("Network")
            .join("Cookies");
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE cookies(
                host_key TEXT NOT NULL,
                top_frame_site_key TEXT NOT NULL DEFAULT '',
                has_cross_site_ancestor INTEGER NOT NULL DEFAULT 0,
                name TEXT NOT NULL,
                value TEXT NOT NULL,
                encrypted_value BLOB NOT NULL DEFAULT X'',
                path TEXT NOT NULL DEFAULT '/',
                expires_utc INTEGER NOT NULL DEFAULT 0,
                is_secure INTEGER NOT NULL DEFAULT 0,
                is_httponly INTEGER NOT NULL DEFAULT 0,
                last_access_utc INTEGER NOT NULL DEFAULT 0,
                has_expires INTEGER NOT NULL DEFAULT 0,
                is_persistent INTEGER NOT NULL DEFAULT 0,
                priority INTEGER NOT NULL DEFAULT 0,
                samesite INTEGER NOT NULL DEFAULT 0,
                source_scheme INTEGER NOT NULL DEFAULT 0,
                source_port INTEGER NOT NULL DEFAULT 0,
                last_update_utc INTEGER NOT NULL DEFAULT 0,
                source_type INTEGER NOT NULL DEFAULT 0,
                creation_utc INTEGER NOT NULL DEFAULT 0
            );
            CREATE UNIQUE INDEX cookies_unique_index ON cookies(
                host_key, top_frame_site_key, has_cross_site_ancestor, name, path,
                source_scheme, source_port);
            "#,
        )
        .unwrap();
        for (host, name, value, is_httponly, enc_len) in rows {
            let enc: Vec<u8> = (0..*enc_len).map(|_| 0x76u8).collect();
            conn.execute(
                "INSERT INTO cookies(host_key, name, value, encrypted_value, is_httponly) \
                 VALUES (?,?,?,?,?)",
                rusqlite::params![host, name, value, enc, *is_httponly],
            )
            .unwrap();
        }
        dir
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn capture_plaintext_phpsessid_succeeds() {
        // Scenario: A WebView2 SQLite row with plaintext PHPSESSID is captured
        let app = fixture_sqlite("plaintext", &[
            (".pixiv.net", "PHPSESSID", "99999_xyz", 1, 0),
            (".pixiv.net", "yuid_b", "y-b", 0, 0),
            ("www.pixiv.net", "a_type", "1", 0, 0),
        ]);
        let got = super::native_windows::capture_from_db(&app, &["pixiv.net"])
            .expect("capture_from_db returns Some");
        assert!(got.contains("PHPSESSID=99999_xyz"), "got: {got}");
        assert!(got.contains("yuid_b=y-b"), "got: {got}");
        assert!(super::has_pixiv_session(&got));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn capture_excludes_non_matching_hosts() {
        // Scenario: Cookies from non-matching hosts are excluded
        let app = fixture_sqlite("host_filter", &[
            (".example.com", "PHPSESSID", "99999_xyz", 0, 0),
            (".pixiv.net", "PHPSESSID", "11111_abc", 0, 0),
        ]);
        let got = super::native_windows::capture_from_db(&app, &["pixiv.net"])
            .expect("capture_from_db returns Some");
        assert!(got.contains("PHPSESSID=11111_abc"), "got: {got}");
        assert!(!got.contains("99999_xyz"), "leaked: {got}");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn capture_returns_none_when_value_empty_encrypted() {
        // Scenario: A WebView2 SQLite row with all-encrypted values cannot be read
        let app = fixture_sqlite("encrypted", &[(".pixiv.net", "PHPSESSID", "", 1, 105)]);
        let got = super::native_windows::capture_from_db(&app, &["pixiv.net"]);
        assert!(got.is_none(), "expected None but got: {got:?}");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn capture_ehentai_returns_ipb_cookies() {
        // Scenario: A cookie SQLite with both ipb cookies is captured
        let app = fixture_sqlite("eh_ipb", &[
            (".e-hentai.org", "ipb_member_id", "42", 0, 0),
            (".e-hentai.org", "ipb_pass_hash", "deadbeef", 0, 0),
            (".exhentai.org", "igneous", "x", 0, 0),
        ]);
        let got = super::native_windows::capture_from_db(
            &app,
            &["e-hentai.org", "exhentai.org"],
        )
        .expect("capture_from_db returns Some");
        assert!(got.contains("ipb_member_id=42"), "got: {got}");
        assert!(got.contains("ipb_pass_hash=deadbeef"), "got: {got}");
        assert!(super::has_ehentai_session(&got), "should pass has_ehentai_session");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn placeholder_for_non_windows() {
        // Pure predicates above cover non-Windows hosts; the SQLite path is
        // Windows-only.
    }
}
