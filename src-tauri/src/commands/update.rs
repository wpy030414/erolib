//! In-app self-update against GitHub Releases.
//!
//! Flow (scheme B — "check → download → launch installer", no code signing):
//!   1. `check_update` — fetch the repo's releases Atom feed (server-rendered
//!      XML, always served regardless of rate-limit / auth state), parse the
//!      latest release tag + notes, then construct the download URL from the
//!      known filename pattern.
//!   2. `download_update` — pull that asset through aria2 (so the user's HTTP
//!      proxy applies), emitting `update://progress` events along the way.
//!   3. `install_update` / `quit_and_install` — open the downloaded dmg/msi
//!      with the system handler; the latter exits first so a running .app can
//!      be replaced on macOS.
//!
//! We deliberately do NOT route this through TaskManager: an update package is
//! a single-file download with no book to import, so it lives outside the
//! book-task table. It still uses the same aria2 engine, so progress/speed and
//! proxy behaviour match the rest of the app.
//!
//! ## Why the Atom feed rather than the REST API or the HTML page
//!
//! - `/releases/latest` (API): returns 403 for anonymous clients when
//!   rate-limited (60 req/h per IP), which hits immediately behind shared IPs /
//!   VPNs / carrier NAT.
//! - `/releases/latest` (HTML): the release page is React CSR — the server
//!   HTML has zero `.dmg`/`.msi` links for anonymous visitors; assets render
//!   only after JS hydration.
//! - `/releases.atom`: pure server-rendered XML, no auth required, no rate
//!   limit for reads. Includes tag name + release notes. Download URLs follow
//!   a predictable pattern: `…/releases/download/{tag}/{filename}`.

use std::cmp::Ordering;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use anyhow::Result;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio::time::Duration;

use crate::services::aria2::Aria2Client;

/// GitHub repo that publishes the release installers (owner/name only — the
/// path is fixed by the releases API).
const REPO: &str = "wpy030414/erolib";

/// GitHub rejects API calls without a User-Agent; any descriptive value works.
const UA: &str = concat!("EroLib/", env!("CARGO_PKG_NAME"), "-updater");

/// One downloadable installer attached to a release.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// Result of `check_update`, consumed by the update dialog / settings badge.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    /// Release notes body; may be empty.
    pub notes: String,
    pub asset: Option<UpdateAsset>,
}

/// Live progress for `update://progress`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub percent: u32,
    pub speed: u64,
    pub completed: u64,
    pub total: u64,
}

