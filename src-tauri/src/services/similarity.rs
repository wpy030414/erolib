//! The `SIMILARITY(a, b)` SQLite scalar UDF backing fuzzy tag matching.
//!
//! Sources misspell / pluralize / hyphenate tags, so an exact lookup misses
//! forms like "japanes" or "stocking". Fuzzy matching resolves a raw tag to a
//! concept when its similarity to a known form is ≥ 0.8 (see
//! `schema/tag_translations.sql` + `services/locale.rs`), where similarity is
//! normalized Levenshtein: `1 - dist / max(len_a, len_b)` — which inherently
//! captures both "similar length" and "≥80% of characters matching".
//!
//! The heavy best-match selection runs ONLY at materialization time (startup +
//! new-tag upsert), never on the query hot path; queries just JOIN the
//! precomputed `tag_resolved` table. sqlx has no safe `create_function`, so we
//! register through the raw handle exactly like sqlx's own `regexp` UDF does.

use libsqlite3_sys as ffi;
use sqlx::sqlite::SqliteConnection;
use std::ffi::c_void;

/// Levenshtein edit distance between two strings, counted in Unicode scalar
/// values (chars), so CJK / kana count as single characters. O(min(m, n))
/// space via a single rolling row.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (long, short) = if a.len() >= b.len() { (&a, &b) } else { (&b, &a) };
    let mut prev: Vec<usize> = (0..=short.len()).collect();
    let mut cur: Vec<usize> = vec![0; short.len() + 1];
    for (i, &lc) in long.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &sc) in short.iter().enumerate() {
            let sub = prev[j] + usize::from(lc != sc);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(sub);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[short.len()]
}

/// Normalized similarity in `0.0..=1.0`: identical → 1.0. Case-insensitive to
/// match the `COLLATE NOCASE` exact path. Two empty strings are considered
/// identical (1.0); one empty vs non-empty is 0.0.
fn similarity(a: &str, b: &str) -> f64 {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let la = a.chars().count();
    let lb = b.chars().count();
    let max = la.max(lb);
    if max == 0 {
        return 1.0;
    }
    1.0 - (levenshtein(&a, &b) as f64 / max as f64)
}

/// Extract a `&str` from a `sqlite3_value` that holds TEXT, else None.
///
/// # Safety
/// `arg` must be a valid `sqlite3_value*`; the returned slice borrows SQLite's
/// buffer, valid only until the function returns (we copy/compare immediately).
unsafe fn text_arg<'a>(arg: *mut ffi::sqlite3_value) -> Option<&'a str> {
    if arg.is_null() || ffi::sqlite3_value_type(arg) != ffi::SQLITE_TEXT {
        return None;
    }
    let ptr = ffi::sqlite3_value_text(arg);
    if ptr.is_null() {
        return None;
    }
    let len = ffi::sqlite3_value_bytes(arg) as usize;
    let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), len);
    std::str::from_utf8(bytes).ok()
}

unsafe extern "C" fn similarity_func(
    ctx: *mut ffi::sqlite3_context,
    n_arg: i32,
    args: *mut *mut ffi::sqlite3_value,
) {
    if n_arg != 2 {
        ffi::sqlite3_result_error_code(ctx, ffi::SQLITE_CONSTRAINT_FUNCTION);
        return;
    }
    let a = text_arg(*args.offset(0));
    let b = text_arg(*args.offset(1));
    match (a, b) {
        (Some(a), Some(b)) => ffi::sqlite3_result_double(ctx, similarity(a, b)),
        _ => ffi::sqlite3_result_null(ctx),
    }
}

/// Register `SIMILARITY(a, b)` on one pooled connection. Invoked from
/// `SqlitePoolOptions::after_connect` so every connection in the pool gets it.
pub async fn register(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    static FN_NAME: &[u8] = b"similarity\0";
    let mut handle = conn.lock_handle().await?;
    let rc = unsafe {
        ffi::sqlite3_create_function_v2(
            handle.as_raw_handle().as_ptr(),
            FN_NAME.as_ptr().cast(),
            2,
            ffi::SQLITE_UTF8 | ffi::SQLITE_DETERMINISTIC,
            std::ptr::null_mut::<c_void>(),
            Some(similarity_func),
            None, // xStep (None => scalar)
            None, // xFinal
            None, // xDestroy
        )
    };
    if rc != ffi::SQLITE_OK as i32 {
        return Err(sqlx::Error::Protocol(format!("register SIMILARITY udf rc={rc}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("日本語", "日本語"), 0);
        assert_eq!(levenshtein("日本語", "日本"), 1);
    }

    #[test]
    fn similarity_scores() {
        assert_eq!(similarity("japanese", "japanese"), 1.0);
        assert_eq!(similarity("Japanese", "japanese"), 1.0); // case-insensitive
        assert!(similarity("japanes", "japanese") >= 0.8);
        assert!(similarity("stocking", "stockings") >= 0.8);
        assert!(similarity("rapes", "rape") >= 0.8);
        assert!(similarity("xyz", "japanese") < 0.8);
        assert!(similarity("lo", "lolicon") < 0.8);
        assert_eq!(similarity("", ""), 1.0);
        assert_eq!(similarity("", "abc"), 0.0);
    }
}
