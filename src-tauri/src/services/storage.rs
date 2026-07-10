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
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<ComicInfo xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <Title>{}</Title>
  <Writer>{}</Writer>
  <Penciller>{}</Penciller>
  <Summary>{}</Summary>
  <Tags>{}</Tags>
</ComicInfo>"#,
        xml_escape(&metadata.title),
        xml_escape(metadata.author.as_deref().unwrap_or("")),
        xml_escape(metadata.artist.as_deref().unwrap_or("")),
        xml_escape(metadata.description.as_deref().unwrap_or("")),
        xml_escape(&metadata.tags.join(", ")),
    )
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
