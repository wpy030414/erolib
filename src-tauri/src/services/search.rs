use std::sync::Arc;

use sqlx::Row;

use crate::db::Database;
use crate::errors::AppError;
use crate::models::{Book, Collection, SearchFacets, SearchQuery, SearchResult, TagCount};

pub struct SearchService {
    db: Arc<Database>,
}

impl SearchService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub async fn search(&self, query: SearchQuery) -> Result<SearchResult, AppError> {
        let page = query.page.max(1);
        let page_size = query.page_size.clamp(1, 200);
        let offset = (page - 1) * page_size;

        // Build the WHERE clause dynamically.
        let mut conditions: Vec<String> = Vec::new();
        let mut args: Vec<String> = Vec::new();

        if let Some(text) = &query.text {
            if !text.trim().is_empty() {
                // Free-text search spans title, author AND tags — typing a tag
                // name surfaces every book carrying it (handy for tag cleanup).
                conditions.push(
                    "(books.title LIKE ? OR books.author LIKE ? OR books.id IN (\
                       SELECT bt.book_id FROM book_tags bt \
                       JOIN tags t ON t.id = bt.tag_id WHERE t.name LIKE ?))".into(),
                );
                let pattern = format!("%{}%", text.trim());
                args.push(pattern.clone()); // title
                args.push(pattern.clone()); // author
                args.push(pattern); // tag name
            }
        }

        if let Some(sources) = &query.sources {
            if !sources.is_empty() {
                let placeholders = sources.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                conditions.push(format!("books.source_plugin IN ({})", placeholders));
                for s in sources {
                    args.push(s.clone());
                }
            }
        }

        if let Some(from) = &query.date_from {
            conditions.push("books.created_at >= ?".into());
            args.push(from.to_rfc3339());
        }
        if let Some(to) = &query.date_to {
            conditions.push("books.created_at <= ?".into());
            args.push(to.to_rfc3339());
        }

        // Tag filters require joins (AND semantics: book has all listed tags).
        let tag_join_and = if let Some(tags) = &query.tags {
            if !tags.is_empty() {
                Some(tags.clone())
            } else {
                None
            }
        } else {
            None
        };
        let tag_join_any = if let Some(tags) = &query.tags_any {
            if !tags.is_empty() {
                Some(tags.clone())
            } else {
                None
            }
        } else {
            None
        };

        let collection_join = if let Some(cols) = &query.collections {
            if !cols.is_empty() {
                Some(cols.clone())
            } else {
                None
            }
        } else {
            None
        };

        let joins = build_joins(tag_join_and.as_ref(), tag_join_any.as_ref(), collection_join.as_ref());

