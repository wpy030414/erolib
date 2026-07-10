use tauri::State;

use crate::models::SearchQuery;
use crate::AppState;

#[tauri::command]
pub async fn search_books(
    query: SearchQuery,
    state: State<'_, AppState>,
) -> Result<crate::models::SearchResult, String> {
    state
        .search_service
        .search(query)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_all_tags(
    text: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::TagCount>, String> {
    // Treat blank text as "no text" (full-library tally).
    let q = text.as_deref().map(str::trim).filter(|t| !t.is_empty());
    state
        .search_service
        .tags_with_count(q)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_all_collections(
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::Collection>, String> {
    state
        .search_service
        .collections()
        .await
        .map_err(|e| e.to_string())
}
