use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn list_collections(
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::Collection>, String> {
    state
        .collection_service
        .list_collections()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reorder_collections(
    positions: Vec<(String, i32)>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .collection_service
        .reorder(positions)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_collection(
    name: String,
    state: State<'_, AppState>,
) -> Result<crate::models::Collection, String> {
    state
        .collection_service
        .create_collection(&name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_collection(
    id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .collection_service
        .rename_collection(&id, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_collection(
    id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .collection_service
        .delete_collection(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_book_to_collection(
    collection_id: String,
    book_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .collection_service
        .add_book_to_collection(&collection_id, &book_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_book_from_collection(
    collection_id: String,
    book_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .collection_service
        .remove_book_from_collection(&collection_id, &book_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_book_collections(
    book_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    state
        .collection_service
        .get_book_collections(&book_id)
        .await
        .map_err(|e| e.to_string())
}
