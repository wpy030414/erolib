use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sqlx::Row;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use tokio::time::sleep;
use uuid::Uuid;

use crate::db::Database;
use crate::models::{BookMetadata, BookSource};
use crate::services::pixiv::{find_existing_by_source, PixivClient};
use crate::services::task::{TaskPayload, TaskSnapshot, TaskSource, TaskStatus};
use crate::services::{Aria2Client, EhentaiClient, LibraryService, StorageService};

const MAX_RETRIES: i32 = 3;
const BACKOFF_SECS: [u64; 3] = [1, 2, 4];

/// Runtime controller for an active worker.
#[derive(Debug)]
pub struct TaskRuntime {
    cancelled: AtomicBool,
    paused: AtomicBool,
}

impl TaskRuntime {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            paused: AtomicBool::new(false),
        }
    }
}

/// Centralised manager for all download/packaging tasks.
pub struct TaskManager {
    app: AppHandle,
    db: Arc<Database>,
    storage: Arc<StorageService>,
    aria2: Aria2Client,
    workers: Mutex<HashMap<String, Arc<TaskRuntime>>>,
    /// Weak self-reference set after construction so workers can obtain an
    /// `Arc<Self>` without an `unsafe` clone via ptr::read.
    self_weak: Mutex<Option<Weak<Self>>>,
}

#[derive(Debug)]
struct TaskRow {
    id: String,
    source: String,
    status: String,
    title: String,
    detail: String,
    progress_current: i64,
    progress_total: i64,
    retry_count: i32,
    max_retries: i32,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
    payload: String,
}

// Hand-implemented because the `sqlx::FromRow` derive (sqlx-macros) can't be
// built in the release profile on this toolchain — see Cargo.toml.
impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for TaskRow {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            source: row.try_get("source")?,
            status: row.try_get("status")?,
            title: row.try_get("title")?,
            detail: row.try_get("detail")?,
            progress_current: row.try_get("progress_current")?,
            progress_total: row.try_get("progress_total")?,
            retry_count: row.try_get("retry_count")?,
            max_retries: row.try_get("max_retries")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            completed_at: row.try_get("completed_at")?,
            payload: row.try_get("payload")?,
        })
    }
}

#[derive(Debug, Serialize)]
struct TaskToast {
    kind: String,
    title: String,
}

// ---- Private helpers ----

fn into_snapshot(row: TaskRow) -> TaskSnapshot {
    let completed_at_str = row.completed_at.clone();
    TaskSnapshot {
        id: row.id,
        source: row.source,
        status: row.status,
        title: row.title,
        detail: row.detail,
        progress_current: row.progress_current,
        progress_total: row.progress_total,
        retry_count: row.retry_count,
        max_retries: row.max_retries,
        created_at: row.created_at,
        updated_at: row.updated_at,
        completed_at: completed_at_str,
    }
}

fn parse_task(row: TaskRow) -> Result<crate::services::task::Task> {
    let payload: TaskPayload =
        serde_json::from_str(&row.payload).context("deserialize task payload")?;
    let source: TaskSource = row
        .source
        .parse()
        .map_err(|e: String| anyhow::anyhow!("{e}"))?;
    let status: TaskStatus = row
        .status
        .parse()
        .map_err(|e: String| anyhow::anyhow!("{e}"))?;
    let created_at: chrono::DateTime<chrono::Utc> =
        row.created_at.parse().context("parse created_at")?;
    let updated_at: chrono::DateTime<chrono::Utc> =
        row.updated_at.parse().context("parse updated_at")?;
    let completed_at: Option<chrono::DateTime<chrono::Utc>> = row
        .completed_at
        .map(|d| d.parse())
        .transpose()
        .context("parse completed_at")?;

    Ok(crate::services::task::Task {
        id: row.id,
        source,
        status,
        title: row.title,
        detail: row.detail,
        progress_current: row.progress_current,
        progress_total: row.progress_total,
        retry_count: row.retry_count,
        max_retries: row.max_retries,
        created_at,
        updated_at,
        completed_at,
        payload,
    })
}

