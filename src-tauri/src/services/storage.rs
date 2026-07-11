use std::path::{Path, PathBuf};

use anyhow::Result;
use uuid::Uuid;
use zip::{write::FileOptions, ZipWriter};

use crate::models::BookMetadata;
use std::io::{Read, Write};

/// Manages on-disk storage of CB7 files, covers, and cache.
pub struct StorageService {
    pub library_path: PathBuf,
    #[allow(dead_code)]
    pub cache_path: PathBuf,
    pub cover_path: PathBuf,
}

impl StorageService {
    pub fn new(base_path: PathBuf) -> Self {
        let library_path = base_path.join("library");
        let cache_path = base_path.join("cache");
        let cover_path = base_path.join("covers");

        // Best-effort creation; failures surface on first real IO.
        for p in [&library_path, &cache_path, &cover_path] {
            let _ = std::fs::create_dir_all(p);
        }

        Self {
            library_path,
            cache_path,
            cover_path,
        }
    }

    /// Create a CB7 (7-zip/ZIP) archive containing a ComicInfo.xml and the
    /// given images. Returns the path to the created file.
    pub fn create_cb7(
        &self,
        images: &[Vec<u8>],
        metadata: &BookMetadata,
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
            let ext = guess_image_extension(image);
            let filename = format!("{:04}.{}", index + 1, ext);
            zip.start_file(&filename, options)?;
            zip.write_all(image)?;
        }

        zip.finish()?;
        Ok(file_path)
    }

    /// Extract the first image from a CB7 archive as the cover.
    pub fn extract_cover(&self, cb7_path: &Path, book_id: &str) -> Result<PathBuf> {
        let cover_path = self.cover_path.join(format!("{}.jpg", book_id));

        let file = std::fs::File::open(cb7_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_lowercase();
            if name.ends_with(".jpg")
                || name.ends_with(".jpeg")
                || name.ends_with(".png")
                || name.ends_with(".webp")
            {
                let mut buffer = Vec::new();
                entry.read_to_end(&mut buffer)?;
                std::fs::write(&cover_path, &buffer)?;
                return Ok(cover_path);
            }
        }

        anyhow::bail!("No image found in archive: {}", cb7_path.display())
    }

    /// Delete a book's CB7 file and cover.
    pub fn delete_book(&self, file_path: &Path, book_id: &str) -> Result<()> {
        if file_path.exists() {
            std::fs::remove_file(file_path)?;
        }
        for ext in &["jpg", "jpeg", "png", "webp"] {
            let cover = self.cover_path.join(format!("{}.{}", book_id, ext));
            if cover.exists() {
                std::fs::remove_file(cover)?;
            }
        }
        Ok(())
    }

    /// Read a cover file into bytes for serving over OPDS / the frontend.
    pub fn read_cover(&self, book_id: &str) -> Option<Vec<u8>> {
        for ext in &["jpg", "jpeg", "png", "webp"] {
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

    /// Decode `raw` (jpg/png/webp) and re-encode as a JPEG whose longest edge
    /// is ≤ `max_edge` (aspect ratio preserved). Returns the original bytes on
    /// any decode/encode failure.
    fn shrink_to_jpeg(raw: &[u8], max_edge: u32) -> Vec<u8> {
        use image::imageops::FilterType;
        use std::io::Cursor;
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

    /// Extract a single page image from a CB7/CBZ archive by index.
    ///
    /// Pages are the image entries within the zip (filtered by extension), in
    /// archive (zip entry) order. `page` is 0-based. CB7s written by this app
    /// name pages `0001.ext`, `0002.ext`, … so zip order already matches reading
    /// order; for CBZs the order is whatever stored the entries.
    pub fn read_page(&self, cb7_path: &Path, page: usize) -> Option<Vec<u8>> {
        let file = std::fs::File::open(cb7_path).ok()?;
        let mut archive = zip::ZipArchive::new(file).ok()?;

        // Collect image entries in zip order, then pick by index.
        let image_exts = [".jpg", ".jpeg", ".png", ".webp"];
        let indices: Vec<usize> = (0..archive.len())
            .filter(|&i| {
                archive
                    .by_index(i)
                    .map(|e| {
                        let n = e.name().to_lowercase();
                        image_exts.iter().any(|ext| n.ends_with(ext))
                    })
                    .unwrap_or(false)
            })
            .collect();

        let entry_idx = *indices.get(page)?;
        let mut entry = archive.by_index(entry_idx).ok()?;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    /// Total image pages in a CB7/CBZ archive.
    pub fn count_pages(&self, cb7_path: &Path) -> Option<usize> {
        let file = std::fs::File::open(cb7_path).ok()?;
        let mut archive = zip::ZipArchive::new(file).ok()?;
        let image_exts = [".jpg", ".jpeg", ".png", ".webp"];
        let count = (0..archive.len())
            .filter(|&i| {
                archive
                    .by_index(i)
                    .map(|e| {
                        let n = e.name().to_lowercase();
                        image_exts.iter().any(|ext| n.ends_with(ext))
                    })
                    .unwrap_or(false)
            })
            .count();
        Some(count)
    }
}

fn create_comic_info(metadata: &BookMetadata) -> String {
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
fn guess_image_extension(bytes: &[u8]) -> &'static str {
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
    }
    "jpg"
}
