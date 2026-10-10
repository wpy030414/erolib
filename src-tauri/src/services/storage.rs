use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use uuid::Uuid;
use zip::{write::FileOptions, ZipWriter};

use crate::models::BookMetadata;
use std::io::{Read, Write};

/// A CB7/CBZ archive kept open + its image-entry indices precomputed, so that
/// flipping through pages of the same book skips the per-page `File::open` +
/// `ZipArchive::new` + O(n) central-directory scan. The archive sits behind a
/// Mutex so blocking threads in `spawn_blocking` can share one handle.
struct CachedArchive {
    archive: Mutex<zip::ZipArchive<std::fs::File>>,
    image_indices: Vec<usize>,
}

/// Cap on how many book archives we keep open at once. A safety net for users
/// bouncing between many books — beyond this the oldest entry is dropped.
const PAGE_CACHE_MAX: usize = 8;

/// Lossy WebP quality for the `PageCompression::Lossy` mode (`encode_page`).
/// 80 keeps manga linework visually lossless while cutting size well below
/// jpeg.
const WEBP_QUALITY: f32 = 80.0;

/// How page and cover images are encoded when a book enters the library.
/// Stored in the `settings` KV table (and sent over IPC) as the lowercase
/// variant name: 'none' | 'lossless' | 'lossy'. Only affects NEW imports and
/// downloads — existing books are never re-encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageCompression {
    /// Store bytes untouched; entry/cover extension comes from magic sniffing.
    #[default]
    None,
    /// Convert every decodable page to lossless WebP unconditionally — format
    /// uniformity is preferred over size (user decision), so the output MAY
    /// be larger than the source (typically for jpeg pages). Bytes already
    /// in WebP pass through.
    Lossless,
    /// Lossy WebP at `WEBP_QUALITY` — the pre-setting "unified page format"
    /// behaviour; the result is only adopted when actually smaller.
    Lossy,
}

impl PageCompression {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Lossless => "lossless",
            Self::Lossy => "lossy",
        }
    }

    /// Parse a DB/IPC string; returns None for unrecognized values (callers
    /// fall back to the `None` default, mirroring `locale::current_locale`).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "lossless" => Some(Self::Lossless),
            "lossy" => Some(Self::Lossy),
            _ => None,
        }
    }
}

/// Manages on-disk storage of CB7 files, covers, and cache.
pub struct StorageService {
    pub library_path: PathBuf,
    pub cover_path: PathBuf,
    /// Per-book zip handle cache: path -> opened archive + image entry indices.
    /// Reused across `read_page`/`count_pages` calls so a book's central
    /// directory is scanned at most once per session.
    page_cache: Mutex<HashMap<PathBuf, Arc<CachedArchive>>>,
}