impl TaskManager {
    pub async fn new(
        app: AppHandle,
        db: Arc<Database>,
        storage: Arc<StorageService>,
    ) -> Result<Self> {
        let aria2 = Aria2Client::new(app.clone()).context(
            "create aria2 client (lazy; will connect on first download)",
        )?;
        Ok(Self {
            app,
            db,
            storage,
            aria2,
            workers: Mutex::new(HashMap::new()),
            self_weak: Mutex::new(None),
        })
    }

    /// Called once after the TaskManager is wrapped in `Arc` to set the
    /// self-referencing weak pointer used by worker tasks.
    pub fn init_self_ref(this: &Arc<Self>) {
        *this.self_weak.blocking_lock() = Some(Arc::downgrade(this));
    }

    pub async fn enqueue(&self, payload: TaskPayload, title: String) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let payload_json = serde_json::to_string(&payload).context("serialize task payload")?;
        sqlx::query(
            "INSERT INTO tasks (id, source, status, title, detail, progress_current, progress_total, retry_count, max_retries, created_at, updated_at, payload)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(payload.source().to_string())
        .bind(TaskStatus::Pending.to_string())
        .bind(&title)
        .bind("")
        .bind(0i64)
        .bind(0i64)
        .bind(0i32)
        .bind(MAX_RETRIES)
        .bind(&now)
        .bind(&now)
        .bind(&payload_json)
        .execute(&self.db.pool)
        .await
        .context("insert task")?;

        self.start_task(&id).await?;
        Ok(id)
    }

    pub async fn list_tasks(&self) -> Result<Vec<TaskSnapshot>> {
        let rows: Vec<TaskRow> = sqlx::query_as(
            "SELECT id, source, status, title, detail, progress_current, progress_total, retry_count, max_retries, created_at, updated_at, completed_at, payload
             FROM tasks ORDER BY created_at DESC",
        )
        .fetch_all(&self.db.pool)
        .await
        .context("list tasks")?;
        Ok(rows.into_iter().map(into_snapshot).collect())
    }

    pub async fn pause_task(&self, id: &str) -> Result<()> {
        let runtime = {
            let workers = self.workers.lock().await;
            workers.get(id).cloned()
        };
        if let Some(rt) = runtime {
            rt.paused.store(true, Ordering::Relaxed);
        }
        self.set_status(id, TaskStatus::Paused, None).await?;
        Ok(())
    }

    pub async fn resume_task(&self, id: &str) -> Result<()> {
        self.set_status(id, TaskStatus::Running, None).await?;
        self.start_task(id).await?;
        Ok(())
    }

    pub async fn cancel_task(&self, id: &str) -> Result<()> {
        let runtime = {
            let workers = self.workers.lock().await;
            workers.get(id).cloned()
        };
        if let Some(rt) = runtime {
            rt.cancelled.store(true, Ordering::Relaxed);
        }
        self.set_status(id, TaskStatus::Cancelled, None).await?;
        self.emit_terminal_toast(id, "cancelled").await?;
        Ok(())
    }

    pub async fn delete_task(&self, id: &str) -> Result<()> {
        {
            let mut workers = self.workers.lock().await;
            if let Some(rt) = workers.remove(id) {
                rt.cancelled.store(true, Ordering::Relaxed);
            }
        }
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(&self.db.pool)
            .await
            .context("delete task")?;
        Ok(())
    }

