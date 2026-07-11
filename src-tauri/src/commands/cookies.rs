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
mod native {
    pub fn capture() -> Option<String> {
        None
    }
    pub fn delete_cookies_for(_suffixes: Vec<String>) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// JS eval redirect trick — works for non-HttpOnly cookies on any platform
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
// Public API
// ---------------------------------------------------------------------------

/// Main cookie capture entry point.
///
/// Capture order (most reliable first):
/// 1. **The login window's own WKWebView data store** — guaranteed to be the
///    store the login actually wrote cookies into.  Uses `with_webview` to get
///    the native WKWebView handle, then walks
///    `wkWebView → configuration → websiteDataStore → httpCookieStore`.
/// 2. **defaultDataStore** — fallback for any cookies that landed outside the
///    login window's own webview data store.
/// 3. **JS eval redirect** — non-HttpOnly cookies only (EHentai).
///
/// IMPORTANT: this is meant to be called from a **background tokio task**, NOT
/// the main thread.  The blocking happens in a spawned thread so the main
/// thread's run loop stays free to process any GCD completion callbacks.
pub fn capture_all_cookies(app: &impl Manager<tauri::Wry>) -> Option<String> {
    // Method 1: each known login window's own WKWebView data store.
    for label in &["pixiv-login", "ehentai-login"] {
        if let Some(window) = app.get_webview_window(label) {
            // Fetch the WKWebView pointer on the main thread (with_webview),
            // then run the blocking capture in a background thread so the
            // main run loop can service the completion callback.
            if let Some(wkptr) = get_wkwebview_ptr(&window) {
                // Raw pointers aren't Send; wrap the address as a usize so we
                // can move it into a background thread.  The blocking capture
                // runs there so the main run loop stays free to service the
                // WKHTTPCookieStore completion callback.
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

    // Method 2: shared data stores (background thread).
    #[cfg(target_os = "macos")]
    if let Some(c) = native::capture() {
        return Some(c);
    }

    // Method 3: JS eval fallback (non-HttpOnly cookies — EHentai).
    for label in &["pixiv-login", "ehentai-login"] {
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
        .with_webview(move |webview| {
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            {
                let ptr = webview.inner();
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
