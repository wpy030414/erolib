use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use axum::response::IntoResponse;
use tauri::{AppHandle, State, Manager};

use crate::services::{OpdsService, RssService, StorageService};
use crate::AppState;

/// Shared handles to the running OPDS/RSS servers so we can stop them on demand.
pub struct ServerHandle {
    pub opds_shutdown: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    pub rss_shutdown: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
}

impl ServerHandle {
    pub fn new() -> Self {
        Self {
            opds_shutdown: Mutex::new(None),
            rss_shutdown: Mutex::new(None),
        }
    }
}

/// Start the OPDS server. Idempotent: if already running, does nothing.
pub async fn start_opds_server(
    port: u16,
    app_handle: &AppHandle,
    state: &AppState,
) -> Result<String, String> {
    let handle = app_handle.state::<Arc<ServerHandle>>();
    if handle.opds_shutdown.lock().unwrap().is_some() {
        return Err("OPDS server is already running".into());
    }

    let (tx, rx) = tokio::sync::watch::channel(false);
    {
        let mut guard = handle.opds_shutdown.lock().unwrap();
        *guard = Some(tx);
    }

    // Bind on all interfaces (LAN-wide, no auth) and advertise the host's
    // primary LAN address so feeds contain device-reachable links.
    let base_url = format!("http://{}:{}", local_lan_ip(), port);
    state.opds_service.set_base_url(base_url.clone());

    let db = state.db.clone();
    let storage = state.storage.clone();
    let storage_path = state.library_service.storage.library_path.clone();
    let covers_path = state.library_service.storage.cover_path.clone();
    let rss_service = state.rss_service.clone();
    let opds_service = state.opds_service.clone();

    let app = axum::Router::new()
        .route("/opds", axum::routing::get(opds_root))
        .route("/opds/search/:query", axum::routing::get(opds_search))
        .route("/covers/:id", axum::routing::get(serve_cover))
        .route("/download/:id", axum::routing::get(serve_download))
        .route("/pages/:id/:n", axum::routing::get(serve_page))
        .route("/article/:id", axum::routing::get(serve_article))
        .with_state(ServerState {
            db,
            storage,
            storage_path,
            covers_path,
            base_url: Arc::new(base_url.clone()),
            rss_service,
            opds_service,
        });

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .map_err(|e| format!("OPDS: {e}"))?;

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let mut rx = rx;
                let _ = rx.changed().await;
            })
            .await
            .ok();
    });

    Ok(base_url)
}

/// Start the RSS server. Idempotent: if already running, does nothing.
pub async fn start_rss_server(
    port: u16,
    app_handle: &AppHandle,
    state: &AppState,
) -> Result<String, String> {
    let handle = app_handle.state::<Arc<ServerHandle>>();
    if handle.rss_shutdown.lock().unwrap().is_some() {
        return Err("RSS server is already running".into());
    }

    let (tx, rx) = tokio::sync::watch::channel(false);
    {
        let mut guard = handle.rss_shutdown.lock().unwrap();
        *guard = Some(tx);
    }

    // Bind on all interfaces (LAN-wide, no auth) and advertise the host's
    // primary LAN address so feeds contain device-reachable links.
    let base_url = format!("http://{}:{}", local_lan_ip(), port);
    state.rss_service.set_base_url(base_url.clone());

    let db = state.db.clone();
    let storage = state.storage.clone();
    let storage_path = state.library_service.storage.library_path.clone();
    let covers_path = state.library_service.storage.cover_path.clone();
    let rss_service = state.rss_service.clone();
    let opds_service = state.opds_service.clone();

    let app = axum::Router::new()
        .route("/rss", axum::routing::get(rss_root))
        .route("/download/:id", axum::routing::get(serve_download))
        .route("/covers/:id", axum::routing::get(serve_cover))
        .route("/pages/:id/:n", axum::routing::get(serve_page))
        .route("/article/:id", axum::routing::get(serve_article))
        .with_state(ServerState {
            db,
            storage,
            storage_path,
            covers_path,
            base_url: Arc::new(base_url.clone()),
            rss_service,
            opds_service,
        });

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .map_err(|e| format!("RSS: {e}"))?;

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let mut rx = rx;
                let _ = rx.changed().await;
            })
            .await
            .ok();
    });

    Ok(base_url)
}

