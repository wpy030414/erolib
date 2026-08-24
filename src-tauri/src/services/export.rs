//! Format conversion for library export: a stored CB7 (zip image sequence +
//! ComicInfo.xml) is repackaged into cb7 / epub / pdf at a user-chosen path.
//!
//! All three formats carry erolib provenance (source plugin/url/post id,
//! published/scraped timestamps, tags, delays) so an exported book round-trips
//! back through `import_book` with its metadata intact:
//!
//! - **cb7**: ComicInfo.xml (reused from `storage::create_comic_info`).
//! - **epub**: standard OPF `<dc:*>` fields + `<meta>` refines under the
//!   `ero:` property namespace; images embedded verbatim.
//! - **pdf**: the Info dictionary (Title/Author/Subject/Keywords) + a custom
//!   `ErolibMetadata` metadata stream carrying the JSON record.
//!
//! Pages are the image entries of the source archive; `read_all_pages` already
//! yields them in reading order. No image is re-encoded unless the target
//! format can't embed it verbatim (PDF can't take webp/png/avif as a raw DCT
//! stream, so non-JPEG images are decoded to RGB and laid down uncompressed;
//! alpha is composited onto white — see `build_image_xobject`).

use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use zip::{write::FileOptions, CompressionMethod, ZipWriter};

use crate::models::BookMetadata;
use crate::services::storage::{create_comic_info, guess_image_extension};

/// Export the given image pages (in reading order) and metadata to `dest` in the
/// requested format. `format` is lowercase (`cb7`/`epub`/`pdf`). `on_page`
/// fires after each page is written with the 0-based page index — the caller
/// uses it to drive the export progress bar.
pub fn export_book(
    images: &[Vec<u8>],
    metadata: &BookMetadata,
    dest: &Path,
    format: &str,
    on_page: &mut dyn FnMut(usize),
) -> Result<()> {
    match format {
        "cb7" => export_cb7(images, metadata, dest, on_page),
        "epub" => export_epub(images, metadata, dest, on_page),
        "pdf" => export_pdf(images, metadata, dest, on_page),
        other => Err(anyhow!("unsupported export format: {other}")),
    }
}

/// Repack the pages into a CB7 with a fresh ComicInfo.xml. The provenance round-
/// trips because `create_comic_info` serializes every `BookMetadata` field.
fn export_cb7(
    images: &[Vec<u8>],
    metadata: &BookMetadata,
    dest: &Path,
    on_page: &mut dyn FnMut(usize),
) -> Result<()> {
    ensure_parent(dest)?;
    let file = std::fs::File::create(dest).context("create cb7")?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let options = FileOptions::default();

    zip.start_file("ComicInfo.xml", options)?;
    zip.write_all(create_comic_info(metadata).as_bytes())?;

    for (index, image) in images.iter().enumerate() {
        let ext = guess_image_extension(image);
        zip.start_file(format!("{:04}.{}", index + 1, ext), options)?;
        zip.write_all(image)?;
        on_page(index);
    }
    zip.finish().context("finish cb7")?;
    Ok(())
}

