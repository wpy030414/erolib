use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tauri::State;

use crate::AppState;

/// Sanitise a book title for use as a filename — replace characters that are
/// illegal on Windows/macOS/Linux filesystems, and trim surrounding whitespace.
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// 8-hex-digit metadata hash: stable across sessions (no file-body read) and
/// unique per distinct (source, title, pages, size). Used for the filename
/// suffix and as the de-dupe key (skip if the dest already exists).
fn meta_hash(book: &crate::models::Book) -> String {
    let mut hasher = Sha256::new();
    hasher.update(book.source_post_id.as_deref().unwrap_or("").as_bytes());
    hasher.update(b"\x1f");
    hasher.update(book.source_url.as_deref().unwrap_or("").as_bytes());
    hasher.update(b"\x1f");
    hasher.update(book.title.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(book.page_count.to_string().as_bytes());
    hasher.update(b"\x1f");
    hasher.update(book.file_size.to_string().as_bytes());
    format!("{:x}", hasher.finalize())[..8].to_string()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStats {
    pub copied: usize,
    pub skipped: usize,
}

/// One-way local sync: copy every library book into `target_dir` as
/// `${title}-${metaHash}.cb7`. Books whose destination already exists are
/// skipped (idempotent re-runs).
///
/// This ONLY copies — it never deletes files in the target directory. Removing
/// a book from the library leaves its synced copy behind on purpose, so the
/// user's local files are never touched on removal. Sync therefore fires only
/// on turn-on and when books are added (import / download complete).
#[tauri::command]
pub async fn sync_to_dir(
    target_dir: String,
    state: State<'_, AppState>,
) -> Result<SyncStats, String> {
    let target = PathBuf::from(&target_dir);
    std::fs::create_dir_all(&target)
        .map_err(|e| format!("create sync dir {}: {e}", target.display()))?;

    let books = state
        .library_service
        .list_books(10000, 0)
        .await
        .map_err(|e| e.to_string())?;

    let mut copied = 0;
    let mut skipped = 0;
    for b in &books {
        let h = meta_hash(b);
        let dest = target.join(format!("{}-{}.cb7", sanitize_filename(&b.title), h));
        if dest.is_file() {
            skipped += 1;
            continue;
        }
        let src = Path::new(&b.file_path);
        if !src.is_file() {
            // Book record exists but its cb7 is gone — nothing to copy.
            continue;
        }
        if let Err(e) = std::fs::copy(src, &dest) {
            tracing::warn!(target: "erolib::sync", path = %dest.display(), %e, "copy failed");
            continue;
        }
        copied += 1;
    }

    Ok(SyncStats { copied, skipped })
}