    pub async fn retry_task(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE tasks SET retry_count = 0, detail = '', progress_current = 0 WHERE id = ?",
        )
        .bind(id)
        .execute(&self.db.pool)
        .await
        .context("reset retry count")?;
        self.resume_task(id).await
    }

    async fn start_task(&self, id: &str) -> Result<()> {
        let mut workers = self.workers.lock().await;
        if workers.contains_key(id) {
            return Ok(());
        }
        let runtime = Arc::new(TaskRuntime::new());
        workers.insert(id.to_string(), runtime.clone());
        drop(workers);

        // Mark running in DB.
        self.set_status(id, TaskStatus::Running, None).await?;

        // Upgrade self-weak to an Arc so the worker can own a reference.
        let self_arc = self
            .self_weak
            .lock()
            .await
            .as_ref()
            .and_then(|w| w.upgrade())
            .context("TaskManager dropped before worker could start")?;

        tokio::spawn(run_task_worker(self_arc, id.to_string(), runtime));
        Ok(())
    }

    async fn set_status(
        &self,
        id: &str,
        status: TaskStatus,
        detail: Option<String>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let completed_at: Option<String> = if status == TaskStatus::Completed
            || status == TaskStatus::Failed
            || status == TaskStatus::Cancelled
        {
            Some(now.clone())
        } else {
            None
        };
        let detail_val = detail.unwrap_or_default();
        sqlx::query(
            "UPDATE tasks SET status = ?, detail = ?, updated_at = ?, completed_at = ? WHERE id = ?",
        )
        .bind(status.to_string())
        .bind(&detail_val)
        .bind(&now)
        .bind(&completed_at)
        .bind(id)
        .execute(&self.db.pool)
        .await
        .context("set task status")?;
        Ok(())
    }

    async fn set_progress(&self, id: &str, current: i64, total: i64, detail: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE tasks SET progress_current = ?, progress_total = ?, detail = ?, updated_at = ? WHERE id = ?",
        )
        .bind(current)
        .bind(total)
        .bind(detail)
        .bind(&now)
        .bind(id)
        .execute(&self.db.pool)
        .await
        .context("set task progress")?;
        Ok(())
    }

    async fn increment_retry(&self, id: &str, detail: &str) -> Result<i32> {
        sqlx::query(
            "UPDATE tasks SET retry_count = retry_count + 1, detail = ?, updated_at = ? WHERE id = ?",
        )
        .bind(detail)
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(&self.db.pool)
        .await
        .context("increment retry")?;
        let row = sqlx::query("SELECT retry_count FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_one(&self.db.pool)
            .await
            .context("fetch retry count")?;
        Ok(row.get::<i32, _>("retry_count"))
    }

    async fn load_task(
        &self,
        id: &str,
    ) -> Result<Option<crate::services::task::Task>> {
        let row: Option<TaskRow> = sqlx::query_as(
            "SELECT id, source, status, title, detail, progress_current, progress_total, retry_count, max_retries, created_at, updated_at, completed_at, payload FROM tasks WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await
        .context("load task")?;
        match row {
            Some(r) => parse_task(r).map(Some),
            None => Ok(None),
        }
    }

    async fn emit_progress(&self, id: &str) -> Result<()> {
        if let Some(task) = self.load_task(id).await? {
            let snapshot: TaskSnapshot = task.into();
            let _ = self.app.emit("task://progress", &snapshot);
        }
        Ok(())
    }

    async fn emit_terminal_toast(&self, id: &str, kind: &str) -> Result<()> {
        if let Some(task) = self.load_task(id).await? {
            let toast = TaskToast {
                kind: kind.to_string(),
                title: task.title,
            };
            let _ = self.app.emit("task://toast", &toast);
        }
        Ok(())
    }
}

// ====================== Worker (runs in tokio::spawn) ======================

async fn run_task_worker(manager: Arc<TaskManager>, task_id: String, runtime: Arc<TaskRuntime>) {
    loop {
        if runtime.cancelled.load(Ordering::Relaxed) {
            return;
        }

        let task = match manager.load_task(&task_id).await {
            Ok(Some(t)) => t,
            Ok(None) => return,
            Err(e) => {
                tracing::error!(target: "erolib::tasks", task_id, %e, "failed to load task");
                let _ = manager
                    .set_status(&task_id, TaskStatus::Failed, Some(e.to_string()))
                    .await;
                let _ = manager.emit_terminal_toast(&task_id, "failed").await;
                return;
            }
        };

        if task.status != TaskStatus::Running {
            return;
        }

        let result = process_task(&manager, &task, &runtime).await;

        match result {
            Ok(()) => {
                let _ = manager
                    .set_status(&task_id, TaskStatus::Completed, Some("done".to_string()))
                    .await;
                let _ = manager.emit_progress(&task_id).await;
                let _ = manager.emit_terminal_toast(&task_id, "completed").await;
                {
                    let mut workers = manager.workers.lock().await;
                    workers.remove(&task_id);
                }
                return;
            }
            Err(e) => {
                let err_str = e.to_string();
                let retries = match manager.increment_retry(&task_id, &err_str).await {
                    Ok(n) => n,
                    Err(e2) => {
                        tracing::error!(target: "erolib::tasks", task_id, %e2, "failed to increment retry");
                        let _ = manager
                            .set_status(&task_id, TaskStatus::Failed, Some(err_str.clone()))
                            .await;
                        let _ = manager.emit_terminal_toast(&task_id, "failed").await;
                        return;
                    }
                };

                if retries > task.max_retries {
                    let _ = manager
                        .set_status(&task_id, TaskStatus::Failed, Some(err_str.clone()))
                        .await;
                    let _ = manager.emit_progress(&task_id).await;
                    let _ = manager.emit_terminal_toast(&task_id, "failed").await;
                    {
                        let mut workers = manager.workers.lock().await;
                        workers.remove(&task_id);
                    }
                    return;
                }

                let delay = BACKOFF_SECS
                    .get((retries as usize).saturating_sub(1))
                    .copied()
                    .unwrap_or(BACKOFF_SECS.last().copied().unwrap_or(4));
                tracing::info!(
                    target: "erolib::tasks",
                    task_id,
                    retries,
                    delay,
                    "retrying task after error"
                );
                let _ = manager
                    .set_progress(
                        &task_id,
                        task.progress_current,
                        task.progress_total,
                        &format!("retrying ({}): {}", retries, err_str),
                    )
                    .await;
                let _ = manager.emit_progress(&task_id).await;
                sleep(Duration::from_secs(delay)).await;
            }
        }
    }
}

async fn process_task(
    manager: &TaskManager,
    task: &crate::services::task::Task,
    runtime: &TaskRuntime,
) -> Result<()> {
    let temp_dir = manager
        .app
        .path()
        .app_local_data_dir()
        .map_err(|e| anyhow::anyhow!("app_local_data_dir: {e}"))?
        .join("downloads")
        .join(&task.id);
    let _ = std::fs::create_dir_all(&temp_dir);

    match &task.payload {
        TaskPayload::PixivBookmarks {
            cookie,
            user_id,
            limit,
        } => {
            process_pixiv(manager, task, runtime, &temp_dir, cookie, user_id, *limit, true).await
        }
        TaskPayload::PixivUserWorks {
            cookie,
            target_user_id,
            limit,
        } => {
            process_pixiv(
                manager,
                task,
                runtime,
                &temp_dir,
                cookie,
                target_user_id,
                *limit,
                false,
            )
            .await
        }
        TaskPayload::EhentaiGallery {
            cookie,
            gallery_url,
            gid,
            token,
        } => {
            // gallery_url is used via the parameter, but to silence lint
            // pass it to the function which accepts it.
            process_ehentai(manager, task, runtime, &temp_dir, cookie, gallery_url, gid, token).await
        }
        TaskPayload::PixivSingleWork { cookie, work_id } => {
            process_pixiv_single(manager, task, runtime, &temp_dir, cookie, work_id).await
        }
    }
}

// ====================== Pixiv processing ======================

async fn process_pixiv(
    manager: &TaskManager,
    task: &crate::services::task::Task,
    runtime: &TaskRuntime,
    temp_dir: &std::path::Path,
    cookie: &str,
    user_id: &str,
    limit: u64,
    bookmarks: bool,
) -> Result<()> {
    let client = PixivClient::new(cookie).context("build pixiv client")?;
    let library = LibraryService::new(manager.db.clone(), manager.storage.clone());

    manager
        .set_progress(&task.id, 0, 0, "listing works...")
        .await?;
    let _ = manager.emit_progress(&task.id).await;

    let works = if bookmarks {
        client
            .fetch_all_bookmarks(user_id, limit, &runtime.cancelled)
            .await
            .context("fetch bookmarks")?
            .into_iter()
            .map(|w| w.into())
            .collect::<Vec<_>>()
    } else {
        client
            .fetch_user_works(user_id, limit, &runtime.cancelled)
            .await
            .context("fetch user works")?
    };

    let total = works.len() as i64;
    manager
        .set_progress(&task.id, 0, total, "downloading...")
        .await?;
    let _ = manager.emit_progress(&task.id).await;

    for (idx, work) in works.iter().enumerate() {
        if runtime.cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("cancelled");
        }
        while runtime.paused.load(Ordering::Relaxed) {
            if runtime.cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("cancelled");
            }
            sleep(Duration::from_millis(500)).await;
        }

        let current = idx as i64 + 1;
        let detail = format!("{} ({}/{})", work.title, current, total);
        manager
            .set_progress(&task.id, current - 1, total, &detail)
            .await?;
        let _ = manager.emit_progress(&task.id).await;

        if let Err(e) =
            process_pixiv_work(manager, runtime, temp_dir, &client, &library, work, None).await
        {
            tracing::warn!(
                target: "erolib::tasks",
                task_id = %task.id,
                work_id = %work.id,
                %e,
                "work failed"
            );
        }
    }

    manager
        .set_progress(&task.id, total, total, "done")
        .await?;
    let _ = manager.emit_progress(&task.id).await;
    Ok(())
}

