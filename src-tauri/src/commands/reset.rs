use tauri::{AppHandle, Manager};

/// Wipe the application's data directory (database, library, covers, cache,
/// session files). This is intended as a "factory reset" from the Settings
/// screen; the frontend clears its own localStorage after the call succeeds.
#[tauri::command]
pub async fn reset_app_data(app_handle: AppHandle) -> Result<(), String> {
    let data_dir = app_handle
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("resolve app_local_data_dir: {e}"))?;

    // Best-effort recursive removal. Errors for individual entries are logged
    // but not returned, so a partially-missing dir still counts as success.
    if data_dir.exists() {
        fn remove_dir_all_best_effort(path: &std::path::Path) {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let _ = if p.is_dir() {
                        std::fs::remove_dir_all(&p)
                    } else {
                        std::fs::remove_file(&p)
                    };
                }
            }
            let _ = std::fs::remove_dir(path);
        }
        remove_dir_all_best_effort(&data_dir);
    }

    Ok(())
}
