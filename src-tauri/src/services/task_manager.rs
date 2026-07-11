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
    /// Per-task EMA (exponential moving average) of the instantaneous download
    /// speed, so the readout glides instead of jittering on every aria2 poll.
    ema_speeds: Mutex<HashMap<String, f64>>,
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
    speed: i64,
    logs: String,
    book_id: Option<String>,
    total_bytes: i64,
    elapsed_ms: i64,
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
            speed: row.try_get("speed")?,
            logs: row.try_get("logs")?,
            book_id: row.try_get("book_id")?,
            total_bytes: row.try_get("total_bytes")?,
            elapsed_ms: row.try_get("elapsed_ms")?,
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
    let logs: Vec<String> = serde_json::from_str(&row.logs).unwrap_or_default();
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
        speed: row.speed,
        logs,
        book_id: row.book_id,
        total_bytes: row.total_bytes,
        elapsed_ms: row.elapsed_ms,
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

    let logs: Vec<String> = serde_json::from_str(&row.logs).unwrap_or_default();
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
        speed: row.speed,
        logs,
        book_id: row.book_id,
        total_bytes: row.total_bytes,
        elapsed_ms: row.elapsed_ms,
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
            ema_speeds: Mutex::new(HashMap::new()),
            self_weak: Mutex::new(None),
        })
    }

    /// Called once after the TaskManager is wrapped in `Arc` to set the
    /// self-referencing weak pointer used by worker tasks.
    pub fn init_self_ref(this: &Arc<Self>) {
        *this.self_weak.blocking_lock() = Some(Arc::downgrade(this));
    }

    /// Reconcile task state on startup. Any task left 'running' from a previous
    /// session (the app was force-quit mid-download) has no live worker, so mark
    /// it 'paused' so the user can resume it from where it stopped instead of it
    /// looking perpetually running. Run once after construction.
    pub async fn reconcile_on_startup(&self) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        // Orphaned 'running' rows have a stale run_started_at from the dead
        // process; drop it (the unmeasured tail between crash and relaunch is
        // unknowable, so we lose that segment rather than charge wall-clock).
        sqlx::query(
            "UPDATE tasks SET status = 'paused', speed = 0, run_started_at = NULL, updated_at = ? WHERE status = 'running'",
        )
        .bind(&now)
        .execute(&self.db.pool)
        .await
        .context("reconcile orphaned running tasks")?;
        Ok(())
    }

    /// Delete every terminal task (completed/failed/cancelled) in one shot.
    pub async fn clear_completed_tasks(&self) -> Result<u64> {
        let res = sqlx::query(
            "DELETE FROM tasks WHERE status IN ('completed','failed','cancelled')",
        )
        .execute(&self.db.pool)
        .await
        .context("clear completed tasks")?;
        Ok(res.rows_affected())
    }

    pub async fn enqueue(&self, payload: TaskPayload, title: String) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let payload_json = serde_json::to_string(&payload).context("serialize task payload")?;

        // Tasks exist only for traceability — keep the newest 100 rows.
        sqlx::query(
            "DELETE FROM tasks WHERE id NOT IN (
                SELECT id FROM tasks ORDER BY created_at DESC LIMIT 99
            )",
        )
        .execute(&self.db.pool)
        .await
        .context("trim old tasks")?;

        sqlx::query(
            "INSERT INTO tasks (id, source, status, title, detail, progress_current, progress_total, retry_count, max_retries, speed, logs, created_at, updated_at, payload)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(0i64)
        .bind("[]")
        .bind(&now)
        .bind(&now)
        .bind(&payload_json)
        .execute(&self.db.pool)
        .await
        .context("insert task")?;

        let _ = self.append_log(&id, "task created").await;
        self.start_task(&id).await?;
        Ok(id)
    }

    pub async fn list_tasks(&self) -> Result<Vec<TaskSnapshot>> {
        let rows: Vec<TaskRow> = sqlx::query_as(
            "SELECT id, source, status, title, detail, progress_current, progress_total, retry_count, max_retries, speed, logs, book_id, total_bytes, elapsed_ms, created_at, updated_at, completed_at, payload
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
        let _ = self.append_log(id, "paused by user").await;
        // Drop the EMA state + zero the readout so the card hides immediately.
        let _ = self.reset_speed(id, 0).await;
        Ok(())
    }

    pub async fn resume_task(&self, id: &str) -> Result<()> {
        // If the worker is still alive (paused mid-download, spinning in place),
        // clear its paused flag so it resumes on its own — no need to spawn a
        // new worker (start_task would no-op anyway since the id is still live).
        let runtime = {
            let workers = self.workers.lock().await;
            workers.get(id).cloned()
        };
        if let Some(rt) = runtime {
            rt.paused.store(false, Ordering::Relaxed);
        }
        self.set_status(id, TaskStatus::Running, None).await?;
        let _ = self.append_log(id, "resumed by user").await;
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
        let _ = self.append_log(id, "cancelled by user").await;
        let _ = self.reset_speed(id, 0).await;
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
        self.ema_speeds.lock().await.remove(id);
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(&self.db.pool)
            .await
            .context("delete task")?;
        Ok(())
    }

    pub async fn retry_task(&self, id: &str) -> Result<()> {
        // Seed the EMA fresh so the retried run doesn't inherit a stale value.
        self.ema_speeds.lock().await.remove(id);
        // Reset the completion readout too — a retry starts over, so the prior
        // (failed/partial) attempt's bytes and time must not bleed into it.
        sqlx::query(
            "UPDATE tasks SET retry_count = 0, detail = '', progress_current = 0, speed = 0, total_bytes = 0, elapsed_ms = 0 WHERE id = ?",
        )
        .bind(id)
        .execute(&self.db.pool)
        .await
        .context("reset retry count")?;
        let _ = self.append_log(id, "retrying task").await;
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
        let now = Utc::now();
        let now_rfc = now.to_rfc3339();
        let is_terminal = matches!(
            status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        );
        let completed_at: Option<String> = if is_terminal {
            Some(now_rfc.clone())
        } else {
            None
        };
        let is_running = status == TaskStatus::Running;
        let detail_val = detail.unwrap_or_default();
        let status_str = status.to_string();

        // Accumulate elapsed_ms for the running segment we're closing, and pick
        // the new run_started_at. Opening a running segment stamps it (or keeps
        // an existing live one); closing one (pause/cancel/complete) folds its
        // duration into elapsed_ms and clears it so the next run opens a fresh
        // segment. This keeps "用时" honest even across pause/resume cycles —
        // paused/pending wall-clock never counts.
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT run_started_at FROM tasks WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.db.pool)
                .await
                .context("read task run_started_at")?;
        let mut elapsed_add_ms: i64 = 0;
        if !is_running {
            if let Some((Some(prev),)) = row.as_ref() {
                if let Ok(start) = chrono::DateTime::parse_from_rfc3339(prev) {
                    let dur = now.signed_duration_since(start.with_timezone(&Utc));
                    elapsed_add_ms = dur.num_milliseconds().max(0);
                }
            }
        }
        let new_run_started: Option<String> = if is_running {
            match row {
                Some((Some(prev),)) => Some(prev), // already running — keep segment
                _ => Some(now_rfc.clone()),        // (re)start — fresh segment
            }
        } else {
            None
        };

        sqlx::query(
            "UPDATE tasks SET status = ?, detail = ?, updated_at = ?, completed_at = ?, elapsed_ms = elapsed_ms + ?, run_started_at = ? WHERE id = ?",
        )
        .bind(&status_str)
        .bind(&detail_val)
        .bind(&now_rfc)
        .bind(&completed_at)
        .bind(elapsed_add_ms)
        .bind(&new_run_started)
        .bind(id)
        .execute(&self.db.pool)
        .await
        .context("set task status")?;
        Ok(())
    }

    async fn set_progress(&self, id: &str, current: i64, total: i64, detail: &str) -> Result<()> {
        self.set_progress_with_speed(id, current, total, detail, 0).await
    }

    async fn set_progress_with_speed(
        &self,
        id: &str,
        current: i64,
        total: i64,
        detail: &str,
        speed: i64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE tasks SET progress_current = ?, progress_total = ?, detail = ?, speed = ?, updated_at = ? WHERE id = ?",
        )
        .bind(current)
        .bind(total)
        .bind(detail)
        .bind(speed)
        .bind(&now)
        .bind(id)
        .execute(&self.db.pool)
        .await
        .context("set task progress")?;
        Ok(())
    }

    async fn set_speed(&self,
        id: &str,
        speed: i64,
    ) -> Result<()> {
        // Smooth the instantaneous speed with an EMA so the readout glides
        // instead of jittering on every poll. The first sample seeds the EMA
        // (no ramp-up from 0); α=0.3 favours the recent sample enough to track
        // real changes while damping aria2's per-poll noise.
        const ALPHA: f64 = 0.3;
        let smoothed = {
            let mut emas = self.ema_speeds.lock().await;
            let entry = emas.entry(id.to_string()).or_insert(speed as f64);
            *entry = ALPHA * (speed as f64) + (1.0 - ALPHA) * *entry;
            *entry
        };
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE tasks SET speed = ?, updated_at = ? WHERE id = ?",
        )
        .bind(smoothed as i64)
        .bind(&now)
        .bind(id)
        .execute(&self.db.pool)
        .await
        .context("set task speed")?;
        // Push the smoothed speed to the frontend so the card's bottom-right
        // readout glides; aria2 polls ~4×/sec.
        let _ = self.emit_progress(id).await;
        Ok(())
    }

    /// Drop the EMA state for a task (on pause/cancel/complete) so the next
    /// run seeds fresh instead of inheriting a stale value.
    async fn reset_speed(&self, id: &str, value: i64) -> Result<()> {
        self.ema_speeds.lock().await.remove(id);
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE tasks SET speed = ?, updated_at = ? WHERE id = ?")
            .bind(value)
            .bind(&now)
            .bind(id)
            .execute(&self.db.pool)
            .await
            .context("reset task speed")?;
        let _ = self.emit_progress(id).await;
        Ok(())
    }

    /// Accumulate downloaded bytes for the "共计 xxMB" completion readout.
    /// Called once per finished file with its size. Cheap increment, no event
    /// push — total_bytes is surfaced when the task completes via emit_progress.
    async fn add_bytes(&self, id: &str, n: i64) -> Result<()> {
        if n <= 0 {
            return Ok(());
        }
        sqlx::query("UPDATE tasks SET total_bytes = total_bytes + ? WHERE id = ?")
            .bind(n)
            .bind(id)
            .execute(&self.db.pool)
            .await
            .context("add task bytes")?;
        Ok(())
    }

    async fn append_log(
        &self,
        id: &str,
        line: &str,
    ) -> Result<()> {
        let row: Option<(String,)> = sqlx::query_as("SELECT logs FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.db.pool)
            .await
            .context("fetch task logs")?;
        let mut logs: Vec<String> = match row {
            Some((json,)) => serde_json::from_str(&json).unwrap_or_default(),
            None => return Ok(()),
        };
        // Prepend a local-time stamp so each log line reads "[HH:MM:SS.mmm]".
        let stamp = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
        logs.push(format!("[{stamp}] {line}"));
        // Keep only the most recent 200 lines so the JSON column does not grow forever.
        if logs.len() > 200 {
            let excess = logs.len() - 200;
            logs.drain(0..excess);
        }
        let json = serde_json::to_string(&logs).context("serialize task logs")?;
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE tasks SET logs = ?, updated_at = ? WHERE id = ?")
            .bind(json)
            .bind(&now)
            .bind(id)
            .execute(&self.db.pool)
            .await
            .context("append task log")?;
        Ok(())
    }

    async fn set_book_id(
        &self,
        id: &str,
        book_id: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE tasks SET book_id = ?, updated_at = ? WHERE id = ?")
            .bind(book_id)
            .bind(&now)
            .bind(id)
            .execute(&self.db.pool)
            .await
            .context("set task book_id")?;
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
            "SELECT id, source, status, title, detail, progress_current, progress_total, retry_count, max_retries, speed, logs, book_id, total_bytes, elapsed_ms, created_at, updated_at, completed_at, payload FROM tasks WHERE id = ?",
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
            Ok(book_id) => {
                if let Some(bid) = &book_id {
                    let _ = manager.set_book_id(&task_id, bid).await;
                    let _ = manager.append_log(&task_id, "task completed").await;
                }
                // Drop the EMA entry so the HashMap doesn't accumulate entries
                // for finished tasks across the app lifetime.
                manager.ema_speeds.lock().await.remove(&task_id);
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
                        .append_log(&task_id, &format!("task failed: {err_str}"))
                        .await;
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
                    .append_log(
                        &task_id,
                        &format!("attempt {retries} failed: {err_str}; retrying in {delay}s"),
                    )
                    .await;
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
) -> Result<Option<String>> {
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
) -> Result<Option<String>> {
    let client = PixivClient::new(cookie).context("build pixiv client")?;
    let library = LibraryService::new(manager.db.clone(), manager.storage.clone());

    manager
        .set_progress(&task.id, 0, 0, "listing works...")
        .await?;
    let _ = manager.append_log(&task.id, "listing works...").await;
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
    let _ = manager.append_log(&task.id, &format!("found {total} works")).await;
    let _ = manager.emit_progress(&task.id).await;

    let mut last_book_id: Option<String> = None;
    // Resume: skip works already completed in a previous run (progress_current
    // counts finished works). Clamp to the list length.
    let start = task.progress_current.max(0) as usize;
    for (idx, work) in works.iter().enumerate() {
        if idx < start {
            continue;
        }
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
        manager
            .set_progress(&task.id, current - 1, total, &work.title)
            .await?;
        let _ = manager
            .append_log(&task.id, &format!("downloading work {current}/{total}: {}", work.title))
            .await;
        let _ = manager.emit_progress(&task.id).await;

        match process_pixiv_work(manager, runtime, temp_dir, &client, &library, work, None).await {
            Ok(bid) => {
                if let Some(b) = bid {
                    last_book_id = Some(b);
                }
                let _ = manager
                    .append_log(&task.id, &format!("work {current}/{total} ok: {}", work.title))
                    .await;
            }
            Err(e) => {
                let msg = format!("work {current}/{total} failed: {e}", );
                let _ = manager.append_log(&task.id, &msg).await;
                tracing::warn!(
                    target: "erolib::tasks",
                    task_id = %task.id,
                    work_id = %work.id,
                    %e,
                    "work failed"
                );
            }
        }
    }

    manager
        .set_progress(&task.id, total, total, "done")
        .await?;
    let _ = manager.append_log(&task.id, "finished batch download").await;
    let _ = manager.emit_progress(&task.id).await;
    Ok(last_book_id)
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
) -> Result<Option<String>> {
    let client = PixivClient::new(cookie).context("build pixiv client")?;
    let library = LibraryService::new(manager.db.clone(), manager.storage.clone());

    manager
        .set_progress(&task.id, 0, 1, "fetching work...")
        .await?;
    let _ = manager.append_log(&task.id, &format!("fetching work {work_id}")).await;
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

    // Propagate the real registered book UUID so the task's "Read" button can
    // open it — process_pixiv_work returns Option<book_id> (the UUID), NOT the
    // Pixiv work id.
    let book_id = process_pixiv_work(manager, runtime, temp_dir, &client, &library, &work, Some(&task.id))
        .await?;

    manager
        .set_progress(&task.id, 1, 1, "done")
        .await?;
    let _ = manager.append_log(&task.id, "finished single work").await;
    let _ = manager.emit_progress(&task.id).await;
    Ok(book_id)
}

async fn process_pixiv_work(
    manager: &TaskManager,
    runtime: &TaskRuntime,
    temp_dir: &std::path::Path,
    client: &PixivClient,
    library: &LibraryService,
    work: &crate::services::pixiv::UserWork,
    task_id: Option<&str>,
) -> Result<Option<String>> {
    // Ugoira (動画作, illustType==2): a multi-frame animation zipped with
    // per-frame delays. Route to a dedicated handler that stores the jpg
    // frames + delays (the reader plays them on a canvas timer). Regular
    // manga (illustType==0/1) continues.
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
            return Ok(Some(prev.book_id.clone()));
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
        while runtime.paused.load(Ordering::Relaxed) {
            if runtime.cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("cancelled");
            }
            sleep(Duration::from_millis(500)).await;
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
        let page_no = pidx + 1;
        let page_total = pages.len();
        if let Some(tid) = task_id {
            let _ = manager
                .append_log(tid, &format!("downloading image {page_no}/{page_total}"))
                .await;
        }
        let gid = manager
            .aria2
            .add_uri(url, Some("https://www.pixiv.net/"), Some(&out), Some(temp_dir))
            .await
            .with_context(|| format!("add uri {}", url))?;
        let tid = task_id.unwrap_or("");
        let path = manager
            .aria2
            .wait_for_gid_with_progress(
                &gid,
                Duration::from_millis(250),
                &runtime.cancelled,
                &runtime.paused,
                |speed| async move {
                    if !tid.is_empty() {
                        let _ = manager.set_speed(tid, speed as i64).await;
                    }
                },
            )
            .await
            .with_context(|| format!("download {}", url))?;
        let bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("read {}", path.display()))?;
        if bytes.len() < 100 {
            anyhow::bail!("suspiciously small image from {}", url);
        }
        if let Some(tid) = task_id {
            let _ = manager.add_bytes(tid, bytes.len() as i64).await;
        }
        images.push(bytes);
        // For single-work tasks, report per-image progress so the card's ring
        // advances instead of spinning indefinitely. Batch tasks pass None and
        // are tracked at the work-index level by the caller.
        if let Some(tid) = task_id {
            let _ = manager
                .set_progress(tid, (pidx + 1) as i64, pages.len() as i64, "downloading")
                .await;
            let _ = manager
                .append_log(tid, &format!("image {page_no}/{page_total} ok"))
                .await;
            let _ = manager.emit_progress(tid).await;
        }
    }

    if images.is_empty() {
        anyhow::bail!("no images downloaded");
    }

    let source = BookSource {
        plugin: "pixiv".into(),
        source_url: source_url.clone(),
        scraped_at: Some(Utc::now()),
        source_post_id: Some(work.id.clone()),
        author: work.author.clone(),
        author_id: work.author_id.clone(),
        published_at: work.published_at.clone(),
    };

    if let Some(tid) = task_id {
        let _ = manager.append_log(tid, "packaging cb7...").await;
    }
    let file_path = manager
        .storage
        .create_cb7(
            &images,
            &BookMetadata {
                title: work.title.clone(),
                tags: work.tags.clone(),
                author: work.author.clone(),
                source_plugin: Some("pixiv".into()),
                source_url: Some(source_url),
                source_post_id: Some(work.id.clone()),
                published_at: work.published_at.clone(),
                scraped_at: source.scraped_at.map(|t| t.to_rfc3339()),
                ..Default::default()
            },
        )
        .context("create cb7")?;
    if let Some(tid) = task_id {
        let _ = manager.append_log(tid, "packaged cb7 ok").await;
    }

    library
        .register_stored_book(
            &book_id,
            &work.title,
            &file_path,
            images.len() as i32,
            Some(&source),
            &work.tags,
            None,
        )
        .await
        .context("register book")?;
    if let Some(tid) = task_id {
        let _ = manager
            .append_log(tid, &format!("registered book: {}", work.title))
            .await;
    }
    Ok(Some(book_id))
}