/// Download a single Pixiv artwork (clicked from the browse grid). Reuses
/// `process_pixiv_work` after resolving the work's metadata via the detail API.
async fn process_pixiv_single(
    manager: &TaskManager,
    task: &crate::services::task::Task,
    runtime: &TaskRuntime,
    temp_dir: &std::path::Path,
    cookie: &str,
    work_id: &str,
) -> Result<()> {
    let client = PixivClient::new(cookie).context("build pixiv client")?;
    let library = LibraryService::new(manager.db.clone(), manager.storage.clone());

    manager
        .set_progress(&task.id, 0, 1, "fetching work...")
        .await?;
    let _ = manager.emit_progress(&task.id).await;

    let work = client
        .fetch_illust_detail(work_id)
        .await
        .context("fetch illust detail")?
        .ok_or_else(|| anyhow::anyhow!("work {work_id} not found"))?;

    manager
        .set_progress(&task.id, 0, 1, "downloading...")
        .await?;
    let _ = manager.emit_progress(&task.id).await;

    process_pixiv_work(manager, runtime, temp_dir, &client, &library, &work, Some(&task.id))
        .await?;

    manager
        .set_progress(&task.id, 1, 1, "done")
        .await?;
    let _ = manager.emit_progress(&task.id).await;
    Ok(())
}

