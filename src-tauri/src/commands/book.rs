
use tauri::State;

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
pub async fn delete_book(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .library_service
        .delete_book(id)
        .await
        .map_err(|e| e.to_string())
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
/// `page` is 0-based. Returns the raw image (jpg/png/webp) so the frontend can
/// render it directly via a blob URL.
#[tauri::command]
pub async fn get_book_page(
    id: String,
    page: usize,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, String> {
    let book = state
        .library_service
        .get_book(&id)
        .await
        .map_err(|e| e.to_string())?;
    let path = std::path::PathBuf::from(&book.file_path);
    let bytes = state
        .storage
        .read_page(&path, page)
        .ok_or_else(|| format!("page {page} not found for book {id}"))?;
    Ok(bytes)
}

/// Total page count for a book, read from the archive on disk (cheap zip-header
/// scan, no full decode). Falls back to the stored value if the file is gone.
#[tauri::command]
pub async fn get_book_page_count(
    id: String,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let book = state
        .library_service
        .get_book(&id)
        .await
        .map_err(|e| e.to_string())?;
    let path = std::path::PathBuf::from(&book.file_path);
    let count = state
        .storage
        .count_pages(&path)
        .filter(|&c| c > 0)
        .unwrap_or_else(|| book.page_count.max(0) as usize);
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
