use tauri::State;

use crate::services::locale;
use crate::services::settings;
use crate::services::PageCompression;
use crate::AppState;

/// Persist the app locale so SQL queries render tags in the current language.
/// The frontend calls this on startup and whenever the user switches language.
#[tauri::command]
pub async fn set_locale(locale_str: String, state: State<'_, AppState>) -> Result<(), String> {
    locale::set_locale(&state.db, &locale_str)
        .await
        .map_err(|e| e.to_string())
}

/// Persist the page-compression mode applied to NEW imports/downloads.
/// Deserialized straight into the enum, so an invalid value fails here.
/// The frontend calls this on startup and whenever the user switches mode.
#[tauri::command]
pub async fn set_compression(
    mode: PageCompression,
    state: State<'_, AppState>,
) -> Result<(), String> {
    settings::set_compression(&state.db, mode)
        .await
        .map_err(|e| e.to_string())
}
