//! System HTTP-proxy auto-detection for downloads.
//!
//! Picks up the proxy a Clash/V2Ray-style client writes when you toggle "set as
//! system proxy", so aria2 downloads transparently route through it with zero
//! configuration — essential for Pixiv/EHentai in regions that need a proxy.
//!
//! Detection order:
//! 1. Environment variables (`ALL_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY`, plus
//!    lowercase), curl-style. These cover `cargo tauri dev` launched from a
//!    shell that has them exported.
//! 2. The macOS system proxy via `scutil --proxy`, for the production GUI app
//!    launched from the Dock/Finder (which does NOT inherit shell env vars).
//!
//! aria2's `all-proxy` only speaks http/https proxies — it cannot use SOCKS —
//! so SOCKS urls are skipped. Results are cached for `CACHE_TTL` so a book's
//! worth of per-image `addUri` calls doesn't spawn `scutil` dozens of times.

use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// How long a detected proxy stays valid before we re-detect. Lets a user
/// toggle their proxy client mid-session and have it picked up within a minute.
const CACHE_TTL: Duration = Duration::from_secs(60);

static CACHE: Mutex<Option<(String, Instant)>> = Mutex::const_new(None);

/// Detect an HTTP/HTTPS proxy URL the aria2 daemon can use.
///
/// Returns `Some("http://host:port")` when a usable proxy is found, or `None`
/// (download direct). Cached for `CACHE_TTL`.
pub async fn detect_http_proxy() -> Option<String> {
    // Serve from cache while fresh.
    {
        let cached = CACHE.lock().await;
        if let Some((url, at)) = cached.as_ref() {
            if at.elapsed() < CACHE_TTL {
                return Some(url.clone());
            }
        }
    }
    let detected = detect_http_proxy_uncached().await;
    match &detected {
        Some(url) => tracing::info!(target: "erolib::proxy", "detected HTTP proxy: {url}"),
        None => tracing::debug!(target: "erolib::proxy", "no HTTP proxy detected; downloads go direct"),
    }
    let mut cached = CACHE.lock().await;
    *cached = detected.as_ref().map(|u| (u.clone(), Instant::now()));
    detected
}

async fn detect_http_proxy_uncached() -> Option<String> {
    if let Some(env) = detect_env_proxy() {
        return Some(env);
    }
    #[cfg(target_os = "macos")]
    if let Some(sys) = detect_macos_system_proxy().await {
        return Some(sys);
    }
    None
}

/// Read `ALL_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY` (and lowercase variants),
/// curl-style. SOCKS urls are skipped because aria2 can't use them.
fn detect_env_proxy() -> Option<String> {
    for key in [
        "ALL_PROXY",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "all_proxy",
        "https_proxy",
        "http_proxy",
    ] {
        if let Ok(raw) = std::env::var(key) {
            let v = raw.trim();
            if v.is_empty() {
                continue;
            }
            // aria2's all-proxy has no SOCKS support — skip so a SOCKS-only
            // ALL_PROXY doesn't poison downloads; fall through to other vars
            // / the system proxy instead.
            if v.to_lowercase().starts_with("socks") {
                continue;
            }
            return Some(normalize_proxy_url(v));
        }
    }
    None
}

/// Ensure the URL has a scheme. aria2's `all-proxy` wants `http://` — the proxy
/// server itself speaks HTTP CONNECT, even when proxying HTTPS traffic — so we
/// prepend `http://` to bare `host:port` strings.
fn normalize_proxy_url(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    }
}

/// Query the macOS system network proxy configuration. This is what Clash /
/// V2Ray write when their "set as system proxy" toggle is on.
#[cfg(target_os = "macos")]
async fn detect_macos_system_proxy() -> Option<String> {
    let output = tokio::process::Command::new("scutil")
        .arg("--proxy")
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_scutil_proxy(&text)
}

/// Parse `scutil --proxy` output (a plist-ish `<dictionary> { ... }`), preferring
/// the HTTPS proxy and falling back to HTTP. Returns `http://host:port`.
#[cfg(target_os = "macos")]
fn parse_scutil_proxy(text: &str) -> Option<String> {
    let fields: std::collections::HashMap<&str, &str> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (k, v) = line.split_once(':')?;
            Some((k.trim(), v.trim()))
        })
        .collect();
    // HTTPS first (our downloads are mostly https), then plain HTTP.
    for (enable_key, host_key, port_key) in [
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
        ("HTTPEnable", "HTTPProxy", "HTTPPort"),
    ] {
        let enabled = fields.get(enable_key).map(|v| *v == "1").unwrap_or(false);
        if !enabled {
            continue;
        }
        if let (Some(host), Some(port)) = (fields.get(host_key), fields.get(port_key)) {
            if !host.is_empty() {
                return Some(format!("http://{host}:{port}"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_scutil_https_proxy() {
        let out = "\
<dictionary> {
  HTTPEnable : 1
  HTTPProxy : 127.0.0.1
  HTTPPort : 7890
  HTTPSEnable : 1
  HTTPSProxy : 127.0.0.1
  HTTPSPort : 7890
  SOCKSEnable : 0
}
";
        assert_eq!(parse_scutil_proxy(out).as_deref(), Some("http://127.0.0.1:7890"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_scutil_http_only() {
        let out = "\
<dictionary> {
  HTTPEnable : 1
  HTTPProxy : 192.168.1.5
  HTTPPort : 8888
  HTTPSEnable : 0
}
";
        assert_eq!(parse_scutil_proxy(out).as_deref(), Some("http://192.168.1.5:8888"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_scutil_disabled() {
        let out = "\
<dictionary> {
  HTTPEnable : 0
  HTTPSEnable : 0
  SOCKSEnable : 0
}
";
        assert!(parse_scutil_proxy(out).is_none());
    }

    #[test]
    fn normalizes_bare_hostport() {
        assert_eq!(normalize_proxy_url("127.0.0.1:7890"), "http://127.0.0.1:7890");
        assert_eq!(normalize_proxy_url("http://127.0.0.1:7890"), "http://127.0.0.1:7890");
    }
}