/// Stop the OPDS server.
pub async fn stop_opds_server(app_handle: &AppHandle) {
    let handle = app_handle.state::<Arc<ServerHandle>>();
    let mut guard = handle.opds_shutdown.lock().unwrap();
    if let Some(tx) = guard.take() {
        let _ = tx.send(true);
    }
}

/// Stop the RSS server.
pub async fn stop_rss_server(app_handle: &AppHandle) {
    let handle = app_handle.state::<Arc<ServerHandle>>();
    let mut guard = handle.rss_shutdown.lock().unwrap();
    if let Some(tx) = guard.take() {
        let _ = tx.send(true);
    }
}

// ---- Tauri command wrappers ----

#[tauri::command]
pub async fn start_opds_server_cmd(
    port: u16,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    start_opds_server(port, &app_handle, &state).await
}

#[tauri::command]
pub async fn stop_opds_server_cmd(app_handle: AppHandle) -> Result<(), String> {
    stop_opds_server(&app_handle).await;
    Ok(())
}

#[tauri::command]
pub async fn start_rss_server_cmd(
    port: u16,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    start_rss_server(port, &app_handle, &state).await
}

#[tauri::command]
pub async fn stop_rss_server_cmd(app_handle: AppHandle) -> Result<(), String> {
    stop_rss_server(&app_handle).await;
    Ok(())
}

// ---- Route handlers ----

/// Unified state shared by both the OPDS and RSS axum routers. Both feed
/// services are configured with the host's LAN base URL at startup, so we
/// thread them in here and let the feed handlers reuse them — otherwise a
/// handler would rebuild a fresh service whose base URL falls back to the
/// wrong default (`http://localhost:8081`), emitting cover/download links no
/// client can reach. The shared cover/download handlers only need the DB
/// handle and the on-disk paths.
#[derive(Clone)]
struct ServerState {
    db: Arc<crate::db::Database>,
    /// Archive reader used to extract a single page image on demand for the
    /// `/pages/:id/:n` route (reuses the in-memory archive cache).
    storage: Arc<StorageService>,
    storage_path: PathBuf,
    covers_path: PathBuf,
    /// Base URL advertised at startup (LAN IP:port). Used by `/article/:id` to
    /// build page-image URLs RSS readers can reach over the LAN.
    base_url: Arc<String>,
    rss_service: Arc<RssService>,
    opds_service: Arc<OpdsService>,
}

async fn opds_root(axum::extract::State(state): axum::extract::State<ServerState>) -> axum::response::Response {
    match state.opds_service.root_feed().await {
        Ok(xml) => (
            [(axum::http::header::CONTENT_TYPE, "application/atom+xml;profile=opds-catalog")],
            xml,
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("error: {}", e),
        )
            .into_response(),
    }
}

async fn opds_search(
    axum::extract::Path(query): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<ServerState>,
) -> axum::response::Response {
    match state.opds_service.search_feed(&query).await {
        Ok(xml) => (
            [(axum::http::header::CONTENT_TYPE, "application/atom+xml;profile=opds-catalog")],
            xml,
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("error: {}", e),
        )
            .into_response(),
    }
}

async fn serve_cover(
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<ServerState>,
) -> axum::response::Response {
    for ext in &["jpg", "jpeg", "png", "webp"] {
        let path = state.covers_path.join(format!("{}.{}", id, ext));
        if let Ok(data) = std::fs::read(&path) {
            let mime = match *ext {
                "png" => "image/png",
                "webp" => "image/webp",
                _ => "image/jpeg",
            };
            return ([(axum::http::header::CONTENT_TYPE, mime)], data).into_response();
        }
    }
    (axum::http::StatusCode::NOT_FOUND, "not found").into_response()
}

