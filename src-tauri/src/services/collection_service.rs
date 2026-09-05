use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;

use crate::db::Database;
use crate::errors::AppError;
use crate::models::Collection;

pub struct CollectionService {
    db: Arc<Database>,
}

impl CollectionService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Idempotent ALTER TABLE — adds the `position` column if it doesn't exist
    /// (migration from the original schema that lacked it). Safe to call on
    /// every startup.
    pub async fn ensure_position_column(&self) -> Result<(), AppError> {
        let row = sqlx::query("PRAGMA table_info(collections)")
            .fetch_all(&self.db.pool)
            .await
            .map_err(AppError::Db)?;
        let has_position = row.iter().any(|r| {
            let name: String = r.try_get("name").unwrap_or_default();
            name == "position"
        });
        if !has_position {
            sqlx::query("ALTER TABLE collections ADD COLUMN position INTEGER NOT NULL DEFAULT 0")
                .execute(&self.db.pool)
                .await
                .map_err(AppError::Db)?;
        }
        Ok(())
    }

    /// Reorder collections to match the given list of (id, position) pairs.
    pub async fn reorder(&self, positions: Vec<(String, i32)>) -> Result<(), AppError> {
        for (id, pos) in positions {
            sqlx::query("UPDATE collections SET position = ? WHERE id = ?")
                .bind(pos)
                .bind(&id)
                .execute(&self.db.pool)
                .await
                .map_err(AppError::Db)?;
        }
        Ok(())
    }

    /// List all collections ordered by position then created_at.
    pub async fn list_collections(&self) -> Result<Vec<Collection>, AppError> {
        sqlx::query_as::<_, Collection>(
            "SELECT * FROM collections ORDER BY position ASC, created_at ASC",
        )
        .fetch_all(&self.db.pool)
        .await
        .map_err(AppError::Db)
    }

    /// Create a new collection with the given name and return it.
    /// Position is set to MAX(position)+1 so new items go to the bottom.
    pub async fn create_collection(&self, name: &str) -> Result<Collection, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        // Find the next position value (MAX + 1).
        let max_pos: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(position) FROM collections",
        )
        .fetch_one(&self.db.pool)
        .await
        .map_err(AppError::Db)?;
        let position = max_pos.unwrap_or(-1) + 1;

        sqlx::query(
            "INSERT INTO collections (id, name, position, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(position)
        .bind(&now)
        .execute(&self.db.pool)
        .await
        .map_err(AppError::Db)?;
        self.get_by_id(&id).await
    }

    async fn get_by_id(&self, id: &str) -> Result<Collection, AppError> {
        sqlx::query_as::<_, Collection>("SELECT * FROM collections WHERE id = ?")
            .bind(id)
            .fetch_one(&self.db.pool)
            .await
            .map_err(AppError::Db)
    }

    /// Rename an existing collection.
    pub async fn rename_collection(&self, id: &str, name: &str) -> Result<(), AppError> {
        let rows = sqlx::query("UPDATE collections SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(&self.db.pool)
            .await
            .map_err(AppError::Db)?;
        if rows.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Collection {id}")));
        }
        Ok(())
    }

    /// Delete a collection. FK cascade removes its `collection_books` rows.
    pub async fn delete_collection(&self, id: &str) -> Result<(), AppError> {
        let rows = sqlx::query("DELETE FROM collections WHERE id = ?")
            .bind(id)
            .execute(&self.db.pool)
            .await
            .map_err(AppError::Db)?;
        if rows.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Collection {id}")));
        }
        Ok(())
    }

    /// Add a book to a collection (idempotent: INSERT OR IGNORE).
    pub async fn add_book_to_collection(
        &self,
        collection_id: &str,
        book_id: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT OR IGNORE INTO collection_books (collection_id, book_id) VALUES (?, ?)",
        )
        .bind(collection_id)
        .bind(book_id)
        .execute(&self.db.pool)
        .await
        .map_err(AppError::Db)?;
        Ok(())
    }

    /// Remove a book from a collection.
    pub async fn remove_book_from_collection(
        &self,
        collection_id: &str,
        book_id: &str,
    ) -> Result<(), AppError> {
        sqlx::query("DELETE FROM collection_books WHERE collection_id = ? AND book_id = ?")
            .bind(collection_id)
            .bind(book_id)
            .execute(&self.db.pool)
            .await
            .map_err(AppError::Db)?;
        Ok(())
    }

    /// Return the IDs of all collections the given book belongs to.
    pub async fn get_book_collections(&self, book_id: &str) -> Result<Vec<String>, AppError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT collection_id FROM collection_books WHERE book_id = ?",
        )
        .bind(book_id)
        .fetch_all(&self.db.pool)
        .await
        .map_err(AppError::Db)?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}
