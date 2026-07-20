// Hide the console window on Windows; keep it in debug builds so
// println! / eprintln! output is still visible during development.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod errors;
mod models;
mod services;

use std::sync::Arc;

use services::{
    task_manager::TaskManager, CollectionService, LibraryService, OpdsService,
    RssService, SearchService, StorageService,
};
use std::time::Duration;

use tauri::Manager;

#[derive(Clone)]
struct AppState {
    library_service: Arc<LibraryService>,
    collection_service: Arc<CollectionService>,
    search_service: Arc<SearchService>,
    opds_service: Arc<OpdsService>,
    rss_service: Arc<RssService>,
    db: Arc<db::Database>,
    storage: Arc<StorageService>,
}

impl AppState {
    fn db_inner(&self) -> Arc<db::Database> {
        self.db.clone()
    }

    fn new(db: Arc<db::Database>, storage: Arc<StorageService>) -> Self {
        let library_service = Arc::new(LibraryService::new(db.clone(), storage.clone()));
        let collection_service = Arc::new(CollectionService::new(db.clone()));
        let search_service = Arc::new(SearchService::new(db.clone()));
        let opds_service = Arc::new(OpdsService::new(db.clone()));
        let rss_service = Arc::new(RssService::new(db.clone()));

        Self {
            library_service,
            collection_service,
            search_service,
            opds_service,
            rss_service,
            db,
            storage,
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            let db = Arc::new(tauri::async_runtime::block_on(async {
                db::Database::new(&app_handle).await
            })?);
            let storage_dir = app_handle
                .path()
                .app_local_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let storage = Arc::new(StorageService::new(storage_dir.clone()));
            let app_state = AppState::new(db.clone(), storage.clone());
            // Migrate collections table — add `position` column if missing from
            // the original schema. Safe to call on every startup.
            if let Err(e) = tauri::async_runtime::block_on(async {
                app_state.collection_service.ensure_position_column().await
            }) {
                tracing::warn!(target: "erolib::setup", %e, "ensure collection position column failed");
            }
            // Neutralize reading sessions left open by a prior force-quit so they
            // render in history but contribute 0 to duration stats. Best-effort:
            // a failure here must not block startup.
            if let Err(e) = tauri::async_runtime::block_on(async {
                app_state.library_service.close_stale_sessions().await
            }) {
                tracing::warn!(target: "erolib::setup", %e, "close stale reading sessions failed");
            }
            app.manage(app_state.clone());
            app.manage(Arc::new(commands::server::ServerHandle::new()));
            // Persist the Pixiv login under the app data dir so it survives
            // restarts. A captured login is restored on launch and re-written on
            // every set; only an explicit re-login (clear + new capture) changes
            // credentials.
            let pixiv_session_path = storage_dir.join("pixiv_session.json");
            app.manage(Arc::new(commands::pixiv::PixivSession::with_persist(
                pixiv_session_path,
            )));
            app.manage(Arc::new(commands::ehentai::EhentaiSession::with_persist(
                storage_dir.join("ehentai_session.json"),
            )));
            // Task manager — must be in an Arc so init_self_ref works.
            let task_manager = Arc::new(
                tauri::async_runtime::block_on(async {
                    TaskManager::new(app_handle.clone(), db.clone(), storage.clone()).await
                })
                .map_err(|e| {
                    tracing::error!(target: "erolib::tasks", %e, "failed to create TaskManager");
                    e
                })?,
            );
            TaskManager::init_self_ref(&task_manager);
            // Mark tasks orphaned by a force-quit (left 'running') as 'paused'
            // so the user can resume them rather than seeing them stuck.
            if let Err(e) = tauri::async_runtime::block_on(task_manager.reconcile_on_startup()) {
                tracing::warn!(target: "erolib::tasks", %e, "startup task reconcile failed");
            }
            app.manage(task_manager);

            // Warm up the WKWebView networking XPC service so the first login
            // window that loads an external URL doesn't stall on process launch.
            // The main window loads local content via Tauri's custom protocol,
            // which never triggers `com.apple.WebKit.Networking` startup. macOS
            // launches this service lazily on the first external navigation, and
            // the 2–5s cold-start delay shows as a white screen. We hide a 1×1
            // webview to absorb that cost early while the user is still browsing
            // the library.
            let warmup_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                // Give the main window a head start before we steal the
                // networking process launch.
                tokio::time::sleep(Duration::from_millis(800)).await;
                let warmup = tauri::WebviewWindowBuilder::new(
                    &warmup_handle,
                    "wkwebview-warmup",
                    tauri::WebviewUrl::External(
                        "https://www.apple.com".parse().unwrap(),
                    ),
                )
                .title("warmup")
                .inner_size(1.0, 1.0)
                .visible(false)
                .build();
                if let Ok(w) = warmup {
                    // Wait long enough for the networking process to launch and
                    // the page to begin loading (typically 1–2s on a cold start).
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    w.close().ok();
                }
                tracing::info!(target: "erolib::setup", "WKWebView networking warmup complete");
            });

