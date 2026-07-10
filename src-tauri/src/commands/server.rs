use std::sync::Arc;
use std::sync::Mutex;

use axum::response::IntoResponse;
use tauri::{AppHandle, State, Manager};

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

#[tauri::command]
pub async fn start_opds_server(
    port: u16,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Ensure the OPDS server is not already running.
    if !app_handle
        .state::<Arc<ServerHandle>>()
        .opds_shutdown
        .lock()
        .unwrap()
        .is_none()
    {
        return Err("OPDS server is already running".into());
    }

    let (tx, rx) = tokio::sync::watch::channel(false);
    {
        let handle_state = app_handle.state::<Arc<ServerHandle>>();
        let mut guard = handle_state.opds_shutdown.lock().unwrap();
        *guard = Some(tx);
    }

    let base_url = format!("http://localhost:{}", port);
    state.opds_service.set_base_url(base_url.clone());

    let db = state.opds_service.db.clone();
    let storage_path = state.library_service.storage.library_path.clone();
    let covers_path = state.library_service.storage.cover_path.clone();

    let app = axum::Router::new()
        .route("/opds", axum::routing::get(opds_root))
        .route("/opds/search/:query", axum::routing::get(opds_search))
        .route("/covers/:id", axum::routing::get(serve_cover))
        .with_state((db, storage_path, covers_path));

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| e.to_string())?;

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

#[tauri::command]
pub async fn stop_opds_server(app_handle: AppHandle) -> Result<(), String> {
    let handle = app_handle.state::<Arc<ServerHandle>>();
    let mut guard = handle.opds_shutdown.lock().unwrap();
    if let Some(tx) = guard.take() {
        let _ = tx.send(true);
    }
    Ok(())
}

#[tauri::command]
pub async fn start_rss_server(
    port: u16,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Ensure the RSS server is not already running.
    if !app_handle
        .state::<Arc<ServerHandle>>()
        .rss_shutdown
        .lock()
        .unwrap()
        .is_none()
    {
        return Err("RSS server is already running".into());
    }

    let (tx, rx) = tokio::sync::watch::channel(false);
    {
        let handle_state = app_handle.state::<Arc<ServerHandle>>();
        let mut guard = handle_state.rss_shutdown.lock().unwrap();
        *guard = Some(tx);
    }

    let base_url = format!("http://localhost:{}", port);
    state.rss_service.set_base_url(base_url.clone());

    let db = state.rss_service.db.clone();
    let storage_path = state.library_service.storage.library_path.clone();
    let covers_path = state.library_service.storage.cover_path.clone();

    let app = axum::Router::new()
        .route("/rss", axum::routing::get(rss_root))
        .route("/download/:id", axum::routing::get(serve_download))
        .route("/covers/:id", axum::routing::get(serve_cover))
        .with_state((db, storage_path, covers_path));

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| e.to_string())?;

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

#[tauri::command]
pub async fn stop_rss_server(app_handle: AppHandle) -> Result<(), String> {
    let handle = app_handle.state::<Arc<ServerHandle>>();
    let mut guard = handle.rss_shutdown.lock().unwrap();
    if let Some(tx) = guard.take() {
        let _ = tx.send(true);
    }
    Ok(())
}

type ServerState = (Arc<crate::db::Database>, std::path::PathBuf, std::path::PathBuf);

async fn opds_root(axum::extract::State((db, _, _)): axum::extract::State<ServerState>) -> axum::response::Response {
    let service = crate::services::OpdsService::new(db);
    match service.root_feed().await {
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
    axum::extract::State((db, _, _)): axum::extract::State<ServerState>,
) -> axum::response::Response {
    let service = crate::services::OpdsService::new(db);
    match service.search_feed(&query).await {
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
    axum::extract::State((_, _, covers)): axum::extract::State<ServerState>,
) -> axum::response::Response {
    for ext in &["jpg", "jpeg", "png", "webp"] {
        let path = covers.join(format!("{}.{}", id, ext));
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

async fn rss_root(
    axum::extract::State((db, _, _)): axum::extract::State<ServerState>,
) -> axum::response::Response {
    let service = crate::services::RssService::new(db);
    match service.feed().await {
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
    axum::extract::State((db, storage, _)): axum::extract::State<ServerState>,
) -> axum::response::Response {
    let book = match sqlx::query_as::<_, crate::models::Book>("SELECT * FROM books WHERE id = ?")
        .bind(&id)
        .fetch_optional(&db.pool)
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
    let path = storage.join(&book.file_path);
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
