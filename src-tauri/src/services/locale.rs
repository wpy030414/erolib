//! Locale-aware tag translation at the SQL layer.
//!
//! Books carry raw tags in many languages/forms; `schema/tag_translations.sql`
//! provides a lookup so queries can render every tag in the CURRENT app locale
//! without touching stored metadata. This module owns the two SQL fragments
//! every tag-displaying query shares, plus reading the persisted locale.
//!
//! The display column is chosen by the `locale` value in the `settings` table
//! (written by the `set_locale` command from the frontend). `zh`→`zh`,
//! `en`→`en`, `ja`→kanji/katakana block preferred then hiragana then romaji.
//! A raw tag with no mapping falls back to its stored name via COALESCE.

use sqlx::Row;

use crate::db::Database;
use crate::errors::AppError;

/// JOIN fragment that links a raw tag-name column to its translation row,
/// aliased to whichever table alias the query uses for `tags` (e.g. `tags` in
/// the book queries, `t` in `tags_with_count`). The aliases `tr` (resolved map)
/// and `tt` (translations) are referenced by [`display_expr`]. `tag_resolved`
/// is a materialized, PK-indexed table covering exact + fuzzy matches, so the
/// read queries do ONE indexed lookup per tag and never scan or fuzzy-compare
/// on the hot path. Include this once per query that has a `tags`-like table
/// in scope.
pub fn tag_join(tags_alias: &str) -> String {
    format!(
        "LEFT JOIN tag_resolved tr ON tr.name = {alias}.name \
         LEFT JOIN tag_translations tt ON tt.id = tr.tid",
        alias = tags_alias
    )
}

/// The SQL expression yielding a tag's display label for `locale`, referencing
/// the `tt`/`fm` aliases from [`tag_join`] and the query's tag-name column
/// (`tags_alias`, e.g. `tags` or `t`). Falls back to the raw stored name when
/// the tag isn't in the translation table; a missing locale cell falls back
/// through COALESCE so a partial row still renders something meaningful.
pub fn display_expr(locale: &str, tags_alias: &str) -> String {
    let name = format!("{alias}.name", alias = tags_alias);
    match locale {
        // Japanese: prefer the kanji/katakana block, then hiragana, then romaji.
        "ja" => format!("COALESCE(tt.ja_kata, tt.ja_hira, tt.romaji, {name})"),
        "en" => format!("COALESCE(tt.en, tt.zh, tt.romaji, {name})"),
        // zh (default): prefer Chinese, then English, then romaji.
        _ => format!("COALESCE(tt.zh, tt.en, tt.romaji, {name})"),
    }
}

/// Read the persisted locale from `settings` (`'zh' | 'en' | 'ja'`), defaulting
/// to `'zh'` when unset or on error (a fresh DB has no row until the frontend
/// pushes one on startup). Kept deliberately forgiving so tag display never
/// breaks over a missing preference.
pub async fn current_locale(db: &Database) -> String {
    let row = sqlx::query("SELECT value FROM settings WHERE key = 'locale'")
        .fetch_optional(&db.pool)
        .await;
    match row {
        Ok(Some(r)) => {
            let v: String = r.try_get("value").unwrap_or_else(|_| "zh".into());
            match v.as_str() {
                "en" | "ja" => v,
                _ => "zh".into(),
            }
        }
        _ => "zh".into(),
    }
}

