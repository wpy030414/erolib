//! App-level settings persisted in the `settings` KV table, beyond locale.
//!
//! Owns the page-compression preference (`PageCompression`, defined next to
//! the encoder in `storage.rs`): how NEW imports and downloads are encoded.
//! The frontend keeps the user's choice in localStorage and pushes it here on
//! startup / change (same pattern as `locale`); the backend reads it once per
//! ingest. Existing books are never re-encoded.

use sqlx::Row;

use crate::db::Database;
use crate::errors::AppError;
use crate::services::storage::PageCompression;

/// Read the persisted page-compression mode, defaulting to `None` when unset
/// or unrecognized (a fresh DB has no row until the frontend pushes one on
/// startup). Deliberately forgiving — mirroring `locale::current_locale` —
/// so an ingest never breaks over a missing preference.
pub async fn current_compression(db: &Database) -> PageCompression {
    let row = sqlx::query("SELECT value FROM settings WHERE key = 'compression'")
        .fetch_optional(&db.pool)
        .await;
    match row {
        Ok(Some(r)) => r
            .try_get::<String, _>("value")
            .ok()
            .and_then(|v| PageCompression::parse(&v))
            .unwrap_or_default(),
        _ => PageCompression::None,
    }
}

/// Persist the compression mode. Idempotent upsert; called by the
/// `set_compression` command.
pub async fn set_compression(db: &Database, mode: PageCompression) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ('compression', ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(mode.as_str())
    .execute(&db.pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}
