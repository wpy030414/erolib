//! System HTTP-proxy auto-detection, shared by every outbound HTTP client.
//!
//! Picks up the proxy a Clash/V2Ray-style client writes when you toggle "set
//! as system proxy", so downloads and metadata fetches route through it with
//! zero configuration — essential for Pixiv/EHentai in regions that need one.
//!
//! Detection order:
//! 1. Environment variables (`ALL_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY`, plus
//!    lowercase), curl-style. These cover `cargo tauri dev` launched from a
//!    shell that has them exported.
//! 2. The Windows per-user registry proxy (`HKCU\...\Internet
//!    Settings\ProxyServer`) — what Clash Verge / v2rayN's "system proxy"
//!    toggle writes, for the production GUI app launched from the Start menu
//!    (which does NOT inherit shell env vars).
//! 3. The macOS system proxy via `scutil --proxy`, same role on macOS.
//!
//! The proxy-enable flag is deliberately ignored on both platforms (see the
//! parser comments): TUN-mode users never flip the toggle but the daemon
//! listens anyway. Stale leftovers (proxy client quit but its system setting
//! survived) are caught by a TCP probe — a proxy that isn't accepting
//! connections is treated as "no proxy" and traffic goes direct.
//!
//! PAC (`AutoConfigURL`) scripts are not supported — Clash/v2rayN don't use
//! PAC mode by default.
//!
//! SOCKS proxies are skipped throughout: reqwest 0.11 is built without the
//! `socks` feature, and the old aria2 `all-proxy` constraint is preserved for
//! parity (Clash's mixed port speaks HTTP anyway).
//!
//! `install()` wires the detector into every `reqwest::ClientBuilder` via
//! `Proxy::custom`: the resolver re-checks the (60s-cached) detection per
//! request, so proxy toggles are picked up without rebuilding clients.
//! Loopback targets always bypass the proxy.

use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a detected proxy stays valid before we re-detect. Lets a user
/// toggle their proxy client mid-session and have it picked up within a minute.
const CACHE_TTL: Duration = Duration::from_secs(60);

/// TCP connect budget when probing a detected proxy. Local proxies
/// (127.0.0.1) answer in well under a millisecond; remote corporate proxies
/// get half a second of slack.
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

static CACHE: Mutex<Option<(String, Instant)>> = Mutex::new(None);

/// Serve from cache while fresh.
fn cached() -> Option<String> {
    let guard = CACHE.lock().ok()?;
    if let Some((url, at)) = guard.as_ref() {
        if at.elapsed() < CACHE_TTL {
            return Some(url.clone());
        }
    }
    None
}

/// Run the synchronous detection pipeline and refresh the cache. Also called
/// (cache-checked by the caller) from the `Proxy::custom` resolver.
fn detect_and_cache() -> Option<String> {
    // Another thread may have refreshed the cache while we waited.
    if let Some(url) = cached() {
        return Some(url);
    }
    let detected = detect_uncached();
    match &detected {
        Some(url) => tracing::info!(target: "erolib::proxy", "detected HTTP proxy: {url}"),
        None => tracing::debug!(target: "erolib::proxy", "no HTTP proxy detected; HTTP goes direct"),
    }
    if let Ok(mut guard) = CACHE.lock() {
        *guard = detected.as_ref().map(|u| (u.clone(), Instant::now()));
    }
    detected
}

fn detect_uncached() -> Option<String> {
    if let Some(env) = detect_env_proxy() {
        return probe_alive(&env);
    }
    #[cfg(target_os = "windows")]
    if let Some(sys) = windows_registry_proxy() {
        return probe_alive(&sys);
    }
    #[cfg(target_os = "macos")]
    if let Some(sys) = macos_system_proxy() {
        return probe_alive(&sys);
    }
    None
}

/// Verify the proxy is actually accepting TCP connections before handing it
/// out. A Clash that has been quit (but whose registry/system setting
/// survived) would otherwise black-hole every request; a failed probe falls
/// back to a direct connection.
fn probe_alive(proxy_url: &str) -> Option<String> {
    let parsed = url::Url::parse(proxy_url).ok()?;
    let host = parsed.host_str()?.to_string();
    let port = parsed.port_or_known_default()?;
    let addr: SocketAddr = match (host.as_str(), port).to_socket_addrs() {
        Ok(mut addrs) => addrs.next()?,
        Err(_) => return None,
    };
    match TcpStream::connect_timeout(&addr, PROBE_TIMEOUT) {
        Ok(_) => Some(proxy_url.to_string()),
        Err(e) => {
            tracing::debug!(target: "erolib::proxy", "proxy {proxy_url} not accepting connections ({e}); going direct");
            None
        }
    }
}