/// Minimal valid EPUB 2/3: `mimetype` (stored, first), `META-INF/container.xml`,
/// `OEBPS/content.opf`, a `toc.ncx`, and one XHTML page per image with the
/// image embedded verbatim under `OEBPS/images/`. Standard `dc:` fields carry
/// title/author/subject(tags)/description; the erolib provenance fields live as
/// `<meta>` refines (EPUB 3) under the `ero:` property namespace.
fn export_epub(
    images: &[Vec<u8>],
    metadata: &BookMetadata,
    dest: &Path,
    on_page: &mut dyn FnMut(usize),
) -> Result<()> {
    ensure_parent(dest)?;
    let file = std::fs::File::create(dest).context("create epub")?;
    let mut zip = ZipWriter::new(BufWriter::new(file));

    // mimetype must be the first entry and stored (uncompressed) per the EPUB
    // spec — readers reject the file otherwise.
    zip.start_file(
        "mimetype",
        FileOptions::default().compression_method(CompressionMethod::Stored),
    )?;
    zip.write_all(b"application/epub+zip")?;

    zip.start_file("META-INF/container.xml", FileOptions::default())?;
    zip.write_all(CONTAINER_XML.as_bytes())?;

    // Images first (raw bytes, no re-encoding), then the XHTML that references
    // them. `guess_image_extension` names them so the XHTML <img src> matches.
    let mut img_paths: Vec<String> = Vec::with_capacity(images.len());
    for (index, image) in images.iter().enumerate() {
        let ext = guess_image_extension(image);
        let path = format!("images/{:04}.{}", index + 1, ext);
        zip.start_file(format!("OEBPS/{path}"), FileOptions::default())?;
        zip.write_all(image)?;
        img_paths.push(path);
        on_page(index);
    }

    // OPF references the real image paths/extensions, so build it after the
    // images are named.
    zip.start_file("OEBPS/content.opf", FileOptions::default())?;
    zip.write_all(build_opf(metadata, &img_paths).as_bytes())?;

    zip.start_file("OEBPS/toc.ncx", FileOptions::default())?;
    zip.write_all(build_ncx(metadata, img_paths.len()).as_bytes())?;

    for (index, img_path) in img_paths.iter().enumerate() {
        zip.start_file(format!("OEBPS/page-{:04}.xhtml", index + 1), FileOptions::default())?;
        zip.write_all(build_xhtml(index + 1, img_path).as_bytes())?;
    }

    zip.finish().context("finish epub")?;
    Ok(())
}

/// PDF: one page per image, page sized to the image (px → pt at 96 dpi so 1px ≈
/// 1pt is close enough for a reader to page through). JPEGs ride the DCT stream
/// verbatim (no re-encode); PNG/WEBP/AVIF decode to RGB and lay down raw (alpha
/// composited onto white — see `build_image_xobject`). The Info dict carries
/// Title/Author/Subject/Keywords; an `ErolibMetadata` stream holds the full
/// JSON record so import can recover provenance the Info dict can't represent
/// (source url, post id, scraped_at, delays).
fn export_pdf(
    images: &[Vec<u8>],
    metadata: &BookMetadata,
    dest: &Path,
    on_page: &mut dyn FnMut(usize),
) -> Result<()> {
    // NOTE: do NOT `use printpdf::*` here — printpdf re-exports its own
    // `image` module (`pub mod image` + `pub use crate::image::*`), whose glob
    // would shadow the external `image` crate and break `image::load_from_memory`.
    // Pin printpdf paths explicitly instead.
    use printpdf::{Image, ImageTransform, Mm, PdfDocument};

    ensure_parent(dest)?;
    // `PdfDocument::new` seeds a blank initial page (which would leave every
    // export with one stray empty page); `empty` starts with zero pages.
    let doc = PdfDocument::empty(metadata.title.clone());

    for (index, page_bytes) in images.iter().enumerate() {
        // build_image_xobject also reports pixel dimensions so the page can
        // be sized to the image (1px ≈ 1pt at 96 dpi → mm = px * 25.4 / 96).
        let (xobj, w, h) = build_image_xobject(page_bytes)?;
        let w_mm = Mm(w as f32 * 25.4 / 96.0);
        let h_mm = Mm(h as f32 * 25.4 / 96.0);
        let (page, layer) = doc.add_page(w_mm, h_mm, "page");

        let img = Image::from(xobj);
        // 96 dpi makes 1px ≈ 1pt, so the image fills the page edge to edge.
        img.add_to_layer(
            doc.get_page(page).get_layer(layer),
            ImageTransform {
                dpi: Some(96.0),
                ..Default::default()
            },
        );
        on_page(index);
    }

    // Standard Info-dict fields every PDF reader shows in document properties.
    let doc = doc
        .with_title(&metadata.title)
        .with_author(metadata.author.as_deref().unwrap_or(""))
        .with_subject(metadata.description.as_deref().unwrap_or(""))
        .with_keywords(metadata.tags.clone());

    // Full provenance JSON, recoverable on import via lopdf. The Info dict
    // alone can't carry source_url/post_id/scraped_at/delays, so stash the
    // whole record in a named stream and link it from Catalog Metadata.
    let meta_json = serde_json::to_string(metadata).unwrap_or_default();
    let mut buf = BufWriter::new(std::fs::File::create(dest).context("create pdf")?);
    doc.save(&mut buf).context("save pdf")?;
    let _ = buf.flush();

    // Reopen with lopdf to graft the metadata stream — printpdf doesn't expose
    // arbitrary stream insertion, but its `save` produces a clean lopdf doc.
    attach_pdf_metadata(dest, &meta_json)?;
    Ok(())
}

