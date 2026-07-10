mod commands;
mod db;
mod errors;
mod models;
mod services;

use std::sync::Arc;

use services::{
    LibraryService, OpdsService, RssService,
    SearchService, StorageService,
};
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
            // One-time migration from the pre-rename identity
            // (com.mangamanager.app + a hardcoded ~/.../manga-manager/ storage
            // dir) into the new im.xrl.erolib data dir. Idempotent.
            migrate_legacy_data(&app_handle);
            let db = Arc::new(tauri::async_runtime::block_on(async {
                db::Database::new(&app_handle).await
            })?);
            let storage_dir = app_handle
                .path()
                .app_local_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let storage = Arc::new(StorageService::new(storage_dir.clone()));
            app.manage(AppState::new(db, storage));
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
            commands::book::export_book,
            commands::book::save_book,
            commands::book::list_books,
            commands::search::search_books,
            commands::search::get_all_tags,
            commands::search::get_all_collections,
            commands::server::start_opds_server,
            commands::server::stop_opds_server,
            commands::server::start_rss_server,
            commands::server::stop_rss_server,
            commands::pixiv::pixiv_test_cookie,
            commands::pixiv::pixiv_get_login,
            commands::pixiv::pixiv_set_login,
            commands::pixiv::pixiv_clear_login,
            commands::pixiv::pixiv_download_bookmarks,
            commands::pixiv::pixiv_cancel_download,
            commands::pixiv::pixiv_fetch_followings,
            commands::pixiv::pixiv_download_user_works,
            commands::pixiv_login::pixiv_open_login_window,
            commands::ehentai::ehentai_open_login_window,
            commands::ehentai::ehentai_download_gallery,
            commands::ehentai::ehentai_cancel_download,
            commands::ehentai::ehentai_get_login,
        ])
        .run(tauri::generate_context!())
        .unwrap();
}

/// Copy data from the pre-rename layout into the new identifier-based data
/// dir, once. Before this project was renamed "erolib" the database lived
/// under com.mangamanager.app/ and storage (library/covers/cache) under a
/// hardcoded `~/Library/Application Support/manga-manager/`. After the rename
/// neither path is where the new identity points, so on first run we move
/// anything we find into the new dir. A marker file makes this run at most
/// once.
fn migrate_legacy_data(app_handle: &tauri::AppHandle) {
    let Ok(new_dir) = app_handle.path().app_local_data_dir() else {
        return;
    };
    let marker = new_dir.join(".migrated_from_legacy");
    if marker.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(&new_dir);

    let legacy_db_dirs: Vec<std::path::PathBuf> = [
        dirs::data_local_dir().map(|d| d.join("com.mangamanager.app")),
        next_to_exe_parent("com.mangamanager.app"),
    ]
    .into_iter()
    .flatten()
    .filter(|d| d.join("manga-manager.db").exists())
    .collect();

    if let Some(src) = legacy_db_dirs.first() {
        if new_dir != *src {
            tracing::info!(
                target: "erolib::db",
                "Migrating legacy DB from {} -> {}",
                src.display(),
                new_dir.display()
            );
            for name in ["manga-manager.db", "manga-manager.db-wal", "manga-manager.db-shm"] {
                let from = src.join(name);
                if from.exists() {
                    let _ = copy_file_replace(&from, &new_dir.join(name));
                }
            }
        }
    }

    let legacy_storage = dirs::data_local_dir().map(|d| d.join("manga-manager"));
    if let Some(src) = legacy_storage.filter(|s| s.is_dir()) {
        for sub in ["library", "covers", "cache"] {
            let from = src.join(sub);
            if from.is_dir() {
                let _ = copy_dir_all(&from, &new_dir.join(sub));
            }
        }
    }

    let _ = std::fs::write(&marker, b"migrated");
}

fn next_to_exe_parent(name: &str) -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join(name)))
}

fn copy_file_replace(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(from, to)?;
    Ok(())
}

fn copy_dir_all(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest = to.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}
