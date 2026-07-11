
use tauri::{Emitter, State};

use crate::errors::AppError;
use crate::models::BookMetadata;
use crate::AppState;

#[tauri::command]
pub async fn import_book(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<crate::models::Book, String> {
    state
        .library_service
        .import_book(file_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_book_from_images(
    images: Vec<Vec<u8>>,
    metadata: BookMetadata,
    state: State<'_, AppState>,
) -> Result<crate::models::Book, String> {
    state
        .library_service
        .import_from_images(images, metadata)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_book(
    id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state
        .library_service
        .delete_book(id.clone())
        .await
        .map_err(|e| e.to_string())?;
    // Broadcast so Pixiv/EHentai browse cards drop their "downloaded" marker
    // (revert to the red-dot not-downloaded state) and the Tasks view hides the
    // now-dangling "Read" button. The service already nulled tasks.book_id, so
    // this just brings the in-memory state in sync.
    let _ = app.emit("book://deleted", serde_json::json!({ "bookId": id }));
    Ok(())
}

#[tauri::command]
pub async fn update_book_metadata(
    id: String,
    metadata: BookMetadata,
    state: State<'_, AppState>,
) -> Result<crate::models::Book, String> {
    state
        .library_service
        .update_metadata(id, metadata)
        .await
        .map_err(|e| e.to_string())
}

/// Metadata for a book (page count, title, file path on disk).
#[tauri::command]
pub async fn get_book(
    id: String,
    state: State<'_, AppState>,
) -> Result<crate::models::Book, String> {
    state
        .library_service
        .get_book(&id)
        .await
        .map_err(|e| e.to_string())
}

/// Return the decoded bytes of a single page from the book's CB7/CBZ archive.
/// `page` is 0-based. Returns the raw image (jpg/png/webp) as a raw binary
/// `ipc::Response` — Tauri ships this to the frontend as an `ArrayBuffer`,
/// avoiding the ~4x serde bloat (and million-long `JSON.parse`) of a `[u8]`
/// number array. The blocking zip read is moved onto a `spawn_blocking` thread
/// so it can't stall the async runtime, and only the lightweight `file_path`
/// lookup (no JOIN/GROUP_CONCAT) hits the DB.
#[tauri::command]
pub async fn get_book_page(
    id: String,
    page: usize,
    state: State<'_, AppState>,
) -> Result<tauri::ipc::Response, String> {
    let file_path = state
        .library_service
        .get_book_file_path(&id)
        .await
        .map_err(|e| e.to_string())?;
    let path = std::path::PathBuf::from(&file_path);
    let storage = state.storage.clone();
    let bytes = tokio::task::spawn_blocking(move || storage.read_page(&path, page))
        .await
        .map_err(|e| format!("page read join failed: {e}"))?
        .ok_or_else(|| format!("page {page} not found for book {id}"))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Total page count for a book, read from the cached archive handle (a cheap
/// lookup into the precomputed image-entry list; the central directory is
/// scanned at most once per session per book). Falls back to the stored DB
/// `page_count` when the file is missing/unreadable.
#[tauri::command]
pub async fn get_book_page_count(
    id: String,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let (file_path, stored_count) = state
        .library_service
        .get_book_file_path_and_count(&id)
        .await
        .map_err(|e| e.to_string())?;
    let path = std::path::PathBuf::from(&file_path);
    let storage = state.storage.clone();
    let count = tokio::task::spawn_blocking(move || storage.count_pages(&path))
        .await
        .map_err(|e| format!("page count join failed: {e}"))?
        .filter(|&c| c > 0)
        .unwrap_or(stored_count.max(0) as usize);
    Ok(count as u32)
}

#[tauri::command]
pub async fn get_book_cover(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, String> {
    state
        .library_service
        .get_cover(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_book_cover_thumb(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, String> {
    state
        .library_service
        .get_cover_thumb(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_book(
    id: String,
    format: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let book = state
        .library_service
        .get_book(&id)
        .await
        .map_err(|e| e.to_string())?;
    if format == book.format {
        return Ok(book.file_path);
    }
    Err(AppError::Other(format!("Export to {} not yet implemented", format)).to_string())
}

/// Copy a book file to a destination chosen by the user (via the save dialog).
/// Looks up the book by id, then duplicates its file to `dest`.
#[tauri::command]
pub async fn save_book(
    id: String,
    dest: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let book = state
        .library_service
        .get_book(&id)
        .await
        .map_err(|e| e.to_string())?;
    let src = std::path::Path::new(&book.file_path);
    if !src.exists() {
        return Err(format!("source file not found: {}", src.display()));
    }
    let dest = std::path::Path::new(&dest).to_path_buf();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }
    std::fs::copy(src, &dest).map_err(|e| format!("copy to {}: {}", dest.display(), e))?;
    Ok(())
}

#[tauri::command]
pub async fn list_books(
    limit: Option<i64>,
    offset: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::Book>, String> {
    state
        .library_service
        .list_books(limit.unwrap_or(100), offset.unwrap_or(0))
        .await
        .map_err(|e| e.to_string())
}
