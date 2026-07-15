//! Helpers shared by the RSS (`rss.rs`) and OPDS (`opds.rs`) feed renderers:
//! XML/HTML text escaping, byte-size formatting, and the per-book metadata
//! blurb used as both the RSS `<description>` and the Atom `<summary>`.

use crate::models::Book;

/// Escape `& < > " '` for safe embedding in XML/HTML text content.
pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Format a byte count as a compact human-readable size (e.g. "12.3 MB").
pub(crate) fn human_size(bytes: i64) -> String {
    let bytes = bytes as f64;
    let (value, unit) = if bytes >= 1_073_741_824.0 {
        (bytes / 1_073_741_824.0, "GB")
    } else if bytes >= 1_048_576.0 {
        (bytes / 1_048_576.0, "MB")
    } else if bytes >= 1024.0 {
        (bytes / 1024.0, "KB")
    } else {
        (bytes, "B")
    };
    format!("{:.1} {}", value, unit)
}

/// Rich per-field metadata blurb as HTML (one line per field, `<br>`-joined).
///
/// Returned WITHOUT a CDATA wrapper so the RSS `<description>` and Atom
/// `<summary type="html">` callers can each wrap it themselves. Field values
/// are xml-escaped so a stray `<`/`&` in scraped author/tag/url data can't
/// break the feed; only the structural `<br>`/`<a>` remain as live markup.
/// Empty fields are omitted; page/format/size and the import date always show.
pub(crate) fn book_metadata_blurb(book: &Book) -> String {
    let mut lines: Vec<String> = Vec::new();
    if let Some(author) = &book.author {
        if !author.is_empty() {
            let label = match &book.author_id {
                Some(aid) if !aid.is_empty() => {
                    format!("作者：{}（{}）", xml_escape(author), xml_escape(aid))
                }
                _ => format!("作者：{}", xml_escape(author)),
            };
            lines.push(label);
        }
    }
    lines.push(format!(
        "{} 页 · {} · {}",
        book.page_count,
        xml_escape(&book.format.to_uppercase()),
        human_size(book.file_size),
    ));
    if let Some(tags) = &book.tags {
        if !tags.is_empty() {
            lines.push(format!("标签：{}", xml_escape(tags)));
        }
    }
    {
        let mut src: Vec<String> = Vec::new();
        if let Some(plugin) = &book.source_plugin {
            if !plugin.is_empty() {
                src.push(xml_escape(plugin));
            }
        }
        if let Some(pid) = &book.source_post_id {
            if !pid.is_empty() {
                src.push(format!("作品 {}", xml_escape(pid)));
            }
        }
        if !src.is_empty() {
            lines.push(format!("来源：{}", src.join(" · ")));
        }
    }
    if let Some(url) = &book.source_url {
        if !url.is_empty() {
            let u = xml_escape(url);
            lines.push(format!("链接：<a href=\"{}\">{}</a>", u, u));
        }
    }
    if let Some(name) = &book.original_filename {
        if !name.is_empty() {
            lines.push(format!("原始文件：{}", xml_escape(name)));
        }
    }
    if let Some(pa) = &book.published_at {
        if !pa.is_empty() {
            lines.push(format!("发布日期：{}", xml_escape(pa)));
        }
    }
    lines.push(format!(
        "收录于：{}",
        book.created_at.format("%Y-%m-%d %H:%M")
    ));
    if book.read_count > 0 {
        match &book.last_read_at {
            Some(lr) => lines.push(format!(
                "阅读 {} 次 · 最近 {}",
                book.read_count,
                lr.format("%Y-%m-%d %H:%M")
            )),
            None => lines.push(format!("阅读 {} 次", book.read_count)),
        }
    }
    lines.join("<br>")
}