async fn process_pixiv_work(
    manager: &TaskManager,
    runtime: &TaskRuntime,
    temp_dir: &std::path::Path,
    client: &PixivClient,
    library: &LibraryService,
    work: &crate::services::pixiv::UserWork,
    task_id: Option<&str>,
) -> Result<()> {
    // Ugoira (動画作, illustType==2): a multi-frame animation zipped with
    // per-frame delays. Route to a dedicated handler that encodes one GIF and
    // stores it as a 1-page book. Regular manga (illustType==0/1) continues.
    if work.illust_type == Some(2) {
        return process_pixiv_ugoira(manager, runtime, temp_dir, client, library, work, task_id)
            .await;
    }

    let pages = client.fetch_pages(&work.id).await.context("fetch pages")?;
    if pages.is_empty() {
        anyhow::bail!("no pages");
    }
    let new_page_count = pages.len() as i32;
    let source_url = format!("https://www.pixiv.net/artworks/{}", work.id);

    let existing = find_existing_by_source(&manager.db.pool, &source_url).await?;
    let book_id = if let Some(prev) = existing {
        if prev.page_count == new_page_count && prev.title == work.title {
            // Backfill tags for books registered before tag scraping worked on
            // the bookmarks path. Idempotent (ON CONFLICT); skips re-downloading.
            if !work.tags.is_empty() {
                let _ = library.link_tags(&prev.book_id, &work.tags).await;
            }
            return Ok(());
        }
        library
            .remove_book(&prev.book_id)
            .await
            .context("remove old book")?;
        prev.book_id
    } else {
        Uuid::new_v4().to_string()
    };

    let mut images: Vec<Vec<u8>> = Vec::with_capacity(pages.len());
    for (pidx, page) in pages.iter().enumerate() {
        if runtime.cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("cancelled");
        }
        let url = if page.urls.original.is_empty() {
            &page.urls.regular
        } else {
            &page.urls.original
        };
        if url.is_empty() {
            continue;
        }
        let out = format!("{:04}", pidx);
        let gid = manager
            .aria2
            .add_uri(url, Some("https://www.pixiv.net/"), Some(&out), Some(temp_dir))
            .await
            .with_context(|| format!("add uri {}", url))?;
        let path = manager
            .aria2
            .wait_for_gid(&gid, Duration::from_millis(500), &runtime.cancelled)
            .await
            .with_context(|| format!("download {}", url))?;
        let bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("read {}", path.display()))?;
        if bytes.len() < 100 {
            anyhow::bail!("suspiciously small image from {}", url);
        }
        images.push(bytes);
        // For single-work tasks, report per-image progress so the card's ring
        // advances instead of spinning indefinitely. Batch tasks pass None and
        // are tracked at the work-index level by the caller.
        if let Some(tid) = task_id {
            let _ = manager
                .set_progress(tid, (pidx + 1) as i64, pages.len() as i64, "downloading")
                .await;
            let _ = manager.emit_progress(tid).await;
        }
    }

    if images.is_empty() {
        anyhow::bail!("no images downloaded");
    }

    let file_path = manager
        .storage
        .create_cb7(
            &images,
            &BookMetadata {
                title: work.title.clone(),
                tags: work.tags.clone(),
                ..Default::default()
            },
        )
        .context("create cb7")?;

    let source = BookSource {
        plugin: "pixiv".into(),
        source_url,
        scraped_at: Some(Utc::now()),
        source_post_id: Some(work.id.clone()),
        author: work.author.clone(),
        author_id: work.author_id.clone(),
        published_at: work.published_at.clone(),
    };

    library
        .register_stored_book(
            &book_id,
            &work.title,
            &file_path,
            images.len() as i32,
            Some(&source),
            &work.tags,
        )
        .await
        .context("register book")?;
    Ok(())
}