/// Compare two semver-ish versions, ignoring any `+build` metadata and an
/// optional leading `v`. Numeric segments compare as integers (so `26.7.20 >
/// 26.7.9`, which a lexicographic compare would get wrong); a missing trailing
/// segment counts as 0 (`1.2 == 1.2.0`).
fn semver_compare(a: &str, b: &str) -> Ordering {
    fn parts(v: &str) -> Vec<u64> {
        v.trim()
            .trim_start_matches(['v', 'V'])
            // Strip build metadata — it must not affect precedence, and the
            // project's `26.7.20+1235` style would otherwise break `parse`.
            .split('+')
            .next()
            .unwrap_or("")
            // A pre-release tag (`-rc.1`) is treated as its base version; we
            // never publish one, so a plain split keeps this simple.
            .split('-')
            .next()
            .unwrap_or("")
            .split('.')
            .map(|seg| seg.trim().parse::<u64>().unwrap_or(0))
            .collect()
    }
    let (pa, pb) = (parts(a), parts(b));
    let len = pa.len().max(pb.len());
    for i in 0..len {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

/// Build the download URL and filename for the current platform based on the
/// known CI naming convention (`EroLib_{tag}_{arch}.{ext}`).
fn make_asset(tag: &str) -> Option<UpdateAsset> {
    let clean_tag = tag.trim().trim_start_matches('v').trim_start_matches('V');
    #[cfg(target_os = "macos")]
    let (arch, ext) = ("aarch64", "dmg");
    #[cfg(target_os = "windows")]
    let (arch, ext) = ("x64", "msi");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let (arch, ext) = ("", "");

    if arch.is_empty() {
        return None;
    }

    let name = format!("EroLib_{clean_tag}_{arch}.{ext}");
    let url = format!(
        "https://github.com/{REPO}/releases/download/{tag}/{name}",
        tag = urlencoding::encode(tag),
    );
    Some(UpdateAsset { name, url, size: 0 })
}

/// Decode HTML-escaped entities in the Atom `<content>` body (the feed
/// double-encodes `&` → `&amp;amp;` and `>` → `&amp;gt;` etc.).
fn unescape_atom_content(raw: &str) -> String {
    raw.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

/// Strip HTML tags from the Atom content, leaving readable release notes.
fn strip_html(html: &str) -> String {
    // scraper::Html::parse_document is infallible (it doesn't return Result).
    let doc = Html::parse_document(html);
    let body = doc
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join("");
    body
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse the latest release from the Atom feed XML. Returns `(tag, notes)` or
/// `None` when the feed has no entries.
fn parse_atom_feed(xml: &str) -> Option<(String, String)> {
    let doc = Html::parse_document(xml);
    let entry_sel = Selector::parse("entry").ok()?;
    let title_sel = Selector::parse("title").ok()?;
    let content_sel = Selector::parse("content").ok()?;

    let entry = doc.select(&entry_sel).next()?;
    let title = entry.select(&title_sel).next()?.text().collect::<String>();

    // Tag is "EroLib v26.7.20+1235" → strip the prefix.
    let tag = title
        .strip_prefix("EroLib v")
        .or_else(|| title.strip_prefix("EroLib "))
        .unwrap_or(&title)
        .to_string();

    let notes = entry
        .select(&content_sel)
        .next()
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default();

    Some((tag, unescape_atom_content(&notes)))
}

/// Fetch the latest release from the Atom feed and decide whether it is newer
/// than the running build.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();

    let http = Client::builder()
        .user_agent(UA)
        .build()
        .map_err(|e| format!("build http client: {e}"))?;

    let feed_url = format!("https://github.com/{REPO}/releases.atom");
    let resp = http
        .get(&feed_url)
        .send()
        .await
        .map_err(|e| format!("fetch releases feed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("GitHub releases feed returned HTTP {status}"));
    }

    let xml = resp
        .text()
        .await
        .map_err(|e| format!("read releases feed: {e}"))?;

    let (latest, notes) = parse_atom_feed(&xml)
        .unwrap_or_default();

    if latest.is_empty() {
        return Ok(UpdateInfo {
            current,
            latest: String::new(),
            has_update: false,
            notes: String::new(),
            asset: None,
        });
    }

    let notes = strip_html(&notes);
    let has_update = semver_compare(&latest, &current) == Ordering::Greater;
    let asset = make_asset(&latest);

    Ok(UpdateInfo {
        current,
        latest,
        has_update,
        notes,
        asset,
    })
}

/// Download the given asset through aria2 into `$APPLOCALDATA/update/`,
/// emitting `update://progress` as bytes arrive. Returns the saved file path.
#[tauri::command]
pub async fn download_update(app: AppHandle, url: String, name: String) -> Result<String, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("resolve app_local_data_dir: {e}"))?
        .join("update");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create update dir: {e}"))?;

    let aria2 = Aria2Client::new(app.clone()).map_err(|e| format!("init aria2: {e}"))?;

    let gid = aria2
        .add_uri(&url, None, None, Some(&name), Some(&dir))
        .await
        .map_err(|e| format!("enqueue update download: {e}"))?;

    // Updates are not user-cancellable mid-flight from the dialog (cancel is a
    // nice-to-have); a shared never-set flag satisfies the worker signature.
    let cancelled = AtomicBool::new(false);
    let paused = AtomicBool::new(false);

    let app_for_emit = app.clone();
    let path: PathBuf = aria2
        .wait_for_gid_with_progress(
            &gid,
            Duration::from_millis(500),
            &cancelled,
            &paused,
            move |upd| {
                let app = app_for_emit.clone();
                async move {
                    let percent = if upd.total_length > 0 {
                        ((upd.completed_length * 100) / upd.total_length) as u32
                    } else {
                        0
                    };
                    let _ = app.emit(
                        "update://progress",
                        UpdateProgress {
                            percent,
                            speed: upd.speed,
                            completed: upd.completed_length,
                            total: upd.total_length,
                        },
                    );
                }
            },
        )
        .await
        .map_err(|e| format!("download update: {e}"))?;

    Ok(path.to_string_lossy().to_string())
}

/// Open a downloaded installer with the system handler (macOS mounts the dmg,
/// Windows starts the msi wizard). The user completes installation manually.
#[tauri::command]
pub async fn install_update(app: AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("open installer {}: {e}", path))
}

/// Open the installer, then quit so a running .app can be replaced on macOS.
#[tauri::command]
pub async fn quit_and_install(app: AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("open installer {}: {e}", path))?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_atom_feed, semver_compare, strip_html};
    use std::cmp::Ordering::*;

    #[test]
    fn compares_numeric_segments_not_lexicographically() {
        assert_eq!(semver_compare("26.7.20", "26.7.9"), Greater);
        assert_eq!(semver_compare("26.7.9", "26.7.20"), Less);
    }

    #[test]
    fn ignores_build_metadata_and_v_prefix() {
        assert_eq!(semver_compare("26.7.20+1235", "26.7.20"), Equal);
        assert_eq!(semver_compare("v26.7.21", "26.7.20+1235"), Greater);
        assert_eq!(semver_compare("26.7.20+1235", "v26.7.21"), Less);
    }

    #[test]
    fn treats_missing_segments_as_zero() {
        assert_eq!(semver_compare("1.2", "1.2.0"), Equal);
        assert_eq!(semver_compare("1.2.1", "1.2"), Greater);
        assert_eq!(semver_compare("27.0", "26.7.20"), Greater);
    }

    #[test]
    fn parses_atom_feed_tag() {
        let xml = r#"
        <feed xmlns="http://www.w3.org/2005/Atom">
          <entry>
            <title>EroLib v26.7.20+1235</title>
            <content type="html">&lt;p&gt;release notes here&lt;/p&gt;</content>
          </entry>
        </feed>"#;
        let (tag, notes) = parse_atom_feed(xml).unwrap();
        assert_eq!(tag, "26.7.20+1235");
        assert!(notes.contains("release notes here"));
    }

    #[test]
    fn parses_atom_multiple_entries() {
        let xml = r#"
        <feed xmlns="http://www.w3.org/2005/Atom">
          <entry>
            <title>EroLib v27.0.0</title>
            <content type="html">latest</content>
          </entry>
          <entry>
            <title>EroLib v26.7.20</title>
            <content type="html">older</content>
          </entry>
        </feed>"#;
        let (tag, _) = parse_atom_feed(xml).unwrap();
        assert_eq!(tag, "27.0.0");
    }

    #[test]
    fn parse_atom_empty_returns_none() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"></feed>"#;
        assert!(parse_atom_feed(xml).is_none());
    }

    #[test]
    fn strip_html_extracts_text() {
        let html = "<p>hello</p><ul><li>item 1</li><li>item 2</li></ul>";
        let text = strip_html(html);
        assert!(text.contains("hello"));
        assert!(text.contains("item 1"));
        assert!(text.contains("item 2"));
    }
}