        // Additional join conditions.
        if let Some(tags) = &tag_join_and {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            conditions.push(format!("books.id IN (
                SELECT bt.book_id FROM book_tags bt
                JOIN tags t ON t.id = bt.tag_id
                WHERE t.name IN ({})
                GROUP BY bt.book_id HAVING COUNT(DISTINCT t.id) = ?
            )", placeholders));
            for t in tags {
                args.push(t.clone());
            }
            args.push(tags.len().to_string());
        }

        if let Some(cols) = &collection_join {
            let placeholders = cols.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            conditions.push(format!("books.id IN (
                SELECT cb.book_id FROM collection_books cb
                JOIN collections c ON c.id = cb.collection_id
                WHERE c.name IN ({})
            )", placeholders));
            for c in cols {
                args.push(c.clone());
            }
        }

        if let Some(tags) = &tag_join_any {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            conditions.push(format!("books.id IN (
                SELECT bt.book_id FROM book_tags bt
                JOIN tags t ON t.id = bt.tag_id
                WHERE t.name IN ({})
            )", placeholders));
            for t in tags {
                args.push(t.clone());
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        // Sorting.
        let order_clause = match query.sort_by.as_str() {
            "title" => format!(" ORDER BY books.title {}", query.sort_order),
            "date" => format!(" ORDER BY books.created_at {}", query.sort_order),
            "size" => format!(" ORDER BY books.file_size {}", query.sort_order),
            _ => format!(" ORDER BY books.created_at {}", query.sort_order),
        };

        // Count total.
        let count_sql = format!("SELECT COUNT(DISTINCT books.id) as count FROM books {joins} {where_clause}");
        let total_row = build_count_query(&count_sql, &args)
            .fetch_one(&self.db.pool)
            .await
            .map_err(AppError::Db)?;
        let total: i64 = total_row.try_get("count").unwrap_or(0);

        // Fetch page. LEFT JOIN tags so Book.tags is populated (GROUP_CONCAT);
        // GROUP BY books.id de-dups the tag-join rows (replaces DISTINCT).
        let data_sql = format!(
            "SELECT books.*, GROUP_CONCAT(tags.name, ',') AS tags \
             FROM books \
             LEFT JOIN book_tags ON book_tags.book_id = books.id \
             LEFT JOIN tags ON tags.id = book_tags.tag_id \
             {where_clause} \
             GROUP BY books.id {order_clause} LIMIT ? OFFSET ?"
        );
        let mut data_query = build_query(&data_sql, &args);
        data_query = data_query.bind(page_size.to_string());
        data_query = data_query.bind(offset.to_string());
        let books: Vec<Book> = data_query
            .fetch_all(&self.db.pool)
            .await
            .map_err(AppError::Db)?;

        Ok(SearchResult {
            books,
            total,
            facets: SearchFacets::default(),
        })
    }

    /// Every tag with its book usage count, sorted by count desc then name,
    /// capped to the top 30. Feeds the tag-chip filter row.
    ///
    /// When `text` is given, counts are tallied only over the books matching
    /// that text (title/author) — the text query dominates the chips, so the
    /// chip set and its counts reflect the text-filtered result set. Tags with
    /// zero matches simply drop out (INNER JOIN). With no text, the full
    /// library is tallied.
    pub async fn tags_with_count(&self, text: Option<&str>) -> Result<Vec<TagCount>, AppError> {
        match text {
            Some(t) => {
                let pattern = format!("%{}%", t);
                sqlx::query_as::<_, TagCount>(
                    "SELECT t.name AS name, COUNT(bt.book_id) AS count \
                     FROM tags t \
                     JOIN book_tags bt ON bt.tag_id = t.id \
                     JOIN books b ON b.id = bt.book_id \
                     WHERE (b.title LIKE ? OR b.author LIKE ? \
                            OR b.id IN (\
                                SELECT bt2.book_id FROM book_tags bt2 \
                                JOIN tags t2 ON t2.id = bt2.tag_id WHERE t2.name LIKE ?)) \
                     GROUP BY t.id \
                     ORDER BY count DESC, t.name ASC \
                     LIMIT 30",
                )
                .bind(&pattern)
                .bind(&pattern)
                .bind(&pattern)
                .fetch_all(&self.db.pool)
                .await
                .map_err(AppError::Db)
            }
            None => {
                sqlx::query_as::<_, TagCount>(
                    "SELECT t.name AS name, COUNT(bt.book_id) AS count \
                     FROM tags t \
                     JOIN book_tags bt ON bt.tag_id = t.id \
                     GROUP BY t.id \
                     ORDER BY count DESC, t.name ASC \
                     LIMIT 30",
                )
                .fetch_all(&self.db.pool)
                .await
                .map_err(AppError::Db)
            }
        }
    }

    pub async fn collections(&self) -> Result<Vec<Collection>, AppError> {
        sqlx::query_as::<_, Collection>("SELECT * FROM collections ORDER BY name")
            .fetch_all(&self.db.pool)
            .await
            .map_err(AppError::Db)
    }
}

fn build_joins(
    _tag_and: Option<&Vec<String>>,
    _tag_any: Option<&Vec<String>>,
    _collection: Option<&Vec<String>>,
) -> String {
    // Joins are expressed as subqueries in WHERE for simplicity; returns empty.
    String::new()
}

fn build_count_query<'a>(
    sql: &'a str,
    args: &'a [String],
) -> sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'a>> {
    let mut q = sqlx::query(sql);
    for a in args {
        q = q.bind(a.clone());
    }
    q
}

fn build_query<'a>(
    sql: &'a str,
    args: &'a [String],
) -> sqlx::query::QueryAs<'a, sqlx::Sqlite, Book, sqlx::sqlite::SqliteArguments<'a>> {
    let mut q = sqlx::query_as::<_, Book>(sql);
    for a in args {
        q = q.bind(a.clone());
    }
    q
}