/// Download a ugoira (動画作) work: fetch its frame manifest, pull the original
/// zip, extract the per-frame jpgs (lossless, native resolution), and store
/// them as a multi-page cb7 plus the per-frame delays (DB). The reader plays
/// the jpg sequence on a canvas timer — no re-encode. The static cover comes
/// from the Pixiv thumbnail. Frame extraction runs off the async runtime.
async fn process_pixiv_ugoira(
    manager: &TaskManager,
    runtime: &TaskRuntime,
    temp_dir: &std::path::Path,
    client: &PixivClient,
    library: &LibraryService,
    work: &crate::services::pixiv::UserWork,
    task_id: Option<&str>,
) -> Result<Option<String>> {
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
    let tid = task_id.unwrap_or("");
    let zip_path = manager
        .aria2
        .wait_for_gid_with_progress(
            &gid,
            Duration::from_millis(250),
            &runtime.cancelled,
            &runtime.paused,
            |speed| async move {
                if !tid.is_empty() {
                    let _ = manager.set_speed(tid, speed as i64).await;
                }
            },
        )
        .await
        .context("download ugoira zip")?;
    let zip_bytes = tokio::fs::read(&zip_path)
        .await
        .context("read ugoira zip")?;
    if let Some(tid) = task_id {
        let _ = manager.add_bytes(tid, zip_bytes.len() as i64).await;
    }

    // 3. Extract the original jpg frames (no re-encoding — keeps them
    //    lossless, native resolution, and tiny; the reader plays the sequence
    //    on a timer using the per-frame delays). Only the zip decompression is
    //    CPU-bound, so run it off the async runtime.
    if let Some(tid) = task_id {
        let _ = manager
            .set_progress(tid, total_frames / 2, total_frames, "extracting frames...")
            .await;
        let _ = manager.emit_progress(tid).await;
    }
    let frame_names = meta.frames.iter().map(|f| f.file.clone()).collect::<Vec<_>>();
    let images = tokio::task::spawn_blocking(move || extract_ugoira_frames(&zip_bytes, &frame_names))
        .await
        .context("join extract task")??;
    let delays_json = serde_json::to_string(
        &meta.frames.iter().map(|f| f.delay).collect::<Vec<_>>(),
    )
    .context("serialize ugoira delays")?;

    if let Some(tid) = task_id {
        let _ = manager
            .set_progress(tid, total_frames, total_frames, "finalizing...")
            .await;
        let _ = manager.emit_progress(tid).await;
    }

    // 4. Store the jpg sequence as an N-page book (one jpg per frame), with the
    //    per-frame delays recorded so the reader can play it. Idempotent: a
    //    previously-imported ugoira (same title, same frame count) is kept.
    let source_url = format!("https://www.pixiv.net/artworks/{}", work.id);
    let existing = find_existing_by_source(&manager.db.pool, &source_url).await?;
    let book_id = if let Some(prev) = existing {
        if prev.page_count == 1 && prev.title == work.title {
            if !work.tags.is_empty() {
                let _ = library.link_tags(&prev.book_id, &work.tags).await;
            }
            return Ok(Some(prev.book_id.clone()));
        }
        library
            .remove_book(&prev.book_id)
            .await
            .context("remove old book")?;
        prev.book_id
    } else {
        Uuid::new_v4().to_string()
    };

    let source = BookSource {
        plugin: "pixiv".into(),
        source_url: source_url.clone(),
        scraped_at: Some(Utc::now()),
        source_post_id: Some(work.id.clone()),
        author: work.author.clone(),
        author_id: work.author_id.clone(),
        published_at: work.published_at.clone(),
    };

    if let Some(tid) = task_id {
        let _ = manager.append_log(tid, "packaging cb7...").await;
    }
    let file_path = manager
        .storage
        .create_cb7(
            &images,
            &BookMetadata {
                title: work.title.clone(),
                tags: work.tags.clone(),
                author: work.author.clone(),
                source_plugin: Some("pixiv".into()),
                source_url: Some(source_url),
                source_post_id: Some(work.id.clone()),
                published_at: work.published_at.clone(),
                scraped_at: source.scraped_at.map(|t| t.to_rfc3339()),
                delays: Some(delays_json.clone()),
                ..Default::default()
            },
        )
        .context("create cb7 (ugoira)")?;
    if let Some(tid) = task_id {
        let _ = manager.append_log(tid, "packaged cb7 ok").await;
    }

    library
        .register_stored_book(
            &book_id,
            &work.title,
            &file_path,
            1, // animated books are logically a single "page" (played as a loop)
            Some(&source),
            &work.tags,
            Some(&delays_json),
        )
        .await
        .context("register ugoira book")?;
    if let Some(tid) = task_id {
        let _ = manager.set_book_id(tid, &book_id).await;
        let _ = manager.append_log(tid, &format!("registered ugoira book: {}", work.title)).await;
    }

    // The cb7's first frame makes a poor cover (often a transition frame) —
    // overwrite it with Pixiv's own thumbnail (cover_url from the detail API).
    if let Some(url) = work.cover_url.as_deref().filter(|u| !u.is_empty()) {
        if let Ok(bytes) = client.download_image(url).await {
            let cover = manager.storage.cover_path.join(format!("{book_id}.jpg"));
            let _ = std::fs::write(&cover, &bytes);
        }
    }
    Ok(Some(book_id))
}