/// Build an `ImageXObject` from raw page bytes, plus the pixel dimensions
/// (so the caller can size the page). JPEG stays a DCT stream (the bytes are
/// written verbatim under `/DCTDecode`); PNG/WEBP/AVIF decode to 8-bit RGB(A)
/// pixels laid down uncompressed. Alpha is composited onto white instead of
/// being kept as a soft mask — see below.
fn build_image_xobject(bytes: &[u8]) -> Result<(printpdf::ImageXObject, u32, u32)> {
    use printpdf::{ColorBits, ColorSpace, ImageFilter, ImageXObject, Px};

    let ext = guess_image_extension(bytes);
    if ext == "jpg" {
        // JPEG: embed verbatim as a DCTDecode stream. Only the pixel
        // dimensions are needed, so read the header via ImageReader (public
        // in image 0.25.x) instead of fully decoding the pixels — a page that
        // can't be fully decoded can still ride the DCT stream.
        let (w, h) = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .context("jpeg format guess")?
            .into_dimensions()
            .context("decode jpeg dims")?;
        return Ok((
            ImageXObject {
                width: Px(w as usize),
                height: Px(h as usize),
                color_space: ColorSpace::Rgb,
                bits_per_component: ColorBits::Bit8,
                interpolate: true,
                image_data: bytes.to_vec(),
                image_filter: Some(ImageFilter::DCT),
                smask: None,
                clipping_bbox: None,
            },
            w,
            h,
        ));
    }

    // PNG / WEBP / AVIF: decode to pixels (AVIF can't ride a DCT stream).
    let img = image::load_from_memory(bytes).context("decode image for pdf")?;
    let (w, h) = (img.width(), img.height());
    let raw = img.to_rgba8().into_raw();

    // Alpha: composite onto white instead of emitting an SMask — printpdf
    // 0.7.0 serializes the SMask height as `img.width` (xobject.rs copy-paste
    // bug), so any image carrying an SMask renders blank in viewers. The
    // alpha check must also be the real color type: to_rgba8() is always 4
    // channels, so a length check would mark every image alpha-bearing and
    // hit the bug for every page.
    let rgb: Vec<u8> = if img.color().has_alpha() {
        raw.chunks_exact(4)
            .flat_map(|c| {
                let a = c[3] as f32 / 255.0;
                let inv = 1.0 - a;
                [
                    (c[0] as f32 * a + 255.0 * inv) as u8,
                    (c[1] as f32 * a + 255.0 * inv) as u8,
                    (c[2] as f32 * a + 255.0 * inv) as u8,
                ]
            })
            .collect()
    } else {
        raw.chunks_exact(4)
            .flat_map(|c| [c[0], c[1], c[2]])
            .collect()
    };

    Ok((
        ImageXObject {
            width: Px(w as usize),
            height: Px(h as usize),
            color_space: ColorSpace::Rgb,
            bits_per_component: ColorBits::Bit8,
            interpolate: true,
            image_data: rgb,
            image_filter: None,
            smask: None,
            clipping_bbox: None,
        },
        w,
        h,
    ))
}

