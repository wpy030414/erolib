use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Pool, Sqlite,
};
use std::str::FromStr;
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
        // `mode=rwc` makes SQLite create the file if absent and read/write
        // otherwise. `SqliteConnectOptions::from_str` parses the `sqlite:`
        // URL form including the query string.
        let db_url = format!("sqlite:{}?mode=rwc", db_path.to_string_lossy());

        tracing::info!(target: "erolib::db", "Opening sqlite at {}", db_path.display());

        // SQLite PRAGMAs are per-connection, so chaining them on
        // `SqliteConnectOptions` (rather than running `PRAGMA ...` once on the
        // pool) guarantees every pooled connection picks them up. WAL keeps
        // concurrent download writes from blocking reads; busy_timeout lets
        // writers wait briefly instead of erroring under contention.
        let opts = SqliteConnectOptions::from_str(&db_url)?
            .create_if_missing(true)
            .pragma("journal_mode", "WAL")
            .pragma("synchronous", "NORMAL")
            .pragma("busy_timeout", "5000")
            .pragma("foreign_keys", "ON");
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await
            .map_err(|e| {
                crate::errors::AppError::Other(format!("open sqlite {}: {e}", db_path.display()))
            })?;

        // Apply the schema. The `sqlx::migrate!` macro lives in sqlx-macros,
        // which can't be built in the release profile on this toolchain (see
        // Cargo.toml + task_manager.rs FromRow hand-written for the same
        // reason), so schema.sql is embedded and executed directly. It is fully
        // idempotent (`CREATE ... IF NOT EXISTS`), so re-running on each launch
        // is a no-op once applied. The books + tasks tables already carry their
        // full column sets, so there's no in-place ALTER upgrade step. (This is
        // a plain schema bootstrap, not a versioned migration system — there's
        // no `_sqlx_migrations` table; the `schema/` dir name reflects that.)
        sqlx::query(include_str!("../../schema/schema.sql"))
            .execute(&pool)
            .await
            .map_err(|e| crate::errors::AppError::Other(format!("apply schema: {e}")))?;

        Ok(Self { pool })
    }
}
