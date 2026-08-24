//! One-shot library migration: re-encode every page of every book to lossy
//! WebP (the unified page format) and refresh the cover cache.
//!
//! Self-contained on purpose — the crate has no lib target, so a bin can't
//! reach `storage::ensure_webp`; this duplicates its ~20 lines with a
//! stricter failure policy (any failed page aborts that book and is listed,
//! never silently kept).
//!
//! Usage:
//!   cargo run --bin migrate_webp -- <library_dir> [--dry-run] [--only=<uuid>]
//!
//! Run with the app closed (the app holds open zip handles + the sqlite db);
//! the script itself only touches files, never the database — `books` rows
//! (file_path, page_count, delays) stay valid because only page bytes and
//! cover files change. Idempotent: already-webp pages pass through, so a
//! re-run after an interruption finishes the rest.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::{write::FileOptions, ZipWriter};

/// Matches storage.rs `WEBP_QUALITY`.
const WEBP_QUALITY: f32 = 80.0;
/// Image entry extensions, matching storage's `archive_for` whitelist.
const IMAGE_EXTS: [&str; 5] = [".jpg", ".jpeg", ".png", ".webp", ".avif"];

fn main() {
    // Collect first: `.any()`/`find_map` would otherwise consume the iterator
    // and `--only` would silently never see the flag.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(lib_arg) = args.first().cloned() else {
        eprintln!("usage: migrate_webp <library_dir> [--dry-run] [--only=<uuid>]");
        std::process::exit(2);
    };
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let only = args.iter().find_map(|a| a.strip_prefix("--only=").map(String::from));

    let lib = PathBuf::from(lib_arg);
    let covers = lib
        .parent()
        .map(|p| p.join("covers"))
        .unwrap_or_else(|| lib.join("covers"));
    let mut books: Vec<PathBuf> = std::fs::read_dir(&lib)
        .expect("read library dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map_or(false, |x| x == "cb7"))
        .collect();
    books.sort();
    if let Some(uuid) = &only {
        books.retain(|p| {
            p.file_stem()
                .map_or(false, |s| s.to_string_lossy() == *uuid)
        });
    }
    println!(
        "{} books found in {} (dry-run: {})",
        books.len(),
        lib.display(),
        dry_run
    );

    let mut skipped_ugoira = 0;
    let mut failed_books = Vec::new();
    let mut done = 0usize;
    let mut bytes_before = 0u64;
    let mut bytes_after = 0u64;
    let mut converted_pages = 0usize;
    let mut kept_webp_pages = 0usize;

    for book in &books {
        let stem = book.file_stem().unwrap().to_string_lossy().to_string();
        match migrate_book(book, &covers, dry_run) {
            Ok(Report {
                skipped_ugoira: true,
                ..
            }) => {
                skipped_ugoira += 1;
                println!("SKIP  {stem} (ugoira: frame sequence kept as-is)");
            }
            Ok(rep) => {
                done += 1;
                converted_pages += rep.converted;
                kept_webp_pages += rep.kept;
                bytes_before += rep.bytes_before;
                bytes_after += rep.bytes_after;
                println!(
                    "{:4}  {stem}  pages={:3}  converted={:2}  kept-webp={:2}  kept-orig={:2}  {} → {} ({}%)",
                    if dry_run { "DRY " } else { " OK " },
                    rep.pages,
                    rep.converted,
                    rep.kept,
                    rep.kept_original,
                    human(rep.bytes_before),
                    human(rep.bytes_after),
                    if rep.bytes_before > 0 {
                        100 * rep.bytes_after / rep.bytes_before
                    } else {
                        100
                    },
                );
            }
            Err(e) => {
                failed_books.push(format!("{}: {e}", book.display()));
                println!("FAIL  {stem}: {e}");
            }
        }
    }

    println!(
        "\ndone: {done} books, {} pages converted, {} pages already webp; {} → {}",
        converted_pages,
        kept_webp_pages,
        human(bytes_before),
        human(bytes_after)
    );
    println!("skipped (ugoira): {skipped_ugoira}");
    if !failed_books.is_empty() {
        println!("FAILED {} books (re-run with --only=<uuid> after fixing):", failed_books.len());
        for f in &failed_books {
            println!("  {f}");
        }
        std::process::exit(1);
    }
}

struct Report {
    skipped_ugoira: bool,
    pages: usize,
    converted: usize,
    kept: usize,
    kept_original: usize,
    bytes_before: u64,
    bytes_after: u64,
}

