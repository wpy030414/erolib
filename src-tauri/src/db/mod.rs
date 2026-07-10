use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use tauri::{AppHandle, Manager};

pub struct Database {
    pub pool: Pool<Sqlite>,
}

impl Database {
    /// Create a new Database by opening (or creating) the SQLite file inside
    /// the app's data directory and running migrations.
    pub async fn new(app_handle: &AppHandle) -> Result<Self, crate::errors::AppError> {
        let data_dir = app_handle
            .path()
            .app_local_data_dir()
            .map_err(|e| crate::errors::AppError::Other(format!("resolve app_local_data_dir: {e}")))?;

        tracing::info!(target: "erolib::db", "Using data dir: {}", data_dir.display());

        // Ensure the directory exists before SQLite tries to open the file.
        std::fs::create_dir_all(&data_dir).map_err(|e| {
            crate::errors::AppError::Other(format!("create_dir_all {}: {e}", data_dir.display()))
        })?;

        let db_path = data_dir.join("manga-manager.db");
        // sqlx's default connect mode is read-only (no create), so opening a
        // never-yet-existing SQLite file returns code 14. `mode=rwc` makes the
        // pool create the file if absent and read/write otherwise.
        let db_url = format!("sqlite:{}?mode=rwc", db_path.to_string_lossy());

        tracing::info!(target: "erolib::db", "Opening sqlite at {}", db_path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .map_err(|e| {
                crate::errors::AppError::Other(format!("open sqlite {}: {e}", db_path.display()))
            })?;

        sqlx::query("PRAGMA foreign_keys = ON;")
            .execute(&pool)
            .await
            .map_err(|e| crate::errors::AppError::Other(format!("pragma foreign_keys: {e}")))?;

        // `sqlx::migrate!` resolves the migrations directory relative to
        // CARGO_MANIFEST_DIR, so it works under both `tauri dev` and builds.
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| crate::errors::AppError::Other(format!("migrations: {e}")))?;

        Ok(Self { pool })
    }
}