/// Graft an `ErolibMetadata` metadata stream onto an existing PDF so import can
/// recover the full provenance record. The stream is a JSON blob linked from the
/// document Catalog's `/Metadata` entry (the PDF spec's sanctioned spot for
/// a stream-style metadata dictionary).
fn attach_pdf_metadata(path: &Path, json: &str) -> Result<()> {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::load(path).context("open pdf for metadata")?;

    let stream = Stream::new(
        dictionary! {
            "Type" => "Metadata",
            "Subtype" => "XML",
            // Tagged so import can recognize it without parsing XML.
            "Erolib" => true,
        },
        json.as_bytes().to_vec(),
    );
    let id = doc.add_object(stream);
    // Attach the metadata stream to the document Catalog's /Metadata entry.
    let root_ref = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .context("pdf catalog root")?;
    doc.get_dictionary_mut(root_ref)?
        .set("Metadata", Object::Reference(id));

    doc.save(path).context("save pdf metadata")?;
    Ok(())
}

fn ensure_parent(dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).context("create dest dir")?;
    }
    Ok(())
}

// ---- EPUB scaffolding --------------------------------------------------------

const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

fn build_opf(metadata: &BookMetadata, img_paths: &[String]) -> String {
    let page_count = img_paths.len();
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str(
        "<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" \
         unique-identifier=\"bookid\">\n",
    );
    s.push_str("  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n");
    s.push_str(&format!(
        "    <dc:identifier id=\"bookid\">{}</dc:identifier>\n",
        xml_escape(&metadata.title)
    ));
    s.push_str(&format!(
        "    <dc:title>{}</dc:title>\n",
        xml_escape(&metadata.title)
    ));
    s.push_str(&format!(
        "    <dc:creator>{}</dc:creator>\n",
        xml_escape(metadata.author.as_deref().unwrap_or(""))
    ));
    s.push_str(&format!(
        "    <dc:description>{}</dc:description>\n",
        xml_escape(metadata.description.as_deref().unwrap_or(""))
    ));
    for tag in &metadata.tags {
        s.push_str(&format!("    <dc:subject>{}</dc:subject>\n", xml_escape(tag)));
    }
    // erolib provenance as refines; standard readers ignore unknown properties.
    s.push_str("    <meta property=\"ero:sourcePlugin\">");
    s.push_str(&xml_escape(metadata.source_plugin.as_deref().unwrap_or("")));
    s.push_str("</meta>\n");
    s.push_str("    <meta property=\"ero:sourceURL\">");
    s.push_str(&xml_escape(metadata.source_url.as_deref().unwrap_or("")));
    s.push_str("</meta>\n");
    s.push_str("    <meta property=\"ero:sourcePostID\">");
    s.push_str(&xml_escape(metadata.source_post_id.as_deref().unwrap_or("")));
    s.push_str("</meta>\n");
    s.push_str("    <meta property=\"ero:publishedAt\">");
    s.push_str(&xml_escape(metadata.published_at.as_deref().unwrap_or("")));
    s.push_str("</meta>\n");
    s.push_str("    <meta property=\"ero:scrapedAt\">");
    s.push_str(&xml_escape(metadata.scraped_at.as_deref().unwrap_or("")));
    s.push_str("</meta>\n");
    s.push_str("    <meta property=\"ero:delays\">");
    s.push_str(&xml_escape(metadata.delays.as_deref().unwrap_or("")));
    s.push_str("</meta>\n");
    s.push_str("  </metadata>\n");

    s.push_str("  <manifest>\n");
    s.push_str("    <item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n");
    for i in 1..=page_count {
        s.push_str(&format!(
            "    <item id=\"page{}\" href=\"page-{:04}.xhtml\" media-type=\"application/xhtml+xml\"/>\n",
            i, i
        ));
    }
    for (i, img_path) in img_paths.iter().enumerate() {
        let ext = img_path.rsplit('.').next().unwrap_or("jpg");
        let mime = match ext {
            "png" => "image/png",
            "webp" => "image/webp",
            "avif" => "image/avif",
            _ => "image/jpeg",
        };
        s.push_str(&format!(
            "    <item id=\"img{}\" href=\"{}\" media-type=\"{}\"/>\n",
            i + 1,
            img_path,
            mime
        ));
    }
    s.push_str("  </manifest>\n");

    s.push_str("  <spine toc=\"ncx\">\n");
    for i in 1..=page_count {
        s.push_str(&format!("    <itemref idref=\"page{}\"/>\n", i));
    }
    s.push_str("  </spine>\n</package>\n");
    s
}