/// Serve a single page image by 0-based index: `GET /pages/:id/:n`.
///
/// RSS readers fetch this for every `<img>` embedded in a feed item's
/// `content:encoded`, letting the user read the whole book as an image
/// sequence instead of downloading the cb7 archive. Extraction reuses the
/// cached `ZipArchive` behind a blocking task so the async runtime stays free.
async fn serve_page(
    axum::extract::Path((id, page)): axum::extract::Path<(String, usize)>,
    axum::extract::State(state): axum::extract::State<ServerState>,
) -> axum::response::Response {
    let book = match sqlx::query_as::<_, crate::models::Book>("SELECT * FROM books WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db.pool)
        .await
    {
        Ok(Some(b)) => b,
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, "book not found").into_response(),
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("db error: {}", e),
            )
                .into_response()
        }
    };

    let path = std::path::PathBuf::from(&book.file_path);
    let storage = state.storage.clone();
    let bytes = match tokio::task::spawn_blocking(move || storage.read_page(&path, page)).await {
        Ok(Some(b)) => b,
        Ok(None) => {
            return (axum::http::StatusCode::NOT_FOUND, "page not found").into_response()
        }
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("read error: {}", e),
            )
                .into_response()
        }
    };
    let mime = match guess_image_mime(&bytes) {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };
    ([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response()
}

/// Render an HTML gallery of every page: `GET /article/:id`.
///
/// This is the safe permalink target for RSS `<link>`. Readers that fall back
/// to fetching the link (instead of rendering `content:encoded`) get a real
/// HTML image strip here — never the raw cb7 bytes that would render as
/// garbled text.
async fn serve_article(
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<ServerState>,
) -> axum::response::Response {
    let book = match sqlx::query_as::<_, crate::models::Book>("SELECT * FROM books WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db.pool)
        .await
    {
        Ok(Some(b)) => b,
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, "book not found").into_response(),
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("db error: {}", e),
            )
                .into_response()
        }
    };

    let base = state.base_url.as_str();
    let mut imgs = String::new();
    for n in 0..book.page_count.max(1) as usize {
        imgs.push_str(&format!(
            r#"<img src="{base}/pages/{id}/{n}" alt="page {p}" loading="lazy" style="max-width:100%;height:auto;display:block;margin:0 auto"/>"#,
            base = base,
            id = book.id,
            n = n,
            p = n + 1,
        ));
    }
    let html = format!(
        r#"<!doctype html><html lang="zh"><head><meta charset="utf-8"/>
<meta name="viewport" content="width=device-width,initial-scale=1"/>
<title>{title}</title>
<style>body{{margin:0;background:#111}}img{{max-width:100%}}</style></head>
<body>{imgs}</body></html>"#,
        title = html_escape(&book.title),
        imgs = imgs,
    );
    (
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
        .into_response()
}

/// Sniff the image MIME kind from magic bytes. Pages carry no per-entry type,
/// so we sniff like the cover handler / storage's extension guesser do.
fn guess_image_mime(bytes: &[u8]) -> &'static str {
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

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

async fn rss_root(
    axum::extract::State(state): axum::extract::State<ServerState>,
) -> axum::response::Response {
    match state.rss_service.feed().await {
        Ok(xml) => (
            [(axum::http::header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
            xml,
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("error: {}", e),
        )
            .into_response(),
    }
}

async fn serve_download(
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<ServerState>,
) -> axum::response::Response {
    let book = match sqlx::query_as::<_, crate::models::Book>("SELECT * FROM books WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db.pool)
        .await
    {
        Ok(Some(b)) => b,
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("db error: {}", e),
            )
                .into_response()
        }
    };

    let ext = std::path::Path::new(&book.file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("cb7");
    let path = state.storage_path.join(&book.file_path);
    let mime = match ext.to_lowercase().as_str() {
        "cbz" => "application/x-cbz",
        "cbr" => "application/x-cbr",
        "pdf" => "application/pdf",
        _ => "application/x-cb7",
    };
    let filename = format!("{}.{}", book.title.replace(' ', "_"), ext);

    match std::fs::read(&path) {
        Ok(data) => {
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static(mime),
            );
            headers.insert(
                axum::http::header::CONTENT_DISPOSITION,
                axum::http::HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename))
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("attachment")),
            );
            (headers, data).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("read error: {}", e),
        )
            .into_response(),
    }
}

/// Best-effort primary LAN IPv4 of this machine. Uses a connect()ed UDP socket
/// (no packets are actually sent) so the kernel reports the default-route
/// interface's address — the one other devices on the same Wi-Fi/LAN reach us
/// on. Falls back to `127.0.0.1` (localhost only) if it can't be determined.
fn local_lan_ip() -> String {
    let socket = match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return "127.0.0.1".to_string(),
    };
    if socket.connect("8.8.8.8:80").is_err() {
        return "127.0.0.1".to_string();
    }
    match socket.local_addr() {
        Ok(std::net::SocketAddr::V4(v4)) => v4.ip().to_string(),
        _ => "127.0.0.1".to_string(),
    }
}
