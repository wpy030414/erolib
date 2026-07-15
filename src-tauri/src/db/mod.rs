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

        let db_path = data_dir.join("erolib.db");

        // 一次性文件名迁移：项目原名 manga-manager，DB 文件曾叫
        // manga-manager.db。首次启动新版时把旧文件（含 WAL/SHM 附属文件，WAL
        // 模式开着会生成它们）原地重命名为 erolib.db，既有库无损搬迁。仅当
        // 新名不存在时执行——主人已跑过新版（erolib.db 已在）则不覆盖；两者
        // 都在时优先新名，旧文件原样留作备份。同一目录内 rename 是原子的。
        if !db_path.exists() {
            let legacy = data_dir.join("manga-manager.db");
            if legacy.exists() {
                for (old_name, new_name) in [
                    ("manga-manager.db", "erolib.db"),
                    ("manga-manager.db-wal", "erolib.db-wal"),
                    ("manga-manager.db-shm", "erolib.db-shm"),
                ] {
                    let from = data_dir.join(old_name);
                    if from.exists() {
                        let to = data_dir.join(new_name);
                        if let Err(e) = std::fs::rename(&from, &to) {
                            tracing::warn!(
                                target: "erolib::db",
                                "rename {} -> {}: {e}",
                                from.display(),
                                to.display()
                            );
                        }
                    }
                }
                tracing::info!(
                    target: "erolib::db",
                    "Migrated legacy DB manga-manager.db -> erolib.db"
                );
            }
        }

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

        // One-time data fixup (PRAGMA user_version guards it to a single run).
        // Historical reading_sessions.duration_ms stored the frontend's per-book
        // *cumulative* total (pushed every second, last-write-wins), not the
        // per-session span — so SUM(duration_ms) wildly inflated "本周已阅读".
        // The reader now reports a per-session delta; zero the corrupted
        // historical values so old rows stop skewing the stat and new sessions
        // accumulate correctly from 0. (No _sqlx_migrations table exists;
        // user_version is this app's lightweight migration guard.)
        let user_version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .map_err(|e| crate::errors::AppError::Other(format!("read user_version: {e}")))?;
        if user_version < 1 {
            sqlx::query("UPDATE reading_sessions SET duration_ms = 0")
                .execute(&pool)
                .await
                .map_err(|e| crate::errors::AppError::Other(format!("reset reading durations: {e}")))?;
            sqlx::query("PRAGMA user_version = 1")
                .execute(&pool)
                .await
                .map_err(|e| crate::errors::AppError::Other(format!("set user_version=1: {e}")))?;
            tracing::info!(target: "erolib::db", "Reset corrupted reading-session durations to 0 (user_version 0 -> 1)");
        }

        Ok(Self { pool })
    }
}