impl StorageService {
    pub fn new(base_path: PathBuf) -> Self {
        let library_path = base_path.join("library");
        let cover_path = base_path.join("covers");

        // Best-effort creation; failures surface on first real IO.
        for p in [&library_path, &cover_path] {
            let _ = std::fs::create_dir_all(p);
        }

        Self {
            library_path,
            cover_path,
            page_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Create a CB7 (7-zip/ZIP) archive containing a ComicInfo.xml and the
    /// given images. Returns the path to the created file. Every page runs
    /// through `encode_page` per `compression`; ugoira frame sequences follow
    /// the same setting — `None` keeps per-frame jpeg quality and delay
    /// timings untouched.
    pub fn create_cb7(
        &self,
        images: &[Vec<u8>],
        metadata: &BookMetadata,
        compression: PageCompression,
    ) -> Result<PathBuf> {
        let book_id = Uuid::new_v4().to_string();
        let file_path = self.library_path.join(format!("{}.cb7", book_id));

        let file = std::fs::File::create(&file_path)?;
        let mut zip = ZipWriter::new(std::io::BufWriter::new(file));
        let options = FileOptions::default();

        // ComicInfo.xml
        let comic_info = create_comic_info(metadata);
        zip.start_file("ComicInfo.xml", options)?;
        zip.write_all(comic_info.as_bytes())?;

        // Images, named sequentially.
        for (index, image) in images.iter().enumerate() {
            let image = Self::encode_page(image, compression);
            let ext = guess_image_extension(&image);
            let filename = format!("{:04}.{}", index + 1, ext);
            zip.start_file(&filename, options)?;
            zip.write_all(&image)?;
        }

        zip.finish()?;
        Ok(file_path)
    }

    /// Extract the first image from a CB7 archive as the cover, encoded per
    /// `compression` and written at the magic-sniffed extension of the result
    /// bytes (see `write_cover`).
    pub fn extract_cover(
        &self,
        cb7_path: &Path,
        book_id: &str,
        compression: PageCompression,
    ) -> Result<PathBuf> {
        let file = std::fs::File::open(cb7_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_lowercase();
            if name.ends_with(".jpg")
                || name.ends_with(".jpeg")
                || name.ends_with(".png")
                || name.ends_with(".webp")
                || name.ends_with(".avif")
            {
                let mut buffer = Vec::new();
                entry.read_to_end(&mut buffer)?;
                return self.write_cover(book_id, &buffer, compression);
            }
        }

        anyhow::bail!("No image found in archive: {}", cb7_path.display())
    }

    /// Encode cover bytes per `compression` and write them at the
    /// magic-sniffed extension of the RESULT bytes (the lossy mode's
    /// smaller-only guard can fall back to the source format, so never
    /// assume `.webp`). Any cover file of a DIFFERENT extension for this
    /// book is removed first — the read paths probe `.webp` first, so a
    /// stale file left over from a previous compression mode would otherwise
    /// permanently shadow the new one.
    pub fn write_cover(
        &self,
        book_id: &str,
        bytes: &[u8],
        compression: PageCompression,
    ) -> Result<PathBuf> {
        let data = Self::encode_page(bytes, compression);
        let ext = guess_image_extension(&data);
        for other in ["webp", "jpg", "jpeg", "png", "avif"] {
            if other != ext {
                let _ = std::fs::remove_file(self.cover_path.join(format!("{book_id}.{other}")));
            }
        }
        let path = self.cover_path.join(format!("{book_id}.{ext}"));
        std::fs::write(&path, &data)?;
        Ok(path)
    }

    /// Delete a book's CB7 file and cover.
    pub fn delete_book(&self, file_path: &Path, book_id: &str) -> Result<()> {
        // Drop any cached open archive handle for this file before removing it
        // — the cached File descriptor would keep the file open and block
        // deletion on Windows (sharing violation). No-op when not cached.
        let _ = self.page_cache.lock().map(|mut c| c.remove(file_path));
        if file_path.exists() {
            std::fs::remove_file(file_path)?;
        }
        for ext in &["jpg", "jpeg", "png", "webp", "avif"] {
            let cover = self.cover_path.join(format!("{}.{}", book_id, ext));
            if cover.exists() {
                std::fs::remove_file(cover)?;
            }
        }
        Ok(())
    }

    /// Read a cover file into bytes for serving over OPDS / the frontend.
    pub fn read_cover(&self, book_id: &str) -> Option<Vec<u8>> {
        for ext in &["webp", "jpg", "jpeg", "png", "avif"] {
            let cover = self.cover_path.join(format!("{}.{}", book_id, ext));
            if let Ok(data) = std::fs::read(&cover) {
                return Some(data);
            }
        }
        None
    }

    /// Read a cover and downscale it to a small JPEG thumbnail (longest edge ≤
    /// `max_edge`). The library grid ships this over IPC instead of the
    /// full-res cover — a few KB vs potentially MBs — and the frontend caches
    /// it in IndexedDB. Falls back to the original bytes if decoding fails so
    /// the caller still has something to display.
    pub fn read_cover_thumb(&self, book_id: &str, max_edge: u32) -> Option<Vec<u8>> {
        let raw = self.read_cover(book_id)?;
        Some(Self::shrink_to_jpeg(&raw, max_edge))
    }

    /// Read `ComicInfo.xml` out of a CB7/CBZ archive and parse it back into
    /// BookMetadata, so an imported cb7 recovers its title / tags / source /
    /// delays. Returns None for archives without ComicInfo.xml (cbr/pdf, or
    /// cbz from tools that don't write one).
    pub fn read_comic_info(&self, cb7_path: &Path) -> Option<BookMetadata> {
        let file = std::fs::File::open(cb7_path).ok()?;
        let mut archive = zip::ZipArchive::new(file).ok()?;
        let mut xml = String::new();
        for i in 0..archive.len() {
            let Ok(mut entry) = archive.by_index(i) else {
                continue;
            };
            if entry.name() == "ComicInfo.xml" {
                entry.read_to_string(&mut xml).ok()?;
                break;
            }
        }
        if xml.is_empty() {
            return None;
        }
        parse_comic_info(&xml)
    }

    /// Encode one page image per the library compression mode.
    ///
    /// Shared passthrough guards (a page is never dropped): `None` returns
    /// the bytes untouched; sources already in WebP pass through in every
    /// mode (the format is already unified — re-encoding a lossy webp to
    /// lossless would only balloon it); AVIF passes through with a warn (no
    /// decoder since dav1d was dropped); undecodable bytes pass through with
    /// a warn. `Lossless` converts unconditionally — format uniformity is
    /// preferred over size (user decision), so the output MAY be larger than
    /// the source. `Lossy` keeps the D-021 guard: the q80 result is only
    /// adopted when actually smaller than the source.
    pub(crate) fn encode_page(raw: &[u8], mode: PageCompression) -> Vec<u8> {
        if mode == PageCompression::None {
            return raw.to_vec();
        }
        // WebP RIFF container: "RIFF" + size + "WEBP".
        if raw.len() >= 12 && raw.starts_with(b"RIFF") && raw[8..12] == *b"WEBP" {
            return raw.to_vec();
        }
        // AVIF: no decoder available — warn and keep the bytes verbatim so a
        // page is never silently dropped from a book.
        if guess_image_extension(raw) == "avif" {
            tracing::warn!(len = raw.len(), "AVIF page encountered but no decoder available; storing as-is");
            return raw.to_vec();
        }
        let Ok(img) = image::load_from_memory(raw) else {
            tracing::warn!(len = raw.len(), "page decode failed; storing as-is");
            return raw.to_vec();
        };
        let rgba = img.to_rgba8();
        let encoder = webp::Encoder::from_rgba(rgba.as_raw(), img.width(), img.height());
        let (out, adopt_only_if_smaller) = match mode {
            PageCompression::Lossless => (encoder.encode_lossless(), false),
            PageCompression::Lossy => (encoder.encode(WEBP_QUALITY), true),
            PageCompression::None => unreachable!("handled above"),
        };
        if out.is_empty() || (adopt_only_if_smaller && out.len() >= raw.len()) {
            // Empty output is an encoder failure; for the lossy mode a
            // non-smaller result means the source was already as compact as
            // q80 webp — keep it. (Lossless adopts unconditionally: format
            // uniformity is preferred over size.)
            return raw.to_vec();
        }
        out.to_vec()
    }

    /// Decode `raw` (jpg/png/webp) and re-encode as a JPEG whose longest edge
    /// is ≤ `max_edge` (aspect ratio preserved). Returns the original bytes on
    /// any decode/encode failure or when the format is undecodable (e.g. AVIF).
    fn shrink_to_jpeg(raw: &[u8], max_edge: u32) -> Vec<u8> {
        use image::imageops::FilterType;
        use std::io::Cursor;
        // AVIF: no decoder available — return the original bytes.
        if guess_image_extension(raw) == "avif" {
            return raw.to_vec();
        }
        let Ok(img) = image::load_from_memory(raw) else {
            return raw.to_vec();
        };
        let scaled = if img.width() > max_edge || img.height() > max_edge {
            img.resize(max_edge, max_edge, FilterType::Triangle)
        } else {
            img
        };
        let mut buf = Cursor::new(Vec::new());
        if scaled
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .is_ok()
        {
            buf.into_inner()
        } else {
            raw.to_vec()
        }
    }

    /// Read every image page from a CB7/CBZ archive, in reading order.
    /// The bulk read behind single-page deletion — callers rewrite the
    /// archive minus the dropped page, so everything must be in memory anyway.
    pub fn read_all_pages(&self, cb7_path: &Path) -> Result<Vec<Vec<u8>>> {
        let cached = self
            .archive_for(cb7_path)
            .ok_or_else(|| anyhow::anyhow!("cannot open archive: {}", cb7_path.display()))?;
        let entry_indices = cached.image_indices.clone();
        let mut guard = cached.archive.lock().map_err(|_| {
            anyhow::anyhow!("archive handle poisoned: {}", cb7_path.display())
        })?;
        let mut pages = Vec::with_capacity(entry_indices.len());
        for idx in entry_indices {
            let mut entry = guard.by_index(idx)?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            pages.push(buf);
        }
        Ok(pages)
    }

    /// Physically remove one page (0-based) from a CB7/CBZ archive.
    ///
    /// Zip has no delete, so this repacks every remaining page into a temp
    /// file next to the original and renames it over. ComicInfo.xml is carried
    /// over via read_comic_info so title/tags/source/delays round-trip; when
    /// absent a bare-bones metadata keeps the archive still importable.
    /// Returns the new page count.
    pub fn rewrite_without_page(
        &self,
        cb7_path: &Path,
        drop_page: usize,
        compression: PageCompression,
    ) -> Result<usize> {
        // Drop any cached open handle for this path BEFORE reading — the cached
        // File descriptor would keep pointing at the old inode after the rename
        // below, so a later read_page() would silently serve pre-delete content.
        // `archive_for` re-opens + re-caches on the next page access after the
        // file is replaced, so invalidating here is safe and required.
        let _ = self.page_cache.lock().map(|mut c| c.remove(cb7_path));

        let mut pages = self.read_all_pages(cb7_path)?;
        if drop_page >= pages.len() {
            anyhow::bail!(
                "page {drop_page} out of range ({} pages) in {}",
                pages.len(),
                cb7_path.display()
            );
        }
        pages.remove(drop_page);
        if pages.is_empty() {
            anyhow::bail!("refusing to delete the last page of {}", cb7_path.display());
        }

        // Read ComicInfo only AFTER we've dropped the per-path cache and BEFORE
        // the rename, so it comes from the current (to-be-replaced) file, never
        // a stale cached handle.
        let metadata = self.read_comic_info(cb7_path).unwrap_or_else(|| BookMetadata {
            title: cb7_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_string(),
            ..BookMetadata::default()
        });

        // Write the replacement beside the original, then rename over it —
        // a crash mid-write leaves the original intact.
        let tmp_path = cb7_path.with_extension("cb7.tmp");
        {
            let file = std::fs::File::create(&tmp_path)?;
            let mut zip = ZipWriter::new(std::io::BufWriter::new(file));
            let options = FileOptions::default();

            zip.start_file("ComicInfo.xml", options)?;
            zip.write_all(create_comic_info(&metadata).as_bytes())?;

            for (index, image) in pages.iter().enumerate() {
                // Re-encode per the current compression mode: a webp-mode
                // repack stays normalized, while `None` keeps every remaining
                // byte untouched (a page deletion never silently re-encodes a
                // whole book).
                let image = Self::encode_page(image, compression);
                let ext = guess_image_extension(&image);
                let filename = format!("{:04}.{}", index + 1, ext);
                zip.start_file(&filename, options)?;
                zip.write_all(&image)?;
            }

            zip.finish()?;
        }
        std::fs::rename(&tmp_path, cb7_path)?;
        // read_all_pages → archive_for re-populated the cache with a handle to
        // the pre-rename file; that File now points at the replaced inode and
        // its image_indices are one page too long. Evict it so the next page
        // access re-opens the repacked archive fresh.
        let _ = self.page_cache.lock().map(|mut c| c.remove(cb7_path));
        Ok(pages.len())
    }

    /// Extract a single page image from a CB7/CBZ archive by index.
    ///
    /// Pages are the image entries within the zip (filtered by extension), in
    /// archive (zip entry) order. `page` is 0-based. CB7s written by this app
    /// name pages `0001.ext`, `0002.ext`, … so zip order already matches reading
    /// order; for CBZs the order is whatever stored the entries.
    pub fn read_page(&self, cb7_path: &Path, page: usize) -> Option<Vec<u8>> {
        let cached = self.archive_for(cb7_path)?;
        let entry_idx = *cached.image_indices.get(page)?;
        let mut guard = cached.archive.lock().ok()?;
        let mut entry = guard.by_index(entry_idx).ok()?;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    /// Total image pages in a CB7/CBZ archive.
    pub fn count_pages(&self, cb7_path: &Path) -> Option<usize> {
        let cached = self.archive_for(cb7_path)?;
        Some(cached.image_indices.len())
    }

    /// Return a cached, opened archive for `cb7_path` along with the
    /// precomputed list of image-entry indices. On miss: open the file, build
    /// the ZipArchive, scan the central directory once for image entries, then
    /// stash it in the cache (evicting an arbitrary entry past the cap). The
    /// open + scan happen outside the cache lock so concurrent page reads for
    /// different books don't serialize on it.
    fn archive_for(&self, cb7_path: &Path) -> Option<Arc<CachedArchive>> {
        // Fast path: already cached.
        {
            let map = self.page_cache.lock().ok()?;
            if let Some(arc) = map.get(cb7_path) {
                return Some(arc.clone());
            }
        }

        // Cache miss: open + scan the central directory once.
        let file = std::fs::File::open(cb7_path).ok()?;
        let mut archive = zip::ZipArchive::new(file).ok()?;
        let image_exts = [".jpg", ".jpeg", ".png", ".webp", ".avif"];
        let mut image_indices: Vec<usize> = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let is_img = archive
                .by_index(i)
                .map(|e| {
                    let n = e.name().to_lowercase();
                    image_exts.iter().any(|ext| n.ends_with(ext))
                })
                .unwrap_or(false);
            if is_img {
                image_indices.push(i);
            }
        }
        let cached = Arc::new(CachedArchive {
            archive: Mutex::new(archive),
            image_indices,
        });

        // Re-check under the write lock: another thread may have inserted the
        // same path while we built this one. If so, drop ours and reuse theirs.
        let mut map = self.page_cache.lock().ok()?;
        if let Some(existing) = map.get(cb7_path) {
            return Some(existing.clone());
        }
        if map.len() >= PAGE_CACHE_MAX {
            // Evict an arbitrary entry (the current path is not yet in the map,
            // so whatever we remove is safe).
            if let Some(key) = map.keys().next().cloned() {
                map.remove(&key);
            }
        }
        map.insert(cb7_path.to_path_buf(), cached.clone());
        Some(cached)
    }
}

pub fn create_comic_info(metadata: &BookMetadata) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    s.push_str(
        "<ComicInfo xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         xmlns:ero=\"https://xrl.im/erolib\">\n",
    );
    s.push_str(&format!(
        "  <Title>{}</Title>\n",
        xml_escape(&metadata.title)
    ));
    s.push_str(&format!(
        "  <Writer>{}</Writer>\n",
        xml_escape(metadata.author.as_deref().unwrap_or(""))
    ));
    s.push_str(&format!(
        "  <Penciller>{}</Penciller>\n",
        xml_escape(metadata.artist.as_deref().unwrap_or(""))
    ));
    s.push_str(&format!(
        "  <Summary>{}</Summary>\n",
        xml_escape(metadata.description.as_deref().unwrap_or(""))
    ));
    s.push_str(&format!(
        "  <Tags>{}</Tags>\n",
        xml_escape(&metadata.tags.join(", "))
    ));
    // erolib provenance under a custom namespace — standard ComicInfo readers
    // ignore unknown elements, but our own importer reads them back so an
    // exported cb7 round-trips losslessly.
    s.push_str(&format!(
        "  <ero:SourcePlugin>{}</ero:SourcePlugin>\n",
        xml_escape(metadata.source_plugin.as_deref().unwrap_or(""))
    ));
    s.push_str(&format!(
        "  <ero:SourceURL>{}</ero:SourceURL>\n",
        xml_escape(metadata.source_url.as_deref().unwrap_or(""))
    ));
    s.push_str(&format!(
        "  <ero:SourcePostID>{}</ero:SourcePostID>\n",
        xml_escape(metadata.source_post_id.as_deref().unwrap_or(""))
    ));
    s.push_str(&format!(
        "  <ero:PublishedAt>{}</ero:PublishedAt>\n",
        xml_escape(metadata.published_at.as_deref().unwrap_or(""))
    ));
    s.push_str(&format!(
        "  <ero:ScrapedAt>{}</ero:ScrapedAt>\n",
        xml_escape(metadata.scraped_at.as_deref().unwrap_or(""))
    ));
    s.push_str(&format!(
        "  <ero:Delays>{}</ero:Delays>\n",
        xml_escape(metadata.delays.as_deref().unwrap_or(""))
    ));
    s.push_str("</ComicInfo>");
    s
}

/// Parse a ComicInfo.xml string back into BookMetadata. Standard fields map to
/// the obvious slots; erolib provenance lives under the `ero:` namespace.
/// Returns None when no `<Title>` text was found (nothing useful to register).
fn parse_comic_info(xml: &str) -> Option<BookMetadata> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut cur: String = String::new();
    let mut meta = BookMetadata::default();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                cur = String::from_utf8_lossy(e.name().as_ref()).into_owned();
            }
            Ok(Event::Empty(_)) => {
                // Self-closed element carries no text content; clear the cursor
                // so a stray Text event can't attach to the wrong field.
                cur.clear();
            }
            Ok(Event::Text(t)) => {
                let Ok(text) = t.unescape() else {
                    continue;
                };
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                match cur.as_str() {
                    "Title" => meta.title = text.to_string(),
                    "Writer" => meta.author = Some(text.to_string()),
                    "Penciller" => meta.artist = Some(text.to_string()),
                    "Summary" => meta.description = Some(text.to_string()),
                    "Tags" => {
                        meta.tags = text
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    "ero:SourcePlugin" => meta.source_plugin = Some(text.to_string()),
                    "ero:SourceURL" => meta.source_url = Some(text.to_string()),
                    "ero:SourcePostID" => meta.source_post_id = Some(text.to_string()),
                    "ero:PublishedAt" => meta.published_at = Some(text.to_string()),
                    "ero:ScrapedAt" => meta.scraped_at = Some(text.to_string()),
                    "ero:Delays" => meta.delays = Some(text.to_string()),
                    _ => {}
                }
            }
            Ok(Event::End(_)) => cur.clear(),
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    if meta.title.is_empty() {
        None
    } else {
        Some(meta)
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Guess the image extension from magic bytes.
pub fn guess_image_extension(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 4 {
        if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return "png";
        }
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return "jpg";
        }
        if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && bytes[8..12] == *b"WEBP" {
            return "webp";
        }
        // ISO-BMFF "ftyp" box: AVIF (still present in pre-migration books)
        // or AVIF sequence.
        if bytes.len() >= 12 && bytes[4..8] == *b"ftyp" {
            let brand = &bytes[8..12];
            if brand == b"avif" || brand == b"avis" {
                return "avif";
            }
        }
    }
    "jpg"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_png() -> Vec<u8> {
        // 64×64 horizontal gradient — a real, decodable PNG fixture.
        let img = image::RgbaImage::from_fn(64, 64, |x, _| {
            image::Rgba([(x * 4) as u8, 128, 255 - (x * 4) as u8, 255])
        });
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("encode png fixture");
        buf.into_inner()
    }

    fn tiny_jpeg() -> Vec<u8> {
        let img = image::RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([(x * 4) as u8, (y * 4) as u8, 64, 255])
        });
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .expect("encode jpeg fixture");
        buf.into_inner()
    }

    fn is_webp(b: &[u8]) -> bool {
        b.len() >= 12 && b.starts_with(b"RIFF") && b[8..12] == *b"WEBP"
    }

    fn temp_storage() -> (StorageService, PathBuf) {
        let dir = std::env::temp_dir().join(format!("erolib-storage-test-{}", Uuid::new_v4()));
        (StorageService::new(dir.clone()), dir)
    }

    #[test]
    fn page_compression_str_roundtrip() {
        for m in [
            PageCompression::None,
            PageCompression::Lossless,
            PageCompression::Lossy,
        ] {
            assert_eq!(PageCompression::parse(m.as_str()), Some(m));
        }
        assert_eq!(PageCompression::parse("bogus"), None);
        assert_eq!(PageCompression::default(), PageCompression::None);
    }

    #[test]
    fn encode_page_none_is_identity() {
        for input in [tiny_png(), tiny_jpeg(), b"not an image".to_vec()] {
            assert_eq!(
                StorageService::encode_page(&input, PageCompression::None),
                input,
                "None mode must return the bytes untouched"
            );
        }
    }

    #[test]
    fn encode_page_lossless_png_becomes_webp() {
        let png = tiny_png();
        let out = StorageService::encode_page(&png, PageCompression::Lossless);
        assert!(is_webp(&out), "lossless mode must always produce webp");
        let img = image::load_from_memory(&out).expect("decode lossless webp");
        assert_eq!((img.width(), img.height()), (64, 64));
    }

    #[test]
    fn encode_page_lossless_jpeg_converts_unconditionally() {
        // User decision: lossless = format uniformity over size — a jpeg page
        // becomes lossless webp even though the output is usually LARGER.
        let jpg = tiny_jpeg();
        let out = StorageService::encode_page(&jpg, PageCompression::Lossless);
        assert!(is_webp(&out), "lossless mode must always produce webp");
        let img = image::load_from_memory(&out).expect("decode lossless webp");
        assert_eq!((img.width(), img.height()), (64, 64));
    }

    #[test]
    fn encode_page_lossy_never_grows_and_keeps_garbage() {
        let jpg = tiny_jpeg();
        let out = StorageService::encode_page(&jpg, PageCompression::Lossy);
        assert!(out.len() <= jpg.len(), "lossy output must never exceed source");
        // Either converted (smaller webp) or passed through unchanged.
        assert!(is_webp(&out) || out == jpg);
        // Undecodable bytes pass through in every mode (a page is never dropped).
        let junk = b"garbage bytes".to_vec();
        assert_eq!(
            StorageService::encode_page(&junk, PageCompression::Lossy),
            junk
        );
        assert_eq!(
            StorageService::encode_page(&junk, PageCompression::Lossless),
            junk
        );
    }

    #[test]
    fn create_cb7_none_preserves_formats_and_bytes() {
        let (storage, _dir) = temp_storage();
        let jpg = tiny_jpeg();
        let png = tiny_png();
        let meta = BookMetadata {
            title: "t".into(),
            ..Default::default()
        };

        let path = storage
            .create_cb7(&[jpg.clone(), png.clone()], &meta, PageCompression::None)
            .expect("create cb7 (none)");
        let zip = std::fs::File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(zip).unwrap();
        let mut names: Vec<String> = zip.file_names().map(str::to_string).collect();
        names.sort();
        assert_eq!(
            names,
            vec!["0001.jpg".to_string(), "0002.png".to_string(), "ComicInfo.xml".to_string()]
        );
        let mut buf = Vec::new();
        zip.by_name("0001.jpg").unwrap().read_to_end(&mut buf).unwrap();
        assert_eq!(buf, jpg, "None mode must store the original bytes");

        // Lossless converts everything deterministically (unconditional).
        let path = storage
            .create_cb7(&[jpg, png], &meta, PageCompression::Lossless)
            .expect("create cb7 (lossless)");
        let zip = std::fs::File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(zip).unwrap();
        let mut buf = Vec::new();
        zip.by_name("0001.webp").unwrap().read_to_end(&mut buf).unwrap();
        assert!(is_webp(&buf));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_cover_replaces_stale_other_extension() {
        let (storage, _dir) = temp_storage();
        let id = Uuid::new_v4().to_string();
        let jpg = tiny_jpeg();

        // Lossless era: cover lands at {id}.webp (unconditional conversion).
        let webp_cover = storage
            .write_cover(&id, &jpg, PageCompression::Lossless)
            .expect("write webp cover");
        assert!(webp_cover.extension().is_some_and(|e| e == "webp"));

        // Switch to None: the cover must move to {id}.jpg and the stale
        // {id}.webp (which read_cover would probe FIRST) must be gone.
        let jpg_cover = storage
            .write_cover(&id, &jpg, PageCompression::None)
            .expect("write jpg cover");
        assert!(jpg_cover.extension().is_some_and(|e| e == "jpg"));
        assert!(!storage.cover_path.join(format!("{id}.webp")).exists());
        assert_eq!(storage.read_cover(&id), Some(jpg));
    }

    #[test]
    fn rewrite_without_page_none_keeps_remaining_bytes() {
        let (storage, _dir) = temp_storage();
        let jpg = tiny_jpeg();
        let png = tiny_png();
        let meta = BookMetadata {
            title: "t".into(),
            ..Default::default()
        };

        let path = storage
            .create_cb7(&[jpg, png.clone()], &meta, PageCompression::None)
            .expect("create cb7 (none)");
        let count = storage
            .rewrite_without_page(&path, 0, PageCompression::None)
            .expect("rewrite without page");
        assert_eq!(count, 1);
        // The surviving page must be byte-identical to what went in — a
        // None-mode repack must not re-encode anything.
        assert_eq!(storage.read_page(&path, 0), Some(png));
        let _ = std::fs::remove_file(&path);
    }
}