/// Persist the locale. Idempotent upsert; called by the `set_locale` command.
pub async fn set_locale(db: &Database, locale: &str) -> Result<(), AppError> {
    let v = match locale {
        "en" | "ja" => locale,
        _ => "zh",
    };
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ('locale', ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(v)
    .execute(&db.pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tag-resolution materialization (exact + fuzzy), kept off the read hot path.
// ---------------------------------------------------------------------------

/// Fingerprint of the current seed set, stored in `settings` after each
/// materialization. Because `tag_form_map` / `tag_resolved` are DERIVED caches
/// whose `tid`s point into `tag_translations.id`, any change to the seeds —
/// including renumbering ids — invalidates them. Comparing a cheap fingerprint
/// (row count + max id + a rolling checksum of the id/form pairs) lets startup
/// detect that and REBUILD the derived tables instead of serving stale tids.
async fn seed_fingerprint(db: &Database) -> Result<String, AppError> {
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(MAX(id),0), \
                COALESCE(SUM((id % 1000) * (LENGTH(COALESCE(zh,'')) + LENGTH(COALESCE(en,'')) + LENGTH(COALESCE(ja_hira,'')) + LENGTH(COALESCE(ja_kata,'')) + LENGTH(COALESCE(romaji,'')))), 0) \
         FROM tag_translations",
    )
    .fetch_one(&db.pool)
    .await
    .map_err(AppError::Db)?;
    Ok(format!("{}:{}:{}", row.0, row.1, row.2))
}

/// Drop and rebuild the derived `tag_form_map` + `tag_resolved` tables when the
/// seed fingerprint changed since the last materialization (or was never set).
/// Returns true if a rebuild happened, so the caller knows fuzzy rows were
/// wiped and must be recomputed. When the fingerprint matches, this is a no-op
/// and the incremental path runs instead.
async fn rebuild_derived_if_seed_changed(db: &Database) -> Result<bool, AppError> {
    let fp = seed_fingerprint(db).await?;
    let stored: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'tag_seed_fp'")
        .fetch_optional(&db.pool)
        .await
        .map_err(AppError::Db)?;
    if stored.as_deref() == Some(fp.as_str()) {
        return Ok(false);
    }
    tracing::info!(target: "erolib::db", "tag seed changed — rebuilding derived tag maps");
    // `tags` and the translation seeds are untouched; only the derived caches.
    for sql in ["DELETE FROM tag_form_map", "DELETE FROM tag_resolved"] {
        sqlx::query(sql).execute(&db.pool).await.map_err(AppError::Db)?;
    }
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ('tag_seed_fp', ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(&fp)
    .execute(&db.pool)
    .await
    .map_err(AppError::Db)?;
    Ok(true)
}

/// Rebuild the exact form→concept map from `v_tag_form`. First rebuilds the
/// derived tables from scratch if the seed changed (so renumbered ids can't
/// leave stale tids); otherwise idempotently tops up only the missing forms
/// (`INSERT OR IGNORE`). Called once at startup.
pub async fn materialize_form_map(db: &Database) -> Result<(), AppError> {
    rebuild_derived_if_seed_changed(db).await?;
    sqlx::query("INSERT OR IGNORE INTO tag_form_map (form, tid) SELECT form, id FROM v_tag_form")
        .execute(&db.pool)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}

/// Resolve every stored tag into `tag_resolved`: exact matches first, then a
/// fuzzy fallback for the names that have no exact hit. Idempotent and
/// incremental — `INSERT OR IGNORE` keeps existing rows, so on startup it only
/// computes rows for tags not already resolved (plus any whose seed changed is
/// intentionally left as-is to avoid churning). Fuzzy selection picks the best
/// concept by SIMILARITY (≥ 0.8) only when it beats the next-best DIFFERENT
/// concept by ≥ 0.03, which suppresses ambiguous mis-matches (they stay
/// unresolved → raw passthrough).
pub async fn materialize_resolved(db: &Database) -> Result<(), AppError> {
    // Exact path: every tag that maps directly via the form map.
    sqlx::query(
        "INSERT OR IGNORE INTO tag_resolved (name, tid) \
         SELECT t.name, m.tid FROM tags t \
         JOIN tag_form_map m ON m.form = t.name",
    )
    .execute(&db.pool)
    .await
    .map_err(AppError::Db)?;

    // Fuzzy fallback: only for tags with NO resolved row yet. For each, pick
    // the best-matching concept by SIMILARITY over all known forms, requiring
    // sim ≥ 0.8 and a ≥ 0.03 lead over the best OTHER concept.
    sqlx::query(
        "WITH rest AS ( \
           SELECT t.name AS name FROM tags t \
           LEFT JOIN tag_resolved r ON r.name = t.name \
           WHERE r.tid IS NULL \
         ), \
         scored AS ( \
           SELECT rest.name AS name, f.id AS tid, MAX(SIMILARITY(rest.name, f.form)) AS s \
           FROM rest JOIN v_tag_form f ON SIMILARITY(rest.name, f.form) >= 0.8 \
           GROUP BY rest.name, f.id \
         ), \
         ranked AS ( \
           SELECT name, tid, s, \
                  ROW_NUMBER() OVER (PARTITION BY name ORDER BY s DESC, tid) AS rn, \
                  MAX(s) OVER (PARTITION BY name) AS best_s, \
                  MAX(CASE WHEN rn2 = 2 THEN s END) OVER (PARTITION BY name) AS second_s \
           FROM (SELECT scored.*, ROW_NUMBER() OVER (PARTITION BY name ORDER BY s DESC, tid) AS rn2 FROM scored) \
         ) \
         INSERT OR IGNORE INTO tag_resolved (name, tid) \
         SELECT name, tid FROM ranked \
         WHERE rn = 1 AND (second_s IS NULL OR best_s - second_s >= 0.03)",
    )
    .execute(&db.pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

/// Resolve a single freshly-added tag (called right after `upsert_tag_and_link`
/// inserts a new `tags` row). Exact first, else fuzzy with the same best+margin
/// rule. Cheap (one tag), so it's safe to run inline during book registration.
/// Takes the pool (not `&Database`) so it composes with the upsert call site.
pub async fn resolve_one_tag(pool: &sqlx::Pool<sqlx::Sqlite>, name: &str) -> Result<(), AppError> {
    sqlx::query(
        "INSERT OR IGNORE INTO tag_resolved (name, tid) \
         SELECT ?, m.tid FROM tag_form_map m WHERE m.form = ?",
    )
    .bind(name)
    .bind(name)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;

    sqlx::query(
        "WITH cand AS ( \
           SELECT f.id AS tid, MAX(SIMILARITY(?, f.form)) AS s \
           FROM v_tag_form f WHERE SIMILARITY(?, f.form) >= 0.8 \
           GROUP BY f.id \
         ), \
         ranked AS ( \
           SELECT tid, s, ROW_NUMBER() OVER (ORDER BY s DESC, tid) AS rn, \
                  MAX(s) OVER () AS best_s, \
                  MAX(CASE WHEN rn2 = 2 THEN s END) OVER () AS second_s \
           FROM (SELECT cand.*, ROW_NUMBER() OVER (ORDER BY s DESC, tid) AS rn2 FROM cand) \
         ) \
         INSERT OR IGNORE INTO tag_resolved (name, tid) \
         SELECT ?, tid FROM ranked \
         WHERE rn = 1 AND (second_s IS NULL OR best_s - second_s >= 0.03)",
    )
    .bind(name)
    .bind(name)
    .bind(name)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