            Ok(())
        })
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            commands::book::import_book,
            commands::book::delete_book,
            commands::book::get_book,
            commands::book::get_book_page,
            commands::book::get_book_page_count,
            commands::book::get_book_cover_thumb,
            commands::book::save_book,
            commands::book::save_book_page,
            commands::book::list_books,
            commands::book::open_book,
            commands::book::record_reading,
            commands::book::get_weekly_reading_ms,
            commands::book::list_recent_books,
            commands::sync::sync_to_dir,
            commands::reset::reset_app_data,
            commands::search::search_books,
            commands::search::get_all_tags,
            commands::server::start_opds_server_cmd,
            commands::server::stop_opds_server_cmd,
            commands::server::start_rss_server_cmd,
            commands::server::stop_rss_server_cmd,
            commands::pixiv::pixiv_get_login,
            commands::pixiv::pixiv_set_login,
            commands::pixiv::pixiv_clear_login,
            commands::pixiv::pixiv_list_bookmarks,
            commands::pixiv::pixiv_list_following_feed,
            commands::pixiv::pixiv_list_recommended,
            commands::pixiv::pixiv_search_illusts,
            commands::pixiv::pixiv_proxy_image,
            commands::pixiv::pixiv_browse_status,
            commands::pixiv_login::pixiv_open_login_window,
            commands::ehentai::ehentai_open_login_window,
            commands::ehentai::ehentai_get_login,
            commands::ehentai::ehentai_clear_login,
            commands::ehentai::ehentai_search,
            commands::ehentai::ehentai_proxy_thumb,
            commands::ehentai::ehentai_browse_status,
            commands::ahentai::ahentai_search,
            commands::ahentai::ahentai_proxy_thumb,
            commands::ahentai::ahentai_browse_status,
            commands::nicecat::nicecat_proxy_thumb,
            commands::nicecat::nicecat_browse_status,
            commands::nicecat::nicecat_fetch_api,
            commands::tasks::tasks_list,
            commands::tasks::task_pause,
            commands::tasks::task_resume,
            commands::tasks::task_cancel,
            commands::tasks::task_delete,
            commands::tasks::task_retry,
            commands::tasks::tasks_clear_completed,
            commands::tasks::tasks_retry_all,
            commands::tasks::task_enqueue_ehentai_gallery,
            commands::tasks::task_enqueue_pixiv_work,
            commands::tasks::task_enqueue_ahentai_gallery,
            commands::tasks::task_enqueue_nicecat_gallery,
            commands::collection::list_collections,
            commands::collection::reorder_collections,
            commands::collection::create_collection,
            commands::collection::rename_collection,
            commands::collection::delete_collection,
            commands::collection::add_book_to_collection,
            commands::collection::remove_book_from_collection,
            commands::collection::get_book_collections,
        ])
        .run(tauri::generate_context!())
        .unwrap();
}

