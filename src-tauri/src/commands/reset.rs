use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use crate::AppState;

/// "Factory reset" from the Settings screen.
///
/// Clears the database tables and wipes on-disk artifacts (cb7 library,
/// covers, cache, login sessions). The db **file itself is kept** — the live
/// `SqlitePool` still references its inode, and on Unix a removed file keeps
/// serving stale rows through open file descriptors until the process exits.
/// Deleting the file then reloading the frontend would therefore still show
/// the old library. Clearing in place keeps the pool valid so the very next
/// `list_books` returns nothing.
#[tauri::command]
pub async fn reset_app_data(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    pixiv_session: State<'_, Arc<crate::commands::pixiv::PixivSession>>,
    ehentai_session: State<'_, Arc<crate::commands::ehentai::EhentaiSession>>,
) -> Result<(), String> {
    // 1. Clear tables in dependency order (children before parents). Deleting
    //    from `books` fires the books_fts triggers too, so the search index is
    //    wiped as well.
    for sql in [
        "DELETE FROM book_tags",
        "DELETE FROM books",
        "DELETE FROM tags",
        "DELETE FROM collection_books",
        "DELETE FROM collections",
        "DELETE FROM tasks",
    ] {
        sqlx::query(sql)
            .execute(&state.db.pool)
            .await
            .map_err(|e| format!("clear table: {e}"))?;
    }
    // Reset autoincrement sequences so any rowids/ids start fresh.
    let _ = sqlx::query("DELETE FROM sqlite_sequence")
        .execute(&state.db.pool)
        .await;

    // 2. Wipe known on-disk data ONLY — cb7 library, covers, cache, and the
    //    two session JSON files. Don't blanket-delete the data dir: that also
    //    removes files the WKWebView/Tauri runtime relies on, which makes the
    //    login window crash the next time it opens. Best-effort (missing = ok).
    let data_dir = app_handle
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("resolve app_local_data_dir: {e}"))?;
    for sub in ["library", "covers", "cache"] {
        let _ = std::fs::remove_dir_all(data_dir.join(sub));
    }
    for f in ["pixiv_session.json", "ehentai_session.json"] {
        let _ = std::fs::remove_file(data_dir.join(f));
    }

    // 3. Clear login state. The session JSON files were wiped in step 2, but
    //    two in-memory caches survive a frontend reload (the backend process
    //    doesn't restart): the PixivSession/EhentaiSession objects (restored
    //    from those files at startup) and the WKWebView shared cookie store
    //    (which keeps the in-app browser "logged in"). Clear both, otherwise
    //    get_login / the login window would still report a stale session
    //    (the "instant login / didn't log out" symptom).
    pixiv_session.clear_login();
    ehentai_session.clear_cookie();
    let _ = tauri::async_runtime::spawn_blocking(|| {
        crate::commands::cookies::clear_section_cookies(&[
            "pixiv.net",
            "e-hentai.org",
            "exhentai.org",
        ])
    })
    .await;

    Ok(())
}
