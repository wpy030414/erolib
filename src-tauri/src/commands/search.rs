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
pub async fn get_all_tags(state: State<'_, AppState>) -> Result<Vec<crate::models::Tag>, String> {
    state
        .search_service
        .tags()
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