/// Download a ugoira (動画作) work: fetch its frame manifest, pull the original
/// zip, extract the per-frame jpgs, encode a single looping GIF honoring the
/// manifest delays, and store it as a 1-page book (a cb7 holding that gif).
/// The browse-grid thumbnail stays the static cover — only the stored book
/// animates. The CPU-bound encode runs off the async runtime (spawn_blocking).
async fn process_pixiv_ugoira(
    manager: &TaskManager,
    runtime: &TaskRuntime,
    temp_dir: &std::path::Path,
    client: &PixivClient,
    library: &LibraryService,
    work: &crate::services::pixiv::UserWork,
    task_id: Option<&str>,
) -> Result<()> {
    // 1. Frame manifest + original-resolution zip URL.
    let meta = client
        .fetch_ugoira_meta(&work.id)
        .await
        .context("fetch ugoira meta")?;
    if meta.frames.is_empty() {
        anyhow::bail!("ugoira has no frames");
    }
    let total_frames = meta.frames.len() as i64;

    if let Some(tid) = task_id {
        let _ = manager
            .set_progress(tid, 0, total_frames, "downloading ugoira zip...")
            .await;
        let _ = manager.emit_progress(tid).await;
    }

    // 2. Download the original zip via aria2 (i.pximg.net needs the Pixiv Referer).
    let gid = manager
        .aria2
        .add_uri(
            &meta.original_src,
            Some("https://www.pixiv.net/"),
            Some("ugoira.zip"),
            Some(temp_dir),
        )
        .await
        .context("add ugoira zip uri")?;
    let zip_path = manager
        .aria2
        .wait_for_gid(&gid, Duration::from_millis(500), &runtime.cancelled)
        .await
        .context("download ugoira zip")?;
    let zip_bytes = tokio::fs::read(&zip_path)
        .await
        .context("read ugoira zip")?;

    // 3. Extract + encode off the runtime (decoding dozens of jpgs + quantizing
    //    is CPU-bound). Map progress so the card's ring visibly advances: ~a
    //    third once the zip is down, full once the gif is built.
    if let Some(tid) = task_id {
        let _ = manager
            .set_progress(tid, total_frames / 3, total_frames, "encoding gif...")
            .await;
        let _ = manager.emit_progress(tid).await;
    }
    let frames = meta.frames.clone();
    let gif = tokio::task::spawn_blocking(move || encode_ugoira_gif(&zip_bytes, &frames))
        .await
        .context("join encode task")??;

    if let Some(tid) = task_id {
        let _ = manager
            .set_progress(tid, total_frames, total_frames, "finalizing...")
            .await;
        let _ = manager.emit_progress(tid).await;
    }

    // 4. Store as a 1-page book (cb7 holding the single gif). Idempotent: a
    //    previously-imported ugoira (page_count==1, same title) is kept.
    let source_url = format!("https://www.pixiv.net/artworks/{}", work.id);
    let existing = find_existing_by_source(&manager.db.pool, &source_url).await?;
    let book_id = if let Some(prev) = existing {
        if prev.page_count == 1 && prev.title == work.title {
            if !work.tags.is_empty() {
                let _ = library.link_tags(&prev.book_id, &work.tags).await;
            }
            return Ok(());
        }
        library
            .remove_book(&prev.book_id)
            .await
            .context("remove old book")?;
        prev.book_id
    } else {
        Uuid::new_v4().to_string()
    };

    let file_path = manager
        .storage
        .create_cb7(
            &[gif],
            &BookMetadata {
                title: work.title.clone(),
                tags: work.tags.clone(),
                ..Default::default()
            },
        )
        .context("create cb7 (ugoira)")?;

    let source = BookSource {
        plugin: "pixiv".into(),
        source_url,
        scraped_at: Some(Utc::now()),
        source_post_id: Some(work.id.clone()),
        author: work.author.clone(),
        author_id: work.author_id.clone(),
        published_at: work.published_at.clone(),
    };

    library
        .register_stored_book(&book_id, &work.title, &file_path, 1, Some(&source), &work.tags)
        .await
        .context("register ugoira book")?;
    Ok(())
}

