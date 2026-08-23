//! Import parsers for non-cb7 formats: read an EPUB or PDF the user picked and
//! return its image pages (in reading order) plus a `BookMetadata` recovered
//! from the format's metadata carrier. `library::import_book` then repacks the
//! pages into a library CB7 via `create_cb7`, so the rest of the app (reader,
//! covers, sync) is unchanged.
//!
//! - **EPUB**: scan the OPF spine in order, pull the `<img src>` from each
//!   XHTML page, and read the referenced image bytes out of the zip. OPF `<dc:*>`
//!   fields + `<meta property="ero:...">` refines map back to `BookMetadata`.
//! - **PDF**: walk every page's `/Resources /XObject` for image streams, in page
//!   order. JPEG (DCTDecode) bytes are recovered verbatim; other filters decode
//!   to pixels and re-encode as JPEG so the library cb7 stays a uniform image
//!   sequence. The Info dict + the `ErolibMetadata` catalog stream (if present)
//!   restore provenance.

use std::io::{Cursor, Read};

use anyhow::{anyhow, Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use zip::ZipArchive;

use crate::models::BookMetadata;

/// Parse an EPUB into (pages, metadata). Pages are the images referenced by the
/// spine XHTML, in spine order; metadata is recovered from the OPF.
pub fn read_epub(path: &std::path::Path) -> Result<(Vec<Vec<u8>>, BookMetadata)> {
    let file = std::fs::File::open(path).context("open epub")?;
    let mut archive = ZipArchive::new(file).context("read epub zip")?;

    // 1. Locate the OPF via container.xml (META-INF/container.xml → rootfile).
    let opf_path = find_opf_path(&mut archive)?;

    // 2. Parse the OPF: collect metadata + the spine manifest ids in order.
    let opf_xml = read_zip_entry(&mut archive, &opf_path)?;
    let opf_dir = opf_path
        .rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default();
    let (meta, manifest, spine) = parse_opf(&opf_xml)?;

    // 3. For each spine item, read its XHTML, find <img src>, resolve the image
    //    path relative to the OPF dir, and pull the bytes (binary).
    let mut pages = Vec::new();
    for item_id in &spine {
        let href = manifest
            .iter()
            .find(|(id, _)| id == item_id)
            .map(|(_, href)| href.clone())
            .ok_or_else(|| anyhow!("spine item {item_id} not in manifest"))?;
        let xhtml_path = join_opf(&opf_dir, &href);
        let xhtml = read_zip_entry(&mut archive, &xhtml_path)?;
        if let Some(img_src) = first_img_src(&xhtml) {
            let img_path = join_opf(&opf_dir, &img_src);
            let bytes = read_zip_entry_bytes(&mut archive, &img_path)?;
            pages.push(bytes);
        }
    }

    Ok((pages, meta))
}

/// Parse a PDF into (pages, metadata). Each page yields the first image XObject
/// found in its Resources; JPEG streams are kept verbatim, others are re-encoded
/// to JPEG. Metadata comes from the Info dict, superseded by the erolib metadata
/// stream when present.
pub fn read_pdf(path: &std::path::Path) -> Result<(Vec<Vec<u8>>, BookMetadata)> {
    use lopdf::{Document, Object};
    let doc = Document::load(path).context("open pdf")?;

    // ErolibMetadata catalog stream (full provenance JSON) — written by export.
    let mut meta = BookMetadata::default();
    if let Ok(catalog) = doc.catalog() {
        if let Ok(Object::Reference(id)) = catalog.get(b"Metadata") {
            if let Ok(Object::Stream(stream)) = doc.get_object(*id) {
                // The erolib stream is written uncompressed; read content
                // directly (decompressed_content is for text streams).
                let bytes = &stream.content;
                if let Ok(m) = serde_json::from_slice::<BookMetadata>(bytes) {
                    meta = m;
                }
            }
        }
    }
    // Fall back to Info dict fields when no erolib stream (e.g. a PDF not made
    // by erolib). Title/author/subject/keywords still beat an empty record.
    if meta.title.is_empty() {
        fill_meta_from_info(&doc, &mut meta)?;
    }

    // Walk pages in order, pulling the first image XObject from each.
    let pages_map = doc.get_pages(); // BTreeMap<page_number, ObjectId> — ordered.
    let mut pages = Vec::with_capacity(pages_map.len());
    for (_num, page_id) in pages_map {
        // Get the page object, then its Resources, then XObject. Resources
        // may be an inline Dictionary or a Reference to one; handle both.
        let resources_obj = doc.get_object(page_id).ok().and_then(|o| match o {
            Object::Dictionary(d) => d.get(b"Resources").ok().cloned(),
            _ => None,
        });
        let img_bytes = first_page_image_from_resources(&doc, resources_obj);
        if let Some(bytes) = img_bytes {
            pages.push(bytes);
        }
    }
    if pages.is_empty() {
        return Err(anyhow!("no extractable images found in PDF"));
    }
    Ok((pages, meta))
}

// ---- EPUB helpers -----------------------------------------------------------

fn find_opf_path<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<String> {
    let container = read_zip_entry(archive, "META-INF/container.xml")?;
    let mut reader = Reader::from_str(&container);
    reader.trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            // <rootfile> is a self-closing element in practice, but handle both.
            Ok(Event::Empty(e)) | Ok(Event::Start(e))
                if e.name().as_ref() == b"rootfile" =>
            {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"full-path" {
                        return Ok(String::from_utf8_lossy(&attr.value).into_owned());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    Err(anyhow!("no rootfile full-path in container.xml"))
}

fn read_zip_entry<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<String> {
    let mut entry = archive
        .by_name(name)
        .with_context(|| format!("zip entry {name} not found"))?;
    let mut s = String::new();
    entry.read_to_string(&mut s)?;
    Ok(s)
}

/// Read a zip entry as raw bytes (images are binary).
fn read_zip_entry_bytes<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>> {
    let mut entry = archive
        .by_name(name)
        .with_context(|| format!("zip entry {name} not found"))?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

fn join_opf(dir: &str, href: &str) -> String {
    if dir.is_empty() || href.starts_with('/') {
        href.to_string()
    } else {
        format!("{dir}/{href}")
    }
}

/// Parse the OPF into (metadata, manifest [(id, href)], spine [item-id]).
fn parse_opf(xml: &str) -> Result<(BookMetadata, Vec<(String, String)>, Vec<String>)> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut buf = Vec::new();

    let mut meta = BookMetadata::default();
    let mut manifest: Vec<(String, String)> = Vec::new();
    // current open element name, to route Text events; plus a per-meta property
    // capture for ero: refines.
    let mut cur = String::new();
    let mut cur_meta_prop: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                cur = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                // <meta property="ero:..."> carries provenance.
                if cur == "meta" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"property" {
                            cur_meta_prop =
                                Some(String::from_utf8_lossy(&attr.value).into_owned());
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let name = String::from_utf8_lossy(&name_bytes);
                if name == "item" {
                    let mut id = String::new();
                    let mut href = String::new();
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"id" => id = String::from_utf8_lossy(&attr.value).into_owned(),
                            b"href" => href = String::from_utf8_lossy(&attr.value).into_owned(),
                            _ => {}
                        }
                    }
                    if !id.is_empty() {
                        manifest.push((id, href));
                    }
                }
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
                    "dc:title" => meta.title = text.to_string(),
                    "dc:creator" => meta.author = Some(text.to_string()),
                    "dc:description" => meta.description = Some(text.to_string()),
                    "dc:subject" => meta.tags.push(text.to_string()),
                    "meta" => {
                        if let Some(prop) = &cur_meta_prop {
                            match prop.as_str() {
                                "ero:sourcePlugin" => meta.source_plugin = Some(text.to_string()),
                                "ero:sourceURL" => meta.source_url = Some(text.to_string()),
                                "ero:sourcePostID" => meta.source_post_id = Some(text.to_string()),
                                "ero:publishedAt" => meta.published_at = Some(text.to_string()),
                                "ero:scrapedAt" => meta.scraped_at = Some(text.to_string()),
                                "ero:delays" => meta.delays = Some(text.to_string()),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let name = String::from_utf8_lossy(&name_bytes);
                if name == "meta" {
                    cur_meta_prop = None;
                }
                if name == "itemref" {
                    // handled in Empty below if self-closed; spine refs are
                    // usually empty elements, but cover the paired-end case.
                }
                cur.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    // Second pass for spine: itemref is an empty element with idref, captured
    // above as Empty only for <item>. Re-scan for itemref separately.
    let spine = parse_spine(xml);

    if meta.title.is_empty() {
        // Fall back to manifest's first item href stem.
        if let Some((_, href)) = manifest.first() {
            meta.title = href
                .rsplit_once('.')
                .map(|(stem, _)| stem.to_string())
                .unwrap_or_else(|| href.clone());
        }
    }
    Ok((meta, manifest, spine))
}

fn parse_spine(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut spine = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) if e.name().as_ref() == b"itemref" => {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"idref" {
                        spine.push(String::from_utf8_lossy(&attr.value).into_owned());
                    }
                }
            }
            Ok(Event::Start(e)) if e.name().as_ref() == b"itemref" => {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"idref" {
                        spine.push(String::from_utf8_lossy(&attr.value).into_owned());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    spine
}

/// First <img src="..."> in an XHTML page string.
fn first_img_src(xhtml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xhtml);
    reader.trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e))
                if e.name().as_ref() == b"img" =>
            {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"src" {
                        return Some(String::from_utf8_lossy(&attr.value).into_owned());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    None
}

// ---- PDF helpers ------------------------------------------------------------

fn fill_meta_from_info(doc: &lopdf::Document, meta: &mut BookMetadata) -> Result<()> {
    use lopdf::Object;
    let Ok(info_ref) = doc.trailer.get(b"Info") else {
        return Ok(());
    };
    let info_id = match doc.dereference(info_ref) {
        Ok((Some(id), _)) => id,
        _ => return Ok(()),
    };
    let Ok(Object::Dictionary(info)) = doc.get_object(info_id) else {
        return Ok(());
    };

    // Helper: dereference a value and return its string content (PDF strings
    // are raw bytes; lopdf exposes them as Object::String(_,_)).
    fn str_field<'a>(doc: &'a lopdf::Document, info: &'a lopdf::Dictionary, key: &[u8]) -> Option<String> {
        let val = info.get(key).ok()?;
        let (_, derefed) = doc.dereference(val).ok()?;
        match derefed {
            Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).into_owned()),
            Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
            _ => None,
        }
    }

    if meta.title.is_empty() {
        meta.title = str_field(doc, info, b"Title").unwrap_or_default();
    }
    if meta.author.is_none() {
        meta.author = str_field(doc, info, b"Author");
    }
    if meta.description.is_none() {
        meta.description = str_field(doc, info, b"Subject");
    }
    if meta.tags.is_empty() {
        if let Some(kw) = str_field(doc, info, b"Keywords") {
            meta.tags = kw
                .split([',', ';'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    Ok(())
}

/// Extract the first image XObject from a page's /Resources value. `resources`
/// may be a Reference (resolve it) or an inline Dictionary. JPEG (DCTDecode)
/// bytes are returned verbatim; other images decode to pixels and re-encode as
/// JPEG so the library cb7 holds a uniform image sequence.
fn first_page_image_from_resources(
    doc: &lopdf::Document,
    resources: Option<lopdf::Object>,
) -> Option<Vec<u8>> {
    use lopdf::{Object, ObjectId};
    // Resolve Resources to a Dictionary (inline or via Reference).
    let res_dict_owned = match resources {
        Some(Object::Reference(id)) => match doc.get_object(id) {
            Ok(Object::Dictionary(d)) => Some(d.clone()),
            _ => None,
        },
        Some(Object::Dictionary(d)) => Some(d.clone()),
        _ => None,
    }?;
    let xobj_ref = res_dict_owned.get(b"XObject").ok()?;
    // XObject can be a direct Dictionary or a Reference to one; clone the
    // entries so the doc borrow doesn't span the loop body.
    let xobj_entries: Vec<(Vec<u8>, lopdf::Object)> = match doc.dereference(xobj_ref) {
        Ok((_, Object::Dictionary(d))) => d.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => return None,
    };
    for (_name, val) in xobj_entries {
        let img_id: ObjectId = match doc.dereference(&val) {
            Ok((Some(id), _)) => id,
            _ => continue,
        };
        let Ok(Object::Stream(stream)) = doc.get_object(img_id) else {
            continue;
        };
        let Ok(subtype) = stream.dict.get(b"Subtype") else {
            continue;
        };
        if subtype.as_name().ok() != Some(b"Image") {
            continue;
        }
        // Filter tells us the encoding: DCTDecode = JPEG (raw bytes are the
        // JPEG file), anything else we decompress + re-encode.
        //
        // NOTE: lopdf's `decompressed_content()` refuses to process image
        // streams (it errors on Subtype=Image), so we read `stream.content`
        // directly and handle the two encodings printpdf produces ourselves:
        //   - DCTDecode → the bytes ARE a complete JPEG file.
        //   - no filter → uncompressed raw RGB(A) pixels; assemble with
        //     Width/Height and re-encode as JPEG.
        let filter = stream.dict.get(b"Filter").ok();
        let is_jpeg = filter
            .and_then(|f| match f {
                Object::Name(n) => Some(n.as_slice() == b"DCTDecode"),
                Object::Array(arr) => arr
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .map(|n| n == b"DCTDecode"),
                _ => None,
            })
            .unwrap_or(false);
        if is_jpeg {
            return Some(stream.content.clone());
        }
        // Uncompressed raw pixels: get dimensions + color space from the dict
        // and build an image buffer.
        let content = &stream.content;
        let Ok(w) = stream.dict.get(b"Width").and_then(Object::as_i64) else {
            continue;
        };
        let Ok(h) = stream.dict.get(b"Height").and_then(Object::as_i64) else {
            continue;
        };
        let cs = stream
            .dict
            .get(b"ColorSpace")
            .and_then(Object::as_name_str)
            .unwrap_or("DeviceRGB")
            .to_string();
        let expected = match cs.as_str() {
            "DeviceGray" => w * h,
            _ => w * h * 3, // DeviceRGB (SMask alpha is separate)
        } as usize;
        if content.len() < expected {
            continue;
        }
        let img = match cs.as_str() {
            "DeviceGray" => image::GrayImage::from_raw(w as u32, h as u32, content[..expected].to_vec())
                .map(image::DynamicImage::ImageLuma8),
            _ => image::RgbImage::from_raw(w as u32, h as u32, content[..expected].to_vec())
                .map(image::DynamicImage::ImageRgb8),
        };
        if let Some(img) = img {
            let mut buf = Cursor::new(Vec::new());
            if img.write_to(&mut buf, image::ImageFormat::Jpeg).is_ok() {
                return Some(buf.into_inner());
            }
        }
    }
    None
}
