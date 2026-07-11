mod commands;
mod db;
mod errors;
mod models;
mod services;

use std::sync::Arc;

use services::{
    task_manager::TaskManager, LibraryService, OpdsService,
    RssService, SearchService, StorageService,
};
use std::time::Duration;

use tauri::Manager;

#[derive(Clone)]
struct AppState {
    library_service: Arc<LibraryService>,
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
    fn storage_inner(&self) -> Arc<StorageService> {
        self.storage.clone()
    }

    fn new(db: Arc<db::Database>, storage: Arc<StorageService>) -> Self {
        let library_service = Arc::new(LibraryService::new(db.clone(), storage.clone()));
        let search_service = Arc::new(SearchService::new(db.clone()));
        let opds_service = Arc::new(OpdsService::new(db.clone()));
        let rss_service = Arc::new(RssService::new(db.clone()));

        Self {
            library_service,
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
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::book::import_book,
            commands::book::import_book_from_images,
            commands::book::delete_book,
            commands::book::update_book_metadata,
            commands::book::get_book,
            commands::book::get_book_page,
            commands::book::get_book_page_count,
            commands::book::get_book_cover,
            commands::book::get_book_cover_thumb,
            commands::book::export_book,
            commands::book::save_book,
            commands::book::list_books,
            commands::sync::sync_to_dir,
            commands::reset::reset_app_data,
            commands::search::search_books,
            commands::search::get_all_tags,
            commands::search::get_all_collections,
            commands::server::start_opds_server_cmd,
            commands::server::stop_opds_server_cmd,
            commands::server::start_rss_server_cmd,
            commands::server::stop_rss_server_cmd,
            commands::pixiv::pixiv_test_cookie,
            commands::pixiv::pixiv_get_login,
            commands::pixiv::pixiv_set_login,
            commands::pixiv::pixiv_clear_login,
            commands::pixiv::pixiv_download_bookmarks,
            commands::pixiv::pixiv_cancel_download,
            commands::pixiv::pixiv_fetch_followings,
            commands::pixiv::pixiv_download_user_works,
            commands::pixiv::pixiv_list_bookmarks,
            commands::pixiv::pixiv_list_following_feed,
            commands::pixiv::pixiv_list_recommended,
            commands::pixiv::pixiv_search_illusts,
            commands::pixiv::pixiv_proxy_image,
            commands::pixiv::pixiv_browse_status,
            commands::pixiv_login::pixiv_open_login_window,
            commands::ehentai::ehentai_open_login_window,
            commands::ehentai::ehentai_download_gallery,
            commands::ehentai::ehentai_cancel_download,
            commands::ehentai::ehentai_get_login,
            commands::ehentai::ehentai_set_login,
            commands::ehentai::ehentai_clear_login,
            commands::ehentai::ehentai_search,
            commands::ehentai::ehentai_proxy_thumb,
            commands::ehentai::ehentai_browse_status,
            commands::tasks::tasks_list,
            commands::tasks::task_pause,
            commands::tasks::task_resume,
            commands::tasks::task_cancel,
            commands::tasks::task_delete,
            commands::tasks::task_retry,
            commands::tasks::tasks_clear_completed,
            commands::tasks::task_enqueue_pixiv_bookmarks,
            commands::tasks::task_enqueue_pixiv_user_works,
            commands::tasks::task_enqueue_ehentai_gallery,
            commands::tasks::task_enqueue_pixiv_work,
        ])
        .run(tauri::generate_context!())
        .unwrap();
}