/// Extract ugoira frames from the downloaded zip and encode a single looping
/// GIF. Frames are jpgs named like "000000.jpg"; the manifest lists them in
/// playback order with per-frame `delay` in ms. Encode speed 10 keeps the
/// NeuQuant step fast on 50–100 frame animations while staying watchable.
fn encode_ugoira_gif(
    zip_bytes: &[u8],
    frames: &[crate::services::pixiv::UgoiraFrame],
) -> Result<Vec<u8>> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::imageops::FilterType;
    use image::{Delay, Frame};
    use std::io::{Cursor, Read};

    // Cap the longest edge of each frame. A 1920×1080 98-frame gif is brutal
    // for the renderer to decode/play back; 1024px keeps the file small and
    // playback smooth while staying watchable full-screen (gif is 256-color
    // anyway, so extra resolution buys little).
    const MAX_EDGE: u32 = 1024;

    let cursor = Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).context("open ugoira zip")?;

    let mut gif_frames: Vec<Frame> = Vec::with_capacity(frames.len());
    for f in frames {
        let mut buf = Vec::new();
        {
            let mut entry = archive
                .by_name(&f.file)
                .with_context(|| format!("locate ugoira frame {}", f.file))?;
            entry.read_to_end(&mut buf)?;
        } // entry dropped here, releasing the archive borrow for the next frame
        let mut img = image::load_from_memory(&buf)
            .with_context(|| format!("decode ugoira frame {}", f.file))?;
        if img.width() > MAX_EDGE || img.height() > MAX_EDGE {
            // resize() preserves aspect ratio (fits within MAX_EDGE×MAX_EDGE).
            img = img.resize(MAX_EDGE, MAX_EDGE, FilterType::Triangle);
        }
        let rgba = img.into_rgba8();
        // delay is ms; denominator 1 → ms resolution (GIF stores 1/100 s).
        let delay = Delay::from_numer_denom_ms(f.delay, 1);
        gif_frames.push(Frame::from_parts(rgba, 0, 0, delay));
    }

    if gif_frames.is_empty() {
        anyhow::bail!("no decodable ugoira frames");
    }

    let mut out: Vec<u8> = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut out, 10);
        encoder.set_repeat(Repeat::Infinite).context("set gif loop")?;
        encoder
            .encode_frames(gif_frames.into_iter())
            .context("encode gif")?;
    }
    Ok(out)
}

// ====================== EHentai processing ======================