fn migrate_book(path: &Path, covers: &Path, dry_run: bool) -> Result<Report, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("open: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("zip: {e}"))?;

    // ComicInfo.xml stays byte-for-byte; non-image entries are dropped the
    // same way create_cb7 drops them.
    let mut comic_info: Option<Vec<u8>> = None;
    let mut pages: Vec<Vec<u8>> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("entry {i}: {e}"))?;
        let name = entry.name().to_lowercase();
        if name == "comicinfo.xml" {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| format!("read ComicInfo: {e}"))?;
            comic_info = Some(buf);
            continue;
        }
        if IMAGE_EXTS.iter().any(|e| name.ends_with(e)) {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| format!("read page {i}: {e}"))?;
            pages.push(buf);
        }
    }
    let Some(comic_info) = comic_info else {
        return Err("no ComicInfo.xml — not an erolib book, leaving alone".into());
    };
    if is_ugoira(&comic_info) {
        return Ok(Report {
            skipped_ugoira: true,
            pages: pages.len(),
            converted: 0,
            kept: 0,
            kept_original: 0,
            bytes_before: 0,
            bytes_after: 0,
        });
    }
    if pages.is_empty() {
        return Err("no image pages".into());
    }

    // Strict: any page that fails to decode/encode fails the book — the
    // original file is left untouched (we only rename at the very end).
    // A page is only re-encoded when the result is actually smaller; tightly
    // compressed sources (pixiv AVIF) can grow at q80 and are kept as-is.
    let mut converted = 0usize;
    let mut kept = 0usize;
    let mut kept_original = 0usize;
    let mut new_pages = Vec::with_capacity(pages.len());
    for (idx, page) in pages.iter().enumerate() {
        if is_webp(page) {
            kept += 1;
            new_pages.push(page.clone());
        } else {
            let out = to_webp(page).map_err(|e| format!("page {}: {e}", idx + 1))?;
            if out.len() < page.len() {
                converted += 1;
                new_pages.push(out);
            } else {
                kept_original += 1;
                new_pages.push(page.clone());
            }
        }
    }

    let bytes_before: u64 = pages.iter().map(|p| p.len() as u64).sum();
    let bytes_after: u64 = new_pages.iter().map(|p| p.len() as u64).sum();

    if dry_run {
        return Ok(Report {
            skipped_ugoira: false,
            pages: pages.len(),
            converted,
            kept,
            kept_original,
            bytes_before,
            bytes_after,
        });
    }

    // Atomic replace: write beside the original, rename over.
    let tmp = path.with_extension("cb7.tmp");
    {
        let file = std::fs::File::create(&tmp).map_err(|e| format!("create tmp: {e}"))?;
        let mut zip = ZipWriter::new(std::io::BufWriter::new(file));
        let options = FileOptions::default();
        zip.start_file("ComicInfo.xml", options)
            .map_err(|e| format!("zip ComicInfo: {e}"))?;
        zip.write_all(&comic_info)
            .map_err(|e| format!("write ComicInfo: {e}"))?;
        for (index, page) in new_pages.iter().enumerate() {
            let ext = ext_of(page);
            let name = format!("{:04}.{ext}", index + 1);
            zip.start_file(&name, options)
                .map_err(|e| format!("zip {name}: {e}"))?;
            zip.write_all(page).map_err(|e| format!("write {name}: {e}"))?;
        }
        zip.finish().map_err(|e| format!("finish zip: {e}"))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("rename over original: {e}"))?;

    // Refresh the cover cache: first image of the new cb7 as webp, dropping
    // any legacy-extension cover files.
    refresh_cover(path, covers)?;

    Ok(Report {
        skipped_ugoira: false,
        pages: pages.len(),
        converted,
        kept,
        kept_original,
        bytes_before,
        bytes_after,
    })
}

/// ugoira books carry `<ero:Delays>[...]</ero:Delays>` — frame timings the
/// reader plays as an animation. Converting frames to lossy webp would both
/// degrade them twice and slow the ingest; leave the whole book as-is.
fn is_ugoira(comic_info: &[u8]) -> bool {
    let xml = String::from_utf8_lossy(comic_info);
    let start = xml.find("<ero:Delays>").map(|i| i + "<ero:Delays>".len());
    let Some(start) = start else { return false };
    xml[start..]
        .find("</ero:Delays>")
        .map(|end| !xml[start..start + end].trim().is_empty())
        .unwrap_or(false)
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP"
}

fn ext_of(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 4 {
        if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return "png";
        }
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return "jpg";
        }
        if is_webp(bytes) {
            return "webp";
        }
        if bytes.len() >= 12 && bytes[4..8] == *b"ftyp" {
            let brand = &bytes[8..12];
            if brand == b"avif" || brand == b"avis" {
                return "avif";
            }
        }
    }
    "jpg"
}

/// Decode any image format the `image` crate knows (jpg/png/webp/avif via
/// avif-native) and re-encode as lossy webp. Failure here is a hard book
/// failure — see `migrate_book` — not a silent pass-through.
fn to_webp(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("decode: {e}"))?;
    let rgba = img.to_rgba8();
    let out = webp::Encoder::from_rgba(rgba.as_raw(), img.width(), img.height()).encode(WEBP_QUALITY);
    if out.is_empty() {
        return Err("webp encode produced empty output".into());
    }
    Ok(out.to_vec())
}

/// Rewrite `covers/{id}.webp` from the first image of the migrated cb7 and
/// delete legacy-extension cover files (old covers are raw bytes whose
/// extension never matched their content).
fn refresh_cover(cb7: &Path, covers: &Path) -> Result<(), String> {
    let id = cb7
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let file = std::fs::File::open(cb7).map_err(|e| format!("cover: open cb7: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("cover: zip: {e}"))?;
    let mut first: Option<Vec<u8>> = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("cover: entry: {e}"))?;
        let name = entry.name().to_lowercase();
        if IMAGE_EXTS.iter().any(|e| name.ends_with(e)) {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| format!("cover: read: {e}"))?;
            first = Some(buf);
            break;
        }
    }
    let Some(first) = first else {
        return Ok(()); // no image to cover with; nothing to update
    };
    let webp = to_webp(&first)?;
    std::fs::write(covers.join(format!("{id}.webp")), &webp)
        .map_err(|e| format!("cover: write webp: {e}"))?;
    for ext in ["jpg", "jpeg", "png", "avif"] {
        let old = covers.join(format!("{id}.{ext}"));
        if old.exists() {
            let _ = std::fs::remove_file(&old);
        }
    }
    Ok(())
}

fn human(n: u64) -> String {
    if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1 << 20) as f64)
    } else if n >= 1 << 10 {
        format!("{:.1} KB", n as f64 / (1 << 10) as f64)
    } else {
        format!("{n} B")
    }
}