/// Read `ALL_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY` (and lowercase variants),
/// curl-style. SOCKS urls are skipped — no SOCKS support downstream — so a
/// SOCKS-only `ALL_PROXY` doesn't poison traffic; fall through to the other
/// vars / the system proxy instead.
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
            if v.to_lowercase().starts_with("socks") {
                continue;
            }
            return Some(normalize_proxy_url(v));
        }
    }
    None
}

/// Ensure the URL has an `http://` scheme. The proxy server itself speaks
/// plain HTTP CONNECT (even when proxying HTTPS traffic), and local Clash
/// ports don't speak TLS, so an `https://` prefix is stripped rather than
/// passed through.
fn normalize_proxy_url(raw: &str) -> String {
    let raw = raw.trim();
    let bare = raw
        .strip_prefix("https://")
        .or_else(|| raw.strip_prefix("http://"))
        .unwrap_or(raw);
    format!("http://{bare}")
}

/// Query the Windows per-user registry proxy — what Clash Verge / v2rayN
/// write when their "set as system proxy" toggle is on. `ProxyEnable` is
/// deliberately ignored: toggling the switch off leaves `ProxyServer` behind
/// while the daemon keeps listening, and TUN-mode users never flip it at all.
#[cfg(target_os = "windows")]
fn windows_registry_proxy() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    let raw: String = key.get_value("ProxyServer").ok()?;
    parse_windows_proxy_server(&raw)
}

/// Parse the registry `ProxyServer` value. Two shapes exist:
/// - plain `host:port` (what Clash Verge / v2rayN write — one proxy for all
///   protocols);
/// - per-protocol `http=h:p;https=h:p;ftp=h:p;socks=h:p` (WinINET legacy).
///
/// HTTPS entry preferred, then HTTP; `socks=`/`ftp=` entries are ignored (no
/// SOCKS support downstream, FTP is dead). Returns `http://host:port`.
fn parse_windows_proxy_server(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.contains('=') {
        let mut http: Option<&str> = None;
        let mut https: Option<&str> = None;
        for part in raw.split(';') {
            let Some((k, v)) = part.trim().split_once('=') else {
                continue;
            };
            let v = v.trim();
            if v.is_empty() || v.to_lowercase().starts_with("socks") {
                continue;
            }
            match k.trim().to_ascii_lowercase().as_str() {
                "https" => https = https.or(Some(v)),
                "http" => http = http.or(Some(v)),
                _ => {}
            }
        }
        let chosen = https.or(http)?;
        return Some(normalize_proxy_url(chosen));
    }
    // Plain host:port; a lone socks:// url is unusable.
    if raw.to_lowercase().starts_with("socks") {
        return None;
    }
    Some(normalize_proxy_url(raw))
}