/// Extract the ugoira zip's jpg frames in manifest order, returning the raw
/// jpg bytes untouched — lossless, native resolution, tiny. The reader plays
/// the sequence on a canvas timer using the manifest delays, so there's no
/// re-encode: fast to produce, fast to load page-by-page.
fn extract_ugoira_frames(
    zip_bytes: &[u8],
    frame_names: &[String],
) -> Result<Vec<Vec<u8>>> {
    use std::io::{Cursor, Read};

    let cursor = Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).context("open ugoira zip")?;

    let mut images: Vec<Vec<u8>> = Vec::with_capacity(frame_names.len());
    for name in frame_names {
        let mut buf = Vec::new();
        {
            let mut entry = archive
                .by_name(name)
                .with_context(|| format!("locate ugoira frame {}", name))?;
            entry.read_to_end(&mut buf)?;
        } // entry dropped here, releasing the archive borrow for the next frame
        if buf.len() < 100 {
            anyhow::bail!("suspiciously small ugoira frame {}", name);
        }
        images.push(buf);
    }
    if images.is_empty() {
        anyhow::bail!("no ugoira frames extracted");
    }
    Ok(images)
}

// ====================== EHentai processing ======================

async fn process_ehentai(
    manager: &TaskManager,
    task: &crate::services::task::Task,
    runtime: &TaskRuntime,
    temp_dir: &std::path::Path,
    cookie: &str,
    gallery_url: &str,
    gid: &str,
    token: &str,
) -> Result<Option<String>> {
    // Honour the source site — ex-only galleries 404 on e-hentai.org.
    let ex = gallery_url.contains("exhentai");
    let client = EhentaiClient::new(cookie, ex).context("build ehentai client")?;
    let library = LibraryService::new(manager.db.clone(), manager.storage.clone());

    manager
        .set_progress(&task.id, 0, 0, "listing pages...")
        .await?;
    let _ = manager.append_log(&task.id, "listing gallery pages...").await;
    let _ = manager.emit_progress(&task.id).await;

    let page_urls = client
        .fetch_gallery_pages(gid, token)
        .await
        .context("fetch gallery pages")?;

    // Best-effort source metadata (posted time + uploader) for the library card.
    let meta = client
        .fetch_gallery_meta(gid, token)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                target: "erolib::tasks",
                task_id = %task.id,
                gid = %gid,
                error = %e,
                "fetch_gallery_meta failed; falling back to empty metadata (title reverts to task title)"
            );
            crate::services::ehentai::GalleryMeta::default()
        });

    let total = page_urls.len() as i64;
    manager
        .set_progress(&task.id, 0, total, "downloading...")
        .await?;
    let _ = manager.append_log(&task.id, &format!("found {total} pages")).await;
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

        // Resume support: if this page was already downloaded to the temp dir
        // (previous run paused/killed mid-flight), reuse its bytes instead of
        // re-fetching. aria2 writes exactly `page-{idx:04}` (no extension).
        let out = format!("page-{:04}", idx);
        let cached_path = temp_dir.join(&out);
        if cached_path.is_file() {
            if let Ok(bytes) = tokio::fs::read(&cached_path).await {
                if bytes.len() >= 200 {
                    let _ = manager
                        .append_log(&task.id, &format!("page {current}/{total} (cached)"))
                        .await;
                    images.push(bytes);
                    continue;
                }
            }
        }

        let _ = manager
            .append_log(&task.id, &format!("downloading page {current}/{total}"))
            .await;
        match client.fetch_page_image(page_url).await {
            Ok(img_url) => {
                let aria_gid = match manager
                    .aria2
                    .add_uri(
                        &img_url,
                        Some(if ex { "https://exhentai.org/" } else { "https://e-hentai.org/" }),
                        Some(&out),
                        Some(temp_dir),
                    )
                    .await
                {
                    Ok(g) => g,
                    Err(e) => {
                        let _ = manager
                            .append_log(&task.id, &format!("page {current} add_uri failed: {e}"))
                            .await;
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
                    .wait_for_gid_with_progress(
                        &aria_gid,
                        Duration::from_millis(250),
                        &runtime.cancelled,
                        &runtime.paused,
                        |speed| {
                            let task_id = task.id.clone();
                            async move {
                                let _ = manager.set_speed(&task_id, speed as i64).await;
                            }
                        },
                    )
                    .await
                {
                    Ok(path) => match tokio::fs::read(&path).await {
                        Ok(bytes) => {
                            if bytes.len() < 200 {
                                let _ = manager
                                    .append_log(&task.id, &format!("page {current} too small"))
                                    .await;
                                tracing::warn!(
                                    target: "erolib::tasks",
                                    task_id = %task.id,
                                    img_url = %img_url,
                                    "suspiciously small image"
                                );
                            } else {
                                let _ = manager
                                    .add_bytes(&task.id, bytes.len() as i64)
                                    .await;
                                let _ = manager
                                    .append_log(&task.id, &format!("page {current}/{total} ok"))
                                    .await;
                                images.push(bytes);
                            }
                        }
                        Err(e) => {
                            let _ = manager
                                .append_log(&task.id, &format!("page {current} read failed: {e}"))
                                .await;
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
                        let _ = manager
                            .append_log(&task.id, &format!("page {current} download failed: {e}"))
                            .await;
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
                let _ = manager
                    .append_log(&task.id, &format!("page {current} fetch failed: {e}"))
                    .await;
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
        .set_progress(&task.id, total, total, "packaging...")
        .await?;
    let _ = manager.append_log(&task.id, "packaging cb7...").await;
    let _ = manager.emit_progress(&task.id).await;

    let source_url = format!(
        "https://{}/g/{}/{}/",
        if ex { "exhentai.org" } else { "e-hentai.org" },
        gid,
        token,
    );
    let source = BookSource {
        plugin: (if ex { "exhentai" } else { "e-hentai" }).into(),
        source_url: source_url.clone(),
        scraped_at: Some(Utc::now()),
        source_post_id: Some(gid.to_string()),
        author: meta.uploader.clone(),
        author_id: None,
        published_at: meta.posted.clone(),
    };

    // Prefer the scraped gallery title; fall back to the task title only if
    // the gallery page had no parseable title. Keep the same value for both
    // the CB7 metadata and the library row so the book list looks consistent.
    let title = if meta.title.is_empty() {
        task.title.clone()
    } else {
        meta.title.clone()
    };

    let file_path = manager
        .storage
        .create_cb7(
            &images,
            &BookMetadata {
                title: title.clone(),
                tags: meta.tags.clone(),
                author: meta.uploader.clone(),
                source_plugin: Some((if ex { "exhentai" } else { "e-hentai" }).into()),
                source_url: Some(source_url),
                source_post_id: Some(gid.to_string()),
                published_at: meta.posted.clone(),
                scraped_at: source.scraped_at.map(|t| t.to_rfc3339()),
                ..Default::default()
            },
        )
        .context("create cb7")?;
    let _ = manager.append_log(&task.id, "packaged cb7 ok").await;

    let book_id = Uuid::new_v4().to_string();
    library
        .register_stored_book(
            &book_id,
            &title,
            &file_path,
            images.len() as i32,
            Some(&source),
            &meta.tags,
            None,
        )
        .await
        .context("register book")?;

    let _ = manager.set_book_id(&task.id, &book_id).await;
    let _ = manager.append_log(&task.id, &format!("registered book: {title}")).await;

    manager
        .set_progress(&task.id, total, total, "done")
        .await?;
    let _ = manager.append_log(&task.id, "done").await;
    let _ = manager.emit_progress(&task.id).await;
    Ok(Some(book_id))
}
