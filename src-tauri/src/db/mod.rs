use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
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

        // Apply migrations. The `sqlx::migrate!` macro lives in sqlx-macros,
        // which can't be built in the release profile on this toolchain (see
        // Cargo.toml), so the SQL is embedded and executed directly. Every
        // migration is idempotent (`CREATE ... IF NOT EXISTS` /
        // `DROP ... IF EXISTS`-then-rebuild), so re-running on each launch is a
        // no-op once applied.
        for sql in [
            include_str!("../../migrations/20260709000001_init.sql"),
            include_str!("../../migrations/20260709000002_fix_fts.sql"),
            include_str!("../../migrations/20260710000001_tasks.sql"),
        ] {
            sqlx::query(sql)
                .execute(&pool)
                .await
                .map_err(|e| crate::errors::AppError::Other(format!("migrations: {e}")))?;
        }

        // books: add source-metadata columns. ALTER TABLE ADD COLUMN has no
        // IF NOT EXISTS and migrations re-run every launch, so guard each with
        // a PRAGMA table_info check. New installs get these via init.sql's
        // CREATE TABLE; this upgrades existing DBs in place.
        for (col, ddl) in [
            ("source_post_id", "TEXT"),
            ("author", "TEXT"),
            ("author_id", "TEXT"),
            ("published_at", "TEXT"),
        ] {
            ensure_column(&pool, "books", col, ddl).await?;
        }

        Ok(Self { pool })
    }
}

/// Idempotently add a column: `ALTER TABLE ... ADD COLUMN` has no
/// `IF NOT EXISTS`, so check `PRAGMA table_info` first.
async fn ensure_column(
    pool: &Pool<Sqlite>,
    table: &str,
    column: &str,
    ddl: &str,
) -> Result<(), crate::errors::AppError> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .map_err(|e| crate::errors::AppError::Other(format!("pragma table_info {table}: {e}")))?;
    let exists = rows.iter().any(|r| {
        r.try_get::<String, _>("name")
            .map(|n| n == column)
            .unwrap_or(false)
    });
    if !exists {
        sqlx::query(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}"))
            .execute(pool)
            .await
            .map_err(|e| {
                crate::errors::AppError::Other(format!("alter {table}.{column}: {e}"))
            })?;
    }
    Ok(())
}