fn build_ncx(metadata: &BookMetadata, page_count: usize) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str(
        "<ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\n",
    );
    s.push_str("  <head><meta name=\"dtb:uid\" content=\"");
    s.push_str(&xml_escape(&metadata.title));
    s.push_str("\"/></head>\n  <docTitle><text>");
    s.push_str(&xml_escape(&metadata.title));
    s.push_str("</text></docTitle>\n  <navMap>\n");
    for i in 1..=page_count {
        s.push_str(&format!(
            "    <navPoint id=\"np{}\"><navLabel><text>Page {}</text></navLabel>\
             <content src=\"page-{:04}.xhtml#p{}\"/></navPoint>\n",
            i, i, i, i
        ));
    }
    s.push_str("  </navMap>\n</ncx>\n");
    s
}

fn build_xhtml(index: usize, img_path: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>Page {index}</title>\
         <style>html,body{{margin:0;padding:0;background:#000}}img{{width:100%;height:auto;display:block}}</style>\
         </head><body id=\"p{index}\"><img src=\"{img_path}\" alt=\"page {index}\"/></body></html>"
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BookMetadata;

    /// A 1×1 red JPEG (smallest valid DCT stream) so export can size pages and
    /// import can recover bytes without needing real image fixtures.
    fn red_jpeg() -> Vec<u8> {
        // JPEG SOI + minimal table + EOI — decodable by the `image` crate.
        static JPEG: &[u8] = &[
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06,
            0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D,
            0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D,
            0x1A, 0x1C, 0x1C, 0x20, 0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28,
            0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
            0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01,
            0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
            0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10,
            0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00,
            0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06,
            0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42,
            0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16,
            0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37,
            0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55,
            0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73,
            0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
            0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
            0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA,
            0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6,
            0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA,
            0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08,
            0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0xFB, 0xD2, 0x8A, 0x28, 0xA0, 0xFF, 0xD9,
        ];
        JPEG.to_vec()
    }

    fn sample_meta() -> BookMetadata {
        BookMetadata {
            title: "测试タイトル".into(),
            author: Some("巧克力".into()),
            tags: vec!["tag1".into(), "标签2".into()],
            source_plugin: Some("pixiv".into()),
            source_url: Some("https://example.com/123".into()),
            source_post_id: Some("123".into()),
            published_at: Some("2024-01-15T12:00:00Z".into()),
            scraped_at: Some("2024-02-01T00:00:00Z".into()),
            delays: None,
            ..Default::default()
        }
    }

    #[test]
    fn epub_round_trips_metadata() {
        let images = vec![red_jpeg(), red_jpeg()];
        let meta = sample_meta();
        let tmp = std::env::temp_dir().join("erolib_epub_test.epub");
        export_epub(&images, &meta, &tmp, &mut |_: usize| {}).expect("export epub");

        let (pages, recovered) = crate::services::import::read_epub(&tmp).expect("read epub");
        assert_eq!(pages.len(), 2);
        assert_eq!(recovered.title, meta.title);
        assert_eq!(recovered.author, meta.author);
        assert_eq!(recovered.tags, meta.tags);
        assert_eq!(recovered.source_plugin, meta.source_plugin);
        assert_eq!(recovered.source_url, meta.source_url);
        assert_eq!(recovered.source_post_id, meta.source_post_id);
        assert_eq!(recovered.published_at, meta.published_at);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn pdf_round_trips_metadata() {
        let images = vec![red_jpeg(), red_jpeg()];
        let meta = sample_meta();
        let tmp = std::env::temp_dir().join("erolib_pdf_test.pdf");
        export_pdf(&images, &meta, &tmp, &mut |_: usize| {}).expect("export pdf");

        let (pages, recovered) = crate::services::import::read_pdf(&tmp).expect("read pdf");
        // The document must have exactly one page per image (no blank seed
        // page from PdfDocument::new).
        let pdf_pages = lopdf::Document::load(&tmp)
            .expect("reload exported pdf")
            .get_pages()
            .len();
        assert_eq!(pdf_pages, 2);
        assert_eq!(pages.len(), 2);
        assert_eq!(recovered.title, meta.title);
        assert_eq!(recovered.source_plugin, meta.source_plugin);
        assert_eq!(recovered.source_url, meta.source_url);
        assert_eq!(recovered.source_post_id, meta.source_post_id);
        assert_eq!(recovered.published_at, meta.published_at);
        let _ = std::fs::remove_file(&tmp);
    }

    /// A 2×1 red webp page (lossless is fine for a fixture), no alpha — a PDF
    /// export must NOT carry an SMask: printpdf 0.7.0 serializes the SMask
    /// height as `img.width` (xobject.rs copy-paste bug), so any image with a
    /// non-null SMask renders blank in viewers. Regression for the
    /// "all-pages-blank PDF" export bug.
    #[test]
    fn pdf_webp_pages_have_no_smask() {
        let mut buf = Vec::new();
        let img = image::RgbaImage::from_pixel(2, 1, image::Rgba([255, 0, 0, 255]));
        image::codecs::webp::WebPEncoder::new_lossless(&mut buf)
            .encode(img.as_raw(), 2, 1, image::ColorType::Rgba8.into())
            .expect("encode webp fixture");
        let images = vec![buf.clone(), buf.clone()];
        let meta = sample_meta();
        let tmp = std::env::temp_dir().join("erolib_webp_pdf_test.pdf");
        export_pdf(&images, &meta, &tmp, &mut |_: usize| {}).expect("export pdf");

        let doc = lopdf::Document::load(&tmp).expect("reload pdf");
        // One page per image, each painting the image via a `Do` operator.
        let pages = doc.get_pages();
        assert_eq!(pages.len(), 2);
        for (_, page_id) in pages {
            let page = doc.get_dictionary(page_id).expect("page dict");
            // printpdf writes uncompressed content streams, so read the raw
            // `content` field — lopdf's decompressed_content() errors on
            // streams without a Filter entry.
            let contents = page
                .get(b"Contents")
                .expect("page contents")
                .as_reference()
                .ok()
                .and_then(|r| doc.get_object(r).ok())
                .and_then(|o| o.as_stream().ok())
                .map(|s| String::from_utf8_lossy(&s.content).to_string())
                .expect("read page contents");
            assert!(contents.contains(" Do"), "page content stream must paint the image");
        }
        // printpdf writes the SMask key even when None, so any non-null value
        // anywhere in the doc means the bug fired.
        let has_real_smask = doc.objects.values().any(|o| match o {
            lopdf::Object::Dictionary(d) => d
                .get(b"SMask")
                .is_ok_and(|v| !matches!(v, lopdf::Object::Null)),
            _ => false,
        });
        assert!(
            !has_real_smask,
            "no image may carry an SMask (printpdf 0.7 serializes a broken height)"
        );

        // Round-trip still works: read_pdf recovers both pages.
        let (recovered_pages, _) = crate::services::import::read_pdf(&tmp).expect("read pdf");
        assert_eq!(recovered_pages.len(), 2);
        let _ = std::fs::remove_file(&tmp);
    }

    /// ensure_webp: jpeg → webp, webp passthrough, junk fallback.
    #[test]
    fn ensure_webp_converts_and_passes_through() {
        use crate::services::storage::StorageService;

        let jpeg = red_jpeg();
        let out = StorageService::ensure_webp(&jpeg);
        assert!(
            out.starts_with(b"RIFF") && out[8..12] == *b"WEBP",
            "jpeg page must be re-encoded to webp"
        );
        // Round-trip: the webp decodes to the same 1×1 dimensions.
        let img = image::load_from_memory(&out).expect("decode webp");
        assert_eq!((img.width(), img.height()), (1, 1));

        // Already-webp bytes pass through untouched.
        let passthrough = StorageService::ensure_webp(&out);
        assert_eq!(passthrough, out);

        // Undecodable bytes fall back to the original (a page is never dropped).
        let junk = vec![0u8; 64];
        assert_eq!(StorageService::ensure_webp(&junk), junk);
    }
}