/// Query the macOS system network proxy configuration. This is what Clash /
/// V2Ray write when their "set as system proxy" toggle is on.
#[cfg(target_os = "macos")]
fn macos_system_proxy() -> Option<String> {
    let output = std::process::Command::new("scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_scutil_proxy(&String::from_utf8_lossy(&output.stdout))
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
    // Clash Verge only sets Enable=1 when "set as system proxy" is ON, but the
    // proxy daemon always listens — and most users keep that toggle off (the GUI
    // app uses TUN mode on macOS). Ignore the Enable flag entirely and just pick
    // the first non-empty host:port so the whole app routes through the proxy
    // automatically. HTTPS proxy preferred, then HTTP.
    for (_enable_key, host_key, port_key) in [
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
        ("HTTPEnable", "HTTPProxy", "HTTPPort"),
    ] {
        if let (Some(host), Some(port)) = (fields.get(host_key), fields.get(port_key)) {
            if !host.is_empty() {
                return Some(format!("http://{host}:{port}"));
            }
        }
    }
    None
}

/// Wire proxy detection into a `reqwest::ClientBuilder`. The custom proxy
/// resolver re-checks the (cached) detector on every request, so a proxy
/// toggled mid-session is picked up within `CACHE_TTL` without rebuilding the
/// client. Loopback targets always bypass the proxy (local servers — OPDS/RSS
/// dev flows, anything on 127.0.0.1 — must never detour through the proxy).
pub fn install(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder.proxy(reqwest::Proxy::custom(|url| {
        if is_loopback_host(url.host_str()) {
            return None;
        }
        cached().or_else(detect_and_cache)
    }))
}

fn is_loopback_host(host: Option<&str>) -> bool {
    match host {
        Some(h) => h.eq_ignore_ascii_case("localhost") || h == "::1" || h.starts_with("127."),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_bare_hostport() {
        assert_eq!(normalize_proxy_url("127.0.0.1:7890"), "http://127.0.0.1:7890");
        assert_eq!(normalize_proxy_url("http://127.0.0.1:7890"), "http://127.0.0.1:7890");
    }

    #[test]
    fn normalizes_https_scheme_down_to_http() {
        // Local proxy ports don't speak TLS; the proxy protocol is plain HTTP
        // CONNECT regardless of the proxied traffic's scheme.
        assert_eq!(normalize_proxy_url("https://proxy.corp:8080"), "http://proxy.corp:8080");
    }

    #[test]
    fn parses_plain_windows_proxy() {
        assert_eq!(
            parse_windows_proxy_server("127.0.0.1:7897").as_deref(),
            Some("http://127.0.0.1:7897")
        );
    }

    #[test]
    fn parses_per_protocol_windows_proxy() {
        let raw = "ftp=10.0.0.5:21;http=127.0.0.1:7890;https=127.0.0.1:7890;socks=127.0.0.1:7891";
        assert_eq!(
            parse_windows_proxy_server(raw).as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn parses_per_protocol_http_only() {
        // No https= entry → fall back to http=.
        assert_eq!(
            parse_windows_proxy_server("http=192.168.1.5:8888;ftp=10.0.0.5:21").as_deref(),
            Some("http://192.168.1.5:8888")
        );
    }

    #[test]
    fn rejects_socks_only_windows_proxy() {
        assert_eq!(parse_windows_proxy_server("socks=127.0.0.1:7891"), None);
        assert_eq!(parse_windows_proxy_server("socks5://127.0.0.1:7891"), None);
    }

    #[test]
    fn rejects_empty_windows_proxy() {
        assert_eq!(parse_windows_proxy_server(""), None);
        assert_eq!(parse_windows_proxy_server("   "), None);
    }

    #[test]
    fn loopback_hosts_bypass_proxy() {
        assert!(is_loopback_host(Some("localhost")));
        assert!(is_loopback_host(Some("LOCALHOST")));
        assert!(is_loopback_host(Some("127.0.0.1")));
        assert!(is_loopback_host(Some("::1")));
        assert!(!is_loopback_host(Some("i.pximg.net")));
        assert!(!is_loopback_host(None));
    }

    /// Smoke test for the real registry read + TCP probe path. The result
    /// depends on the machine (a Clash/V2Ray "system proxy" may or may not be
    /// on), so it only asserts well-formedness — but running it with
    /// `--nocapture` on a machine that *does* have a system proxy is the
    /// end-to-end proof the Windows detection works. On non-Windows it falls
    /// back to env vars (usually empty in CI) and returns None.
    #[test]
    fn detection_path_is_well_formed() {
        let detected = detect_uncached();
        eprintln!("proxy detection -> {detected:?}");
        if let Some(url) = detected {
            assert!(url.starts_with("http://"), "unexpected proxy url: {url}");
        }
    }

    /// The "no proxy found" outcome must NOT be cached — that is exactly what
    /// lets a user launch EroLib first and enable Clash afterwards: the next
    /// request re-detects and picks the proxy up. This test pins that
    /// behaviour by pointing the detector at a dead port and asserting the
    /// cache stays empty afterwards.
    #[test]
    fn failed_detection_is_not_cached() {
        const KEYS: [&str; 6] = [
            "ALL_PROXY",
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "all_proxy",
            "https_proxy",
            "http_proxy",
        ];
        let saved: Vec<(&str, Option<String>)> = KEYS
            .iter()
            .map(|k| (*k, std::env::var(k).ok()))
            .collect();
        for k in KEYS {
            std::env::remove_var(k);
        }
        // A syntactically valid proxy on a closed port. Env is checked first,
        // so this makes the outcome deterministic regardless of the machine's
        // registry/system settings; port 9 refuses instantly (no 500ms wait).
        std::env::set_var("HTTP_PROXY", "http://127.0.0.1:9");
        *CACHE.lock().unwrap() = None;

        let first = detect_and_cache();
        assert!(first.is_none(), "dead proxy must not be adopted: {first:?}");
        assert!(
            cached().is_none(),
            "a failed detection must not be cached — enabling the proxy later \
             relies on the next request re-detecting"
        );

        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        *CACHE.lock().unwrap() = None;
    }

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
    fn parses_scutil_disabled_still_returns_proxy() {
        // Even when the system-proxy toggle is off, Clash's daemon still
        // listens on the port — the host:port is what matters.
        let out = "\
<dictionary> {
  HTTPEnable : 0
  HTTPProxy : 127.0.0.1
  HTTPPort : 7897
  HTTPSEnable : 0
  HTTPSProxy : 127.0.0.1
  HTTPSPort : 7897
  SOCKSEnable : 0
}
";
        assert_eq!(
            parse_scutil_proxy(out).as_deref(),
            Some("http://127.0.0.1:7897")
        );
    }
}