async fn process_ehentai(
    manager: &TaskManager,
    task: &crate::services::task::Task,
    runtime: &TaskRuntime,
    temp_dir: &std::path::Path,
    cookie: &str,
    _gallery_url: &str,
    gid: &str,
    token: &str,
) -> Result<()> {
    let client = EhentaiClient::new(cookie).context("build ehentai client")?;
    let library = LibraryService::new(manager.db.clone(), manager.storage.clone());

    manager
        .set_progress(&task.id, 0, 0, "listing pages...")
        .await?;
    let _ = manager.emit_progress(&task.id).await;

    let page_urls = client
        .fetch_gallery_pages(gid, token)
        .await
        .context("fetch gallery pages")?;

    // Best-effort source metadata (posted time + uploader) for the library card.
    let meta = client.fetch_gallery_meta(gid, token).await.unwrap_or_default();

    let total = page_urls.len() as i64;
    manager
        .set_progress(&task.id, 0, total, "downloading...")
        .await?;
    let _ = manager.emit_progress(&task.id).await;

    // fetch_page_image scrapes the page-view HTML (cookie-gated), so it stays
    // in-process. The resolved image URL is then pulled via aria2 — e-hentai's
    // image CDN only needs the Referer (matching the in-process download_image),
    // not the cookie — giving us aria2's multi-connection acceleration.
    let mut images: Vec<Vec<u8>> = Vec::new();

    for (idx, page_url) in page_urls.iter().enumerate() {
        if runtime.cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("cancelled");
        }
        while runtime.paused.load(Ordering::Relaxed) {
            if runtime.cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("cancelled");
            }
            sleep(Duration::from_millis(500)).await;
        }

        let current = idx as i64 + 1;
        let detail = format!("page {}/{}", current, total);
        manager
            .set_progress(&task.id, current - 1, total, &detail)
            .await?;
        let _ = manager.emit_progress(&task.id).await;

        match client.fetch_page_image(page_url).await {
            Ok(img_url) => {
                let out = format!("page-{:04}", idx);
                let aria_gid = match manager
                    .aria2
                    .add_uri(&img_url, Some("https://e-hentai.org/"), Some(&out), Some(temp_dir))
                    .await
                {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::warn!(
                            target: "erolib::tasks",
                            task_id = %task.id,
                            page_url = %page_url,
                            %e,
                            "aria2 add_uri failed"
                        );
                        sleep(Duration::from_millis(400)).await;
                        continue;
                    }
                };
                match manager
                    .aria2
                    .wait_for_gid(&aria_gid, Duration::from_millis(500), &runtime.cancelled)
                    .await
                {
                    Ok(path) => match tokio::fs::read(&path).await {
                        Ok(bytes) => {
                            if bytes.len() < 200 {
                                tracing::warn!(
                                    target: "erolib::tasks",
                                    task_id = %task.id,
                                    img_url = %img_url,
                                    "suspiciously small image"
                                );
                            } else {
                                images.push(bytes);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "erolib::tasks",
                                task_id = %task.id,
                                path = %path.display(),
                                %e,
                                "read downloaded image"
                            );
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            target: "erolib::tasks",
                            task_id = %task.id,
                            page_url = %page_url,
                            %e,
                            "aria2 download failed"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "erolib::tasks",
                    task_id = %task.id,
                    page_url = %page_url,
                    %e,
                    "page fetch failed",
                );
            }
        }
        sleep(Duration::from_millis(400)).await;
    }

    if images.is_empty() {
        anyhow::bail!("no images downloaded");
    }

    manager
        .set_progress(&task.id, total, total, "building cb7...")
        .await?;
    let _ = manager.emit_progress(&task.id).await;

    let file_path = manager
        .storage
        .create_cb7(
            &images,
            &BookMetadata {
                title: task.title.clone(),
                tags: vec![],
                ..Default::default()
            },
        )
        .context("create cb7")?;

    let source = BookSource {
        plugin: "ehentai".into(),
        source_url: format!("https://e-hentai.org/g/{}/{}/", gid, token),
        scraped_at: Some(Utc::now()),
        source_post_id: Some(gid.to_string()),
        author: meta.uploader,
        author_id: None,
        published_at: meta.posted,
    };
    let title = task.title.clone();

    library
        .register_stored_book(
            &Uuid::new_v4().to_string(),
            &title,
            &file_path,
            images.len() as i32,
            Some(&source),
            &[],
        )
        .await
        .context("register book")?;

    manager
        .set_progress(&task.id, total, total, "done")
        .await?;
    let _ = manager.emit_progress(&task.id).await;
    Ok(())
}
