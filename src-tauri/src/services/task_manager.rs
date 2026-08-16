use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
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
use crate::services::aria2::ProgressUpdate;
use crate::services::{Aria2Client, AhentaiClient, EhentaiClient, LibraryService, StorageService};

/// Aggregated per-slot progress for one in-flight image download: (completed
/// bytes, total bytes, instantaneous speed). Shared between the per-gid aria2
/// poll callback (writer) and the per-task ticker (reader/sum).
type SlotProgress = (i64, i64, i64);

/// RAII handle that aborts the progress ticker when dropped — covers normal
/// return AND early `?`/bail paths so the ticker never outlives its loop.
struct TickerGuard(tokio::task::JoinHandle<()>);
impl Drop for TickerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

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

    /// Delete every completed task in one shot.
    pub async fn clear_completed_tasks(&self) -> Result<u64> {
        let res = sqlx::query(
            "DELETE FROM tasks WHERE status = 'completed'",
        )
        .execute(&self.db.pool)
        .await
        .context("clear completed tasks")?;
        Ok(res.rows_affected())
    }

    /// Retry all failed tasks + resume all paused tasks in one shot.
    /// Returns (retried, resumed) counts.
    pub async fn retry_and_resume_all(&self) -> Result<(u64, u64)> {
        let failed: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM tasks WHERE status = 'failed'",
        )
        .fetch_all(&self.db.pool)
        .await
        .context("list failed tasks")?;

        let paused: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM tasks WHERE status = 'paused'",
        )
        .fetch_all(&self.db.pool)
        .await
        .context("list paused tasks")?;

        let retried = failed.len() as u64;
        let resumed = paused.len() as u64;

        for id in &failed {
            if let Err(e) = self.retry_task(id).await {
                tracing::warn!(target: "erolib::tasks", %e, task_id = %id, "retry_all: retry failed");
            }
        }
        for id in &paused {
            if let Err(e) = self.resume_task(id).await {
                tracing::warn!(target: "erolib::tasks", %e, task_id = %id, "retry_all: resume failed");
            }
        }

        Ok((retried, resumed))
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

        let _ = self.append_log(&id, "📋 创建任务").await;
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
        let _ = self.append_log(id, "⏸ 暂停").await;
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
        let _ = self.append_log(id, "▶ 恢复").await;
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
        let _ = self.append_log(id, "⏹ 取消").await;
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
        let _ = self.append_log(id, "🔄 重试任务").await;
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

        let result = process_task(manager.clone(), &task, runtime.clone()).await;

        // If cancelled while running (user cancel, or a sibling page-failure
        // that flipped the flag to drain in-flight downloads), don't overwrite
        // the Cancelled status with Completed/retry — clean up and return.
        if runtime.cancelled.load(Ordering::Relaxed) {
            let mut workers = manager.workers.lock().await;
            workers.remove(&task_id);
            return;
        }

        match result {
            Ok(book_id) => {
                if let Some(bid) = &book_id {
                    let _ = manager.set_book_id(&task_id, bid).await;
                    let _ = manager.append_log(&task_id, "✅ 任务完成").await;
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
    manager: Arc<TaskManager>,
    task: &crate::services::task::Task,
    runtime: Arc<TaskRuntime>,
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
        TaskPayload::EhentaiGallery {
            cookie,
            gallery_url,
            gid,
            token,
        } => {
            process_ehentai(manager.clone(), task, runtime.clone(), &temp_dir, cookie, gallery_url, gid, token).await
        }
        TaskPayload::PixivSingleWork { cookie, work_id } => {
            process_pixiv_single(manager.clone(), task, runtime.clone(), &temp_dir, cookie, work_id).await
        }
        TaskPayload::AhentaiGallery { gallery_id, title } => {
            process_ahentai(manager.clone(), task, runtime.clone(), &temp_dir, gallery_id, title).await
        }
        TaskPayload::NicecatGallery { comic_id, title } => {
            process_nicecat(manager.clone(), task, runtime.clone(), comic_id, title).await
        }
    }
}

/// Download a single Pixiv artwork (clicked from the browse grid). Reuses
/// `process_pixiv_work` after resolving the work's metadata via the detail API.
async fn process_pixiv_single(
    manager: Arc<TaskManager>,
    task: &crate::services::task::Task,
    runtime: Arc<TaskRuntime>,
    temp_dir: &std::path::Path,
    cookie: &str,
    work_id: &str,
) -> Result<Option<String>> {
    let client = PixivClient::new(cookie).context("build pixiv client")?;
    let library = LibraryService::new(manager.db.clone(), manager.storage.clone());

    manager
        .set_progress(&task.id, 0, 1, "fetching work...")
        .await?;
    let _ = manager.append_log(&task.id, &format!("🔍 抓取作品 {work_id} 信息")).await;
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
    let book_id = process_pixiv_work(manager.clone(), runtime.clone(), temp_dir, &client, &library, &work, Some(&task.id))
        .await?;

    manager
        .set_progress(&task.id, 1, 1, "done")
        .await?;
    let _ = manager.append_log(&task.id, "✅ 完成").await;
    let _ = manager.emit_progress(&task.id).await;
    Ok(book_id)
}

/// Spawn a background ticker that aggregates per-slot speed (~2.5×/sec) and
/// pushes a smooth speed readout for the task. The progress bar itself is
/// driven solely by image-count updates from `download_pages_concurrent`
/// (one tick per completed page) — the ticker never overwrites it with
/// byte-level aria2 progress, so the bar always shows "pages done / total
/// pages" regardless of whether the content is an image gallery or a
/// ugoira/animated zip. Returns a `TickerGuard` whose Drop aborts the
/// ticker. No-op (dummy guard) when `task_id` is None — batch tasks don't
/// have their own task_id.
fn spawn_progress_ticker(
    manager: &Arc<TaskManager>,
    runtime: Arc<TaskRuntime>,
    task_id: Option<&str>,
    progress: &Arc<std::sync::Mutex<HashMap<usize, SlotProgress>>>,
) -> TickerGuard {
    let tid = match task_id.map(|s| s.to_string()) {
        Some(t) => t,
        None => return TickerGuard(tokio::spawn(async {})),
    };
    let manager = Arc::clone(manager);
    let progress = Arc::clone(progress);
    TickerGuard(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(400));
        interval.tick().await; // discard the immediate first tick
        loop {
            interval.tick().await;
            if runtime.cancelled.load(Ordering::Relaxed) {
                break;
            }
            // Sum only the speed component across all in-flight gids so the
            // speed readout reflects aggregate throughput. The byte totals
            // (agg.0/agg.1) are intentionally ignored — the progress bar
            // tracks completed images, not partial byte downloads.
            let agg_speed = progress
                .lock()
                .map(|g| g.values().fold(0i64, |s, v| s + v.2))
                .unwrap_or(0);
            let _ = manager.set_speed(&tid, agg_speed).await;
        }
    }))
}

/// Download one image via aria2 with a Rust-side retry (2 retries, 1s/2s
/// backoff) layered on aria2's built-in `max-tries=5`. Owns every handle
/// (`Arc<TaskManager>` + `Arc<TaskRuntime>`) and takes only owned / `'static`
/// arguments, so the returned future is `'static + Send` — required because
/// each per-image download is `tokio::spawn`'d as its own task (bounded by a
/// semaphore) inside the worker. `slot` + `progress` feed the per-task ticker a
/// byte-level view of this gid so the progress bar glides mid-download.
/// Returns `Ok(None)` for an empty URL (caller skips the page).
async fn download_one_image(
    manager: Arc<TaskManager>,
    runtime: Arc<TaskRuntime>,
    url: String,
    referer: &'static str,
    origin: Option<&'static str>,
    out: String,
    temp_dir: std::path::PathBuf,
    min_bytes: usize,
    slot: usize,
    progress: Arc<std::sync::Mutex<HashMap<usize, SlotProgress>>>,
) -> Result<Option<Vec<u8>>> {
    if runtime.cancelled.load(Ordering::Relaxed) {
        anyhow::bail!("cancelled");
    }
    while runtime.paused.load(Ordering::Relaxed) {
        if runtime.cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("cancelled");
        }
        sleep(Duration::from_millis(500)).await;
    }
    if url.is_empty() {
        return Ok(None);
    }

    let backoffs = [Duration::from_secs(1), Duration::from_secs(2)];
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..3u32 {
        if attempt > 0 {
            // Cancel-aware backoff so pause→cancel still aborts promptly.
            let ticks = backoffs[(attempt as usize) - 1].as_secs() * 2;
            for _ in 0..ticks {
                if runtime.cancelled.load(Ordering::Relaxed) {
                    anyhow::bail!("cancelled");
                }
                sleep(Duration::from_millis(500)).await;
            }
        }
        let gid = match manager
            .aria2
            .add_uri(&url, Some(referer), origin, Some(&out), Some(&temp_dir))
            .await
        {
            Ok(g) => g,
            Err(e) => {
                last_err = Some(e.context(format!("add uri {}", url)));
                continue;
            }
        };
        let path = match manager
            .aria2
            .wait_for_gid_with_progress(
                &gid,
                Duration::from_millis(250),
                &runtime.cancelled,
                &runtime.paused,
                {
                    // Record this gid's latest byte progress so the per-task
                    // ticker can sum across all in-flight downloads.
                    let progress = Arc::clone(&progress);
                    move |upd: ProgressUpdate| {
                        let progress = Arc::clone(&progress);
                        async move {
                            if let Ok(mut g) = progress.lock() {
                                g.insert(
                                    slot,
                                    (upd.completed_length as i64, upd.total_length as i64, upd.speed as i64),
                                );
                            }
                        }
                    }
                },
            )
            .await
        {
            Ok(p) => p,
            Err(e) => {
                let _ = manager.aria2.remove(&gid).await;
                last_err = Some(e.context(format!("download {}", url)));
                continue;
            }
        };
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) => {
                let _ = manager.aria2.remove(&gid).await;
                last_err = Some(anyhow::anyhow!("read {}: {e}", path.display()));
                continue;
            }
        };
        if bytes.len() < min_bytes {
            let _ = manager.aria2.remove(&gid).await;
            last_err = Some(anyhow::anyhow!("suspiciously small image from {}", url));
            continue;
        }
        return Ok(Some(bytes));
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("download failed: {}", url)))
}

/// A previously-registered library book for the same source work. Used by the
/// per-source processors' "already in library" guard so a manual retry of a
/// task that actually finished registering (but died right at the end, or is
/// being re-run alongside an existing completed copy) returns the existing
/// book instead of packaging + inserting a duplicate row.
struct ExistingBook {
    book_id: String,
    page_count: i32,
}

/// Look up a library book by its exact `source_url`. Matches the URLs the
/// processors stamp when registering (`…/g/{id}/` for AHentai, the ncmm.cc
/// info URL for NiceCat, the e/exhentai gallery URL for EHentai).
async fn find_book_by_source_url(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    source_url: &str,
) -> Result<Option<ExistingBook>> {
    let row: Option<(String, i32)> =
        sqlx::query_as("SELECT id, page_count FROM books WHERE source_url = ? LIMIT 1")
            .bind(source_url)
            .fetch_optional(pool)
            .await
            .context("find book by source_url")?;
    Ok(row.map(|(book_id, page_count)| ExistingBook { book_id, page_count }))
}

/// How `download_pages_concurrent` should treat a single-page failure.
enum PageErrorPolicy {
    /// One failed page fails the whole task (Pixiv semantics: incomplete book
    /// sequence is unacceptable).
    FailWholeTask,
    /// Failed pages are skipped; survivors are packaged (EHentai/ASMHentai/
    /// NiceCat semantics).
    SkipPage,
}

/// Description of one page to download via aria2 inside a task worker.
struct PageDownload {
    index: usize,
    url: String,
    out: String,
    referer: &'static str,
    origin: Option<&'static str>,
    min_bytes: usize,
}

/// Shared concurrent page downloader used by every source processor.
///
/// Spawns all pages into an 8-concurrent aria2 JoinSet, draining in real time.
/// Downloaded images are persisted to `temp_dir` as individual files so the
/// next run can resume via file cache.
async fn download_pages_concurrent(
    manager: Arc<TaskManager>,
    runtime: Arc<TaskRuntime>,
    task_id: &str,
    temp_dir: PathBuf,
    pages: Vec<PageDownload>,
    resume_from_temp: bool,
    policy: PageErrorPolicy,
) -> Result<Vec<Option<Vec<u8>>>> {
    let total = pages.len();
    let mut results: Vec<Option<Vec<u8>>> = vec![None; total];
    if total == 0 {
        return Ok(results);
    }

    let sem = Arc::new(Semaphore::new(8));
    let progress_state: Arc<std::sync::Mutex<HashMap<usize, SlotProgress>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));
    let _ticker = spawn_progress_ticker(&manager, Arc::clone(&runtime), Some(task_id), &progress_state);

    // Resume: load any pages already persisted to temp_dir.
    let mut completed = 0usize;
    if resume_from_temp {
        for page in &pages {
            let cached = temp_dir.join(&page.out);
            if cached.is_file() {
                if let Ok(bytes) = tokio::fs::read(&cached).await {
                    if bytes.len() >= page.min_bytes {
                        let len = bytes.len();
                        results[page.index] = Some(bytes);
                        let _ = manager.add_bytes(task_id, len as i64).await;
                        completed += 1;
                        let _ = manager.set_progress(task_id, completed as i64, total as i64, "downloading").await;
                        let _ = manager.emit_progress(task_id).await;
                    }
                }
            }
        }
    }
    if completed > 0 {
        let _ = manager.append_log(task_id, &format!("📥 已恢复 {completed}/{total} 页 (缓存命中)")).await;
    }

    // Spawn all remaining pages into the JoinSet — aria2/reqwest auto-consume
    // from the 8-wide semaphore. Log one line per completed page so every
    // module gets the same fine-grained log format.
    let remaining: Vec<&PageDownload> = pages.iter().filter(|p| results[p.index].is_none()).collect();

    if !remaining.is_empty() {
        let _ = manager.append_log(task_id, &format!("⬇ 开始下载 {} 页 (8 并发)", remaining.len())).await;

        let mut set: JoinSet<(usize, Result<Option<Vec<u8>>>)> = JoinSet::new();
        for page in &remaining {
            if runtime.cancelled.load(Ordering::Relaxed) {
                break;
            }
            while runtime.paused.load(Ordering::Relaxed) {
                if runtime.cancelled.load(Ordering::Relaxed) {
                    break;
                }
                sleep(Duration::from_millis(500)).await;
            }
            let permit = sem.clone().acquire_owned().await.unwrap();
            let mgr = Arc::clone(&manager);
            let rt = Arc::clone(&runtime);
            let dir = temp_dir.clone();
            let prg = Arc::clone(&progress_state);
            let url = page.url.clone();
            let out = page.out.clone();
            let idx = page.index;
            let referer = page.referer;
            let origin = page.origin;
            let min_bytes = page.min_bytes;
            set.spawn(async move {
                let _permit = permit;
                let r = download_one_image(mgr, rt, url, referer, origin, out, dir, min_bytes, idx, prg).await;
                (idx, r)
            });
        }

        let mut failed: Option<anyhow::Error> = None;
        while let Some(res) = set.join_next().await {
            match res {
                Ok((idx, Ok(Some(bytes)))) => {
                    let len = bytes.len();
                    results[idx] = Some(bytes);
                    let _ = manager.add_bytes(task_id, len as i64).await;
                    completed += 1;
                    let _ = manager.append_log(task_id, &format!("📥 第 {completed}/{total} 页 完成")).await;
                    let _ = manager.set_progress(task_id, completed as i64, total as i64, "downloading").await;
                    let _ = manager.emit_progress(task_id).await;
                }
                Ok((_idx, Ok(None))) => {}
                Ok((idx, Err(e))) => match policy {
                    PageErrorPolicy::FailWholeTask => {
                        runtime.cancelled.store(true, Ordering::Relaxed);
                        failed = Some(e);
                        break;
                    }
                    PageErrorPolicy::SkipPage => {
                        let current = idx + 1;
                        let _ = manager.append_log(task_id, &format!("❌ 第 {current} 页失败: {e}")).await;
                    }
                },
                Err(e) => {
                    runtime.cancelled.store(true, Ordering::Relaxed);
                    failed = Some(anyhow::anyhow!("download task panicked: {e}"));
                    break;
                }
            }
        }
        while set.join_next().await.is_some() {}

        if let Some(e) = failed {
            return Err(e);
        }
    }

    Ok(results)
}

async fn process_pixiv_work(
    manager: Arc<TaskManager>,
    runtime: Arc<TaskRuntime>,
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
        return process_pixiv_ugoira(&manager, &runtime, temp_dir, client, library, work, task_id)
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

    let page_total = pages.len();
    let tid_opt = task_id;

    let downloads: Vec<PageDownload> = pages
        .iter()
        .enumerate()
        .map(|(pidx, page)| PageDownload {
            index: pidx,
            url: if page.urls.original.is_empty() {
                page.urls.regular.clone()
            } else {
                page.urls.original.clone()
            },
            out: format!("{:04}", pidx),
            referer: "https://www.pixiv.net/",
            origin: None,
            min_bytes: 100,
        })
        .collect();

    let mut images = if let Some(tid) = tid_opt {
        download_pages_concurrent(
            Arc::clone(&manager),
            Arc::clone(&runtime),
            tid,
            temp_dir.to_path_buf(),
            downloads,
            false,
            PageErrorPolicy::FailWholeTask,
        )
        .await?
    } else {
        // Batch sub-tasks don't have their own task_id; run with a transient id
        // just for progress bookkeeping.
        download_pages_concurrent(
            Arc::clone(&manager),
            Arc::clone(&runtime),
            &Uuid::new_v4().to_string(),
            temp_dir.to_path_buf(),
            downloads,
            false,
            PageErrorPolicy::FailWholeTask,
        )
        .await?
    };

    if let Some(tid) = tid_opt {
        let completed = images.iter().filter(|o| o.is_some()).count() as i64;
        if completed > 0 {
            let _ = manager
                .append_log(tid, &format!("📥 已下载 {completed}/{page_total} 页"))
                .await;
        }
    }

    let images: Vec<Vec<u8>> = images.iter_mut().map(|o| o.take()).flatten().collect();

    if images.is_empty() {
        anyhow::bail!("no images downloaded");
    }

    let source = BookSource {
        source_plugin: "pixiv".into(),
        source_url: source_url.clone(),
        scraped_at: Some(Utc::now()),
        source_post_id: Some(work.id.clone()),
        author: work.author.clone(),
        author_id: work.author_id.clone(),
        published_at: work.published_at.clone(),
    };

    if let Some(tid) = task_id {
        let _ = manager.append_log(tid, "📦 打包 CB7").await;
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
        let _ = manager.append_log(tid, "📦 打包完成").await;
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
            .append_log(tid, &format!("📚 注册书籍: {}", work.title))
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
            None,
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
            |upd: ProgressUpdate| async move {
                if !tid.is_empty() {
                    let _ = manager.set_speed(tid, upd.speed as i64).await;
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
        source_plugin: "pixiv".into(),
        source_url: source_url.clone(),
        scraped_at: Some(Utc::now()),
        source_post_id: Some(work.id.clone()),
        author: work.author.clone(),
        author_id: work.author_id.clone(),
        published_at: work.published_at.clone(),
    };

    if let Some(tid) = task_id {
        let _ = manager.append_log(tid, "📦 打包 CB7 (ugoira)").await;
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
        let _ = manager.append_log(tid, "📦 打包完成").await;
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
        let _ = manager.append_log(tid, &format!("📚 注册书籍: {}", work.title)).await;
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
    manager: Arc<TaskManager>,
    task: &crate::services::task::Task,
    runtime: Arc<TaskRuntime>,
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

    // Already-in-library guard for manual retry after a task died at the very
    // last step: if the gallery is registered, adopt the existing book instead
    // of re-downloading and inserting a duplicate.
    {
        let source_url = format!(
            "https://{}/g/{}/{}/",
            if ex { "exhentai.org" } else { "e-hentai.org" },
            gid,
            token,
        );
        if let Some(prev) = find_book_by_source_url(&manager.db.pool, &source_url).await? {
            let total = prev.page_count as i64;
            let _ = manager
                .append_log(&task.id, "📚 已在书库中，跳过下载")
                .await;
            manager
                .set_progress(&task.id, total, total, "done")
                .await?;
            let _ = manager.emit_progress(&task.id).await;
            return Ok(Some(prev.book_id));
        }
    }

    manager
        .set_progress(&task.id, 0, 0, "listing pages...")
        .await?;
    let _ = manager.append_log(&task.id, "🔍 抓取画廊页面列表").await;
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
    let _ = manager.append_log(&task.id, &format!("🔍 发现 {total} 页")).await;
    let _ = manager.emit_progress(&task.id).await;

    // Two-phase download (critical for avoiding e-hentai rate limits):
    //   Phase 1 (serial): the `/s/` page-view HTML scrape (`fetch_page_image`)
    //     is cookie-gated and hard rate-limited, so it stays serial with the
    //     original ~400ms cadence. Resume cache hits skip straight to the slot.
    //   Phase 2 (8 concurrent): the resolved image-CDN URLs only need a
    //     Referer (not the cookie), so they fan out 8-wide through aria2.
    //     Each is `tokio::spawn`'d as its own `'static + Send` task bounded by
    //     a Semaphore(8); results land in a pre-sized Vec indexed by page,
    //     preserving cb7 page order regardless of completion order.
    let mut results: Vec<Option<Vec<u8>>> = vec![None; page_urls.len()];
    let mut done_count: i64 = 0;

    manager
        .set_progress(&task.id, 0, total, "resolving pages")
        .await?;
    let _ = manager.emit_progress(&task.id).await;

    // ---- Phase 1: serial resolve (rate-limited page-view scrape) ----
    // Collected pending downloads: (page_idx, resolved_img_url, out_name).
    let mut pending: Vec<(usize, String, String)> = Vec::new();
    // Advance the progress bar per resolved page so big galleries don't look
    // frozen at 0/total during the serial (~400ms/page) resolve phase.
    let mut resolved: i64 = 0;
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
        let out = format!("page-{:04}", idx);

        // Resume support: if this page was already downloaded to the temp dir
        // (previous run paused/killed mid-flight), reuse its bytes. aria2
        // writes exactly `page-{idx:04}` (no extension).
        let cached_path = temp_dir.join(&out);
        let mut cache_hit = false;
        if cached_path.is_file() {
            if let Ok(bytes) = tokio::fs::read(&cached_path).await {
                if bytes.len() >= 200 {
                    let _ = manager
                        .append_log(&task.id, &format!("📥 第 {current}/{total} 页 完成 (缓存)"))
                        .await;
                    results[idx] = Some(bytes);
                    done_count += 1;
                    cache_hit = true;
                }
            }
        }
        if cache_hit {
            resolved += 1;
            let _ = manager
                .set_progress(&task.id, resolved, total, "resolving pages")
                .await;
            let _ = manager.emit_progress(&task.id).await;
            // Cached pages skip the network — preserve the old behaviour of
            // skipping the rate-limit sleep for them.
            continue;
        }

        let _ = manager
            .append_log(&task.id, &format!("📥 解析第 {current}/{total} 页"))
            .await;
        match client.fetch_page_image(page_url).await {
            Ok(img_url) => {
                pending.push((idx, img_url, out));
            }
            Err(e) => {
                let _ = manager
                    .append_log(&task.id, &format!("❌ 第 {current} 页抓取失败: {e}"))
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
        resolved += 1;
        let _ = manager
            .set_progress(&task.id, resolved, total, "resolving pages")
            .await;
        let _ = manager.emit_progress(&task.id).await;
        // Preserve the ~400ms rate-limit cadence between page-view scrapes.
        sleep(Duration::from_millis(400)).await;
    }

    if done_count > 0 {
        let _ = manager
            .set_progress(&task.id, done_count, total, "downloading")
            .await;
        let _ = manager.emit_progress(&task.id).await;
    }

    // ---- Phase 2: concurrent image download via shared helper ----
    if !pending.is_empty() {
        let _ = manager
            .append_log(
                &task.id,
                &format!("⬇ 开始下载 {} 页 (8 并发)", pending.len()),
            )
            .await;

        let referer: &'static str = if ex {
            "https://exhentai.org/"
        } else {
            "https://e-hentai.org/"
        };
        let downloads: Vec<PageDownload> = pending
            .iter()
            .enumerate()
            .map(|(slot, (_orig_idx, img_url, out))| PageDownload {
                index: slot,
                url: img_url.clone(),
                out: out.clone(),
                referer,
                origin: None,
                min_bytes: 200,
            })
            .collect();

        let new_images = download_pages_concurrent(
            Arc::clone(&manager),
            Arc::clone(&runtime),
            &task.id,
            temp_dir.to_path_buf(),
            downloads,
            true,
            PageErrorPolicy::SkipPage,
        )
        .await?;

        // Merge back into the full results array using original page indices
        // stored in the pending list (the serial resolve phase placed them
        // sequentially, covering the full 0..total range).
        for (slot, (orig_idx, _img_url, _out)) in pending.iter().enumerate() {
            if let Some(bytes) = &new_images[slot] {
                results[*orig_idx] = Some(bytes.clone());
            }
        }
    }

    // Cancelled mid-flight (user cancel): bail before packaging so we don't
    // register a partial gallery. In-flight downloads already removed their
    // aria2 gids via the cancel path while the join drained.
    if runtime.cancelled.load(Ordering::Relaxed) {
        anyhow::bail!("cancelled");
    }

    // Flatten in page order, dropping any pages that failed (None slots).
    let images: Vec<Vec<u8>> = results.into_iter().flatten().collect();

    if images.is_empty() {
        anyhow::bail!("no images downloaded");
    }

    manager
        .set_progress(&task.id, total, total, "packaging...")
        .await?;
    let _ = manager.append_log(&task.id, "📦 打包 CB7").await;
    let _ = manager.emit_progress(&task.id).await;

    let source_url = format!(
        "https://{}/g/{}/{}/",
        if ex { "exhentai.org" } else { "e-hentai.org" },
        gid,
        token,
    );
    let source = BookSource {
        source_plugin: (if ex { "exhentai" } else { "e-hentai" }).into(),
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
    let _ = manager.append_log(&task.id, &format!("📚 注册书籍: {title}")).await;

    manager
        .set_progress(&task.id, total, total, "done")
        .await?;
    let _ = manager.append_log(&task.id, "✅ 完成").await;
    let _ = manager.emit_progress(&task.id).await;
    Ok(Some(book_id))
}

// ====================== AHentai processing ======================

async fn process_ahentai(
    manager: Arc<TaskManager>,
    task: &crate::services::task::Task,
    runtime: Arc<TaskRuntime>,
    _temp_dir: &std::path::Path,
    gallery_id: &str,
    fallback_title: &str,
) -> Result<Option<String>> {
    let client = AhentaiClient::new().context("build ahentai client")?;
    let library = LibraryService::new(manager.db.clone(), manager.storage.clone());

    manager
        .set_progress(&task.id, 0, 0, "fetching metadata...")
        .await?;
    let _ = manager.append_log(&task.id, "🔍 抓取画廊元数据").await;
    let _ = manager.emit_progress(&task.id).await;

    let meta = client
        .fetch_gallery_meta(gallery_id)
        .await
        .context("fetch gallery meta")?;

    // Already-in-library guard: if this gallery is registered, adopt the
    // existing book instead of re-downloading + inserting a duplicate. This
    // fires on manual retry after a task died at the very last step (book
    // committed, terminal status lost) or after a reset wiped the task row.
    let source_url = format!("{}/g/{}/", crate::services::ahentai::AHENTAI_BASE, gallery_id);
    if let Some(prev) = find_book_by_source_url(&manager.db.pool, &source_url).await? {
        let total = meta.page_count.max(prev.page_count) as i64;
        let _ = manager
            .append_log(&task.id, "📚 已在书库中，跳过下载")
            .await;
        manager
            .set_progress(&task.id, total, total, "done")
            .await?;
        let _ = manager.emit_progress(&task.id).await;
        return Ok(Some(prev.book_id));
    }

    let total = meta.page_count as i64;
    if total == 0 {
        anyhow::bail!("gallery {gallery_id} has 0 pages (missing or deleted?)");
    }

    // Prefer the scraped title; fall back to the task title.
    let title = if meta.title.is_empty() {
        fallback_title.to_string()
    } else {
        meta.title.clone()
    };

    manager
        .set_progress(&task.id, 0, total, "downloading...")
        .await?;
    let _ = manager
        .append_log(&task.id, &format!("⬇ 开始下载 {total} 页"))
        .await;
    let _ = manager.emit_progress(&task.id).await;

    let load_dir = meta.load_dir.clone();
    let gid = gallery_id.to_string();

    // ASMHentai removed the `load_dir` hidden input; the CDN path is now
    // images.asmhentai.com/{last3digits}/{gid}/{page}.jpg.
    // Fall back to the last 3 digits when load_dir is empty.
    let dir = if load_dir.is_empty() {
        let last3 = if gid.len() >= 3 { &gid[gid.len() - 3..] } else { &gid };
        last3.to_string()
    } else {
        load_dir.trim_matches('/').to_string()
    };

    // Download all pages via aria2 through the shared concurrent helper.
    let downloads: Vec<PageDownload> = (1..=total)
        .map(|page| PageDownload {
            index: page as usize - 1,
            url: format!(
                "https://images.asmhentai.com/{}/{}/{}.jpg",
                dir, gid, page
            ),
            out: format!("page-{:04}", page - 1),
            referer: "https://asmhentai.com/",
            origin: None,
            min_bytes: 200,
        })
        .collect();

    let mut results = download_pages_concurrent(
        Arc::clone(&manager),
        Arc::clone(&runtime),
        &task.id,
        _temp_dir.to_path_buf(),
        downloads,
        true,
        PageErrorPolicy::SkipPage,
    )
    .await?;

    if runtime.cancelled.load(Ordering::Relaxed) {
        anyhow::bail!("cancelled");
    }

    let images: Vec<Vec<u8>> = results.iter_mut().map(|o| o.take()).flatten().collect();
    if images.is_empty() {
        anyhow::bail!("no images downloaded from gallery {gallery_id}");
    }

    manager
        .set_progress(&task.id, total, total, "packaging...")
        .await?;
    let _ = manager.append_log(&task.id, "📦 打包 CB7").await;
    let _ = manager.emit_progress(&task.id).await;

    // Merge all tag-like metadata for the book.
    let mut all_tags: Vec<String> = Vec::new();
    all_tags.extend(meta.tags.clone());
    all_tags.extend(meta.artists.clone());
    all_tags.extend(meta.groups.clone());
    all_tags.extend(meta.languages.clone());
    if !meta.category.is_empty() {
        all_tags.push(meta.category.clone());
    }
    all_tags.extend(meta.parodies.clone());

    // Uploader from the gallery page's artist list — join with ", ".
    let author = if meta.artists.is_empty() {
        None
    } else {
        Some(meta.artists.join(", "))
    };

    // (see the early guard above; keep a single canonical URL for both)
    let source = BookSource {
        source_plugin: "asmhentai".into(),
        source_url: source_url.clone(),
        source_post_id: Some(gid.to_string()),
        scraped_at: Some(Utc::now()),
        author: author.clone(),
        author_id: None,
        published_at: None,
    };

    let file_path = manager
        .storage
        .create_cb7(
            &images,
            &BookMetadata {
                title: title.clone(),
                tags: all_tags.clone(),
                author,
                source_plugin: Some("asmhentai".into()),
                source_url: Some(source_url),
                source_post_id: Some(gid.to_string()),
                scraped_at: source.scraped_at.map(|t| t.to_rfc3339()),
                ..Default::default()
            },
        )
        .context("create cb7")?;
    let _ = manager.append_log(&task.id, "📦 打包完成").await;

    let book_id = Uuid::new_v4().to_string();
    library
        .register_stored_book(
            &book_id,
            &title,
            &file_path,
            images.len() as i32,
            Some(&source),
            &all_tags,
            None,
        )
        .await
        .context("register book")?;

    let _ = manager.set_book_id(&task.id, &book_id).await;
    let _ = manager
        .append_log(&task.id, &format!("📚 注册书籍: {title}"))
        .await;

    manager
        .set_progress(&task.id, total, total, "done")
        .await?;
    let _ = manager.append_log(&task.id, "✅ 完成").await;
    let _ = manager.emit_progress(&task.id).await;
    Ok(Some(book_id))
}

// ====================== NiceCat processing ======================

/// Parse a raw `getComicOrder` API response into `(image_urls, title)`.
///
/// The input is the **full response envelope** (including the `{"code":...,
/// "data":...}` wrapper), e.g. `{"code":"4000200","data":{"imageData":[...],
/// "comicData":{...}}}`.  Image URLs are read from `data.imageData[*].imageUrl`;
/// the title is taken from `data.comicData.name` (or `.title`).
///
/// Falls back to a recursive search via `find_page_image_urls` if `imageData`
/// is absent/empty.
fn parse_nicecat_order_response(
    order_json_str: &str,
    comic_id: &str,
) -> Result<(Vec<String>, Option<String>)> {
    let order: serde_json::Value =
        serde_json::from_str(order_json_str).context("parse nicecat order response")?;
    // The response is the full envelope; imageData lives under data.
    let order_data = order.get("data").unwrap_or(&order);

    // --- Extract page image URLs ---
    let image_urls: Vec<String> = order_data
        .get("imageData")
        .and_then(|arr| arr.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|it| it.get("imageUrl").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if image_urls.is_empty() {
        if let Some(found) = find_page_image_urls(&order) {
            tracing::info!(target: "erolib::tasks::nicecat", comic_id = %comic_id, count = found.len(), "found image URLs via fallback recursive search");
            return Ok((found, None));
        }
        anyhow::bail!("no imageData found in nicecat API response for {} (top keys: {:?})", comic_id, order_data.as_object().map(|o| o.keys().collect::<Vec<_>>()));
    }

    tracing::info!(target: "erolib::tasks::nicecat", comic_id = %comic_id, page_count = image_urls.len(), "extracted image URLs from getComicOrder response");

    let comic_data = order_data.get("comicData");
    let title = comic_data.and_then(|c| c.get("name").or_else(|| c.get("title"))).and_then(|v| v.as_str()).map(String::from);
    Ok((image_urls, title))
}

/// Parse the `ComicInfo/info` API response from the NiceCat info page.
///
/// Returns (author, published_at, tags, title).
/// - author: from `tagData.artist[0].name` (画师)
/// - published_at: from `update_time` (上传时间)
/// - tags: all tag names across all `tagData` categories
/// - title: from `name_one` (or `name`)
fn parse_nicecat_info_response(json_str: &str) -> (Option<String>, Option<String>, Vec<String>, Option<String>) {
    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return (None, None, vec![], None),
    };

    let comic_data = parsed
        .get("data")
        .and_then(|d| d.get("comicData"));

    // --- Title: prefer name_one (original title) ---
    let title = comic_data
        .and_then(|c| c.get("name_one"))
        .or_else(|| comic_data.and_then(|c| c.get("name")))
        .or_else(|| comic_data.and_then(|c| c.get("title")))
        .and_then(|v| v.as_str())
        .map(String::from);

    // --- Author: from tagData.artist[0].name ---
    let author = comic_data
        .and_then(|c| c.get("tagData"))
        .and_then(|t| t.get("artist"))
        .and_then(|a| a.get(0))
        .and_then(|a| a.get("name"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // --- Published date: from update_time ---
    let published_at = comic_data
        .and_then(|c| c.get("update_time"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // --- Tags: collect all tag names from ALL tagData categories ---
    let mut tags: Vec<String> = Vec::new();
    if let Some(tag_data) = comic_data.and_then(|c| c.get("tagData")) {
        if let Some(obj) = tag_data.as_object() {
            for (_category, tag_array) in obj {
                if let Some(arr) = tag_array.as_array() {
                    for tag in arr {
                        if let Some(name) = tag.get("name").and_then(|v| v.as_str()) {
                            tags.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    // Deduplicate (some tags may appear in multiple categories).
    tags.sort();
    tags.dedup();

    (author, published_at, tags, title)
}

/// Extract the first non-empty string value for any of the given keys by
/// walking the JSON tree depth-first.
#[allow(dead_code)]
fn extract_field(val: &serde_json::Value, keys: &[&str]) -> Option<String> {
    match val {
        serde_json::Value::Object(map) => {
            for k in keys {
                if let Some(v) = map.get(*k) {
                    if let Some(s) = v.as_str() {
                        if !s.is_empty() {
                            return Some(s.to_string());
                        }
                    }
                }
            }
            for (_k, v) in map {
                if let Some(found) = extract_field(v, keys) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let Some(found) = extract_field(item, keys) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

/// Recursively search a JSON value for an array of image-like URLs.
/// (Fallback when the standard `imageData` structure is absent.)
fn find_page_image_urls(val: &serde_json::Value) -> Option<Vec<String>> {
    match val {
        serde_json::Value::Array(arr) => {
            if arr.iter().all(|v| v.is_string()) {
                let urls: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                if !urls.is_empty()
                    && urls.iter().any(|u| {
                        let lower = u.to_lowercase();
                        lower.ends_with(".jpg")
                            || lower.ends_with(".jpeg")
                            || lower.ends_with(".png")
                            || lower.ends_with(".webp")
                            || lower.ends_with(".gif")
                            || lower.contains("/img/")
                            || lower.contains("/images/")
                            || lower.contains("/comic-content/")
                    })
                {
                    return Some(urls);
                }
            }
            for item in arr {
                if let Some(found) = find_page_image_urls(item) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Object(map) => {
            for key in &["imageList", "images", "pages", "pageList", "pageUrls", "imageData"] {
                if let Some(v) = map.get(*key) {
                    if let Some(found) = find_page_image_urls(v) {
                        return Some(found);
                    }
                }
            }
            for (_k, v) in map {
                if let Some(found) = find_page_image_urls(v) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

async fn process_nicecat(
    manager: Arc<TaskManager>,
    task: &crate::services::task::Task,
    runtime: Arc<TaskRuntime>,
    comic_id: &str,
    fallback_title: &str,
) -> Result<Option<String>> {
    let library = LibraryService::new(manager.db.clone(), manager.storage.clone());

    // Already-in-library guard: adopt the existing book instead of
    // re-downloading + inserting a duplicate on manual retry (the original
    // task may have died at the very last step after the book was committed).
    let source_url = format!("https://ncmm.cc/comic/info/id.{}", comic_id);
    if let Some(prev) = find_book_by_source_url(&manager.db.pool, &source_url).await? {
        let total = prev.page_count as i64;
        let _ = manager
            .append_log(&task.id, "📚 已在书库中，跳过下载")
            .await;
        manager
            .set_progress(&task.id, total, total, "done")
            .await?;
        let _ = manager.emit_progress(&task.id).await;
        return Ok(Some(prev.book_id));
    }

    manager
        .set_progress(&task.id, 0, 0, "extracting page data...")
        .await?;

    // 1. Fetch comic metadata + page-order via pure HTTP (concurrency-safe:
    //    each call is independent, unlike the old shared-WebView localStorage).
    let _ = manager
        .append_log(&task.id, "🔍 抓取 NiceCat 元数据 + 页序")
        .await;
    let _ = manager.emit_progress(&task.id).await;

    // Run the two independent HTTP calls concurrently.
    let (info_res, order_res) = tokio::join!(
        crate::services::nicecat::fetch_comic_info_raw(comic_id),
        crate::services::nicecat::fetch_comic_order_raw(comic_id),
    );
    let info_raw = info_res.map_err(|e| anyhow::anyhow!("ComicInfo/info failed: {e}"))?;
    let order_raw = order_res.map_err(|e| anyhow::anyhow!("getComicOrder failed: {e}"))?;

    // Per-page log is emitted by download_pages_concurrent via "📥 第 i/n 页"

    // 2. Parse page image URLs from the order response.
    let (page_urls, order_title) = parse_nicecat_order_response(&order_raw, comic_id)?;

    let total = page_urls.len() as i64;
    if total == 0 {
        anyhow::bail!("no page images found for comic {comic_id}");
    }

    // 3. Extract metadata from the info response.
    let (scraped_author, scraped_published, scraped_tags, info_title) =
        parse_nicecat_info_response(&info_raw);

    // Prefer info-page title, fall back to order title, then fallback_title.
    let title = info_title
        .or(order_title)
        .unwrap_or_else(|| fallback_title.to_string());
    let author = scraped_author;
    let published_at = scraped_published;
    let all_tags: Vec<String> = if scraped_tags.is_empty() {
        vec!["nicecat".into()]
    } else {
        scraped_tags
    };

    manager
        .set_progress(&task.id, 0, total, "downloading...")
        .await?;
    let _ = manager
        .append_log(&task.id, &format!("⬇ 开始下载 {} 页", total))
        .await;
    let _ = manager.emit_progress(&task.id).await;

    // Download all pages via aria2 through the shared concurrent helper.
    let downloads: Vec<PageDownload> = page_urls
        .iter()
        .enumerate()
        .map(|(idx, url)| PageDownload {
            index: idx,
            url: url.clone(),
            out: format!("page-{:04}", idx),
            referer: "https://ncmm.cc/",
            origin: Some("https://ncmm.cc"),
            min_bytes: 200,
        })
        .collect();

    let temp_dir = manager
        .storage
        .library_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("library path has no parent"))?
        .join("downloads")
        .join(&task.id);
    let mut results = download_pages_concurrent(
        Arc::clone(&manager),
        Arc::clone(&runtime),
        &task.id,
        temp_dir,
        downloads,
        true,
        PageErrorPolicy::SkipPage,
    )
    .await?;

    if runtime.cancelled.load(Ordering::Relaxed) {
        anyhow::bail!("cancelled");
    }

    let images: Vec<Vec<u8>> = results.iter_mut().map(|o| o.take()).flatten().collect();
    if images.is_empty() {
        anyhow::bail!("no images downloaded for comic {comic_id}");
    }

    // 4. Package as CB7 and register.
    manager
        .set_progress(&task.id, total, total, "packaging...")
        .await?;
    let _ = manager.append_log(&task.id, "📦 打包 CB7").await;
    let _ = manager.emit_progress(&task.id).await;

    // (source_url already computed by the early already-in-library guard)
    let source = BookSource {
        source_plugin: "nicecat".into(),
        source_url: source_url.clone(),
        source_post_id: Some(comic_id.to_string()),
        scraped_at: Some(Utc::now()),
        author: author.clone(),
        author_id: None,
        published_at: published_at.clone(),
    };

    let file_path = manager
        .storage
        .create_cb7(
            &images,
            &BookMetadata {
                title: title.clone(),
                tags: all_tags.clone(),
                author,
                source_plugin: Some("nicecat".into()),
                source_url: Some(source_url),
                source_post_id: Some(comic_id.to_string()),
                scraped_at: source.scraped_at.map(|t| t.to_rfc3339()),
                ..Default::default()
            },
        )
        .context("create cb7")?;
    let _ = manager.append_log(&task.id, "📦 打包完成").await;

    let book_id = Uuid::new_v4().to_string();
    library
        .register_stored_book(
            &book_id,
            &title,
            &file_path,
            images.len() as i32,
            Some(&source),
            &all_tags,
            None,
        )
        .await
        .context("register book")?;

    let _ = manager.set_book_id(&task.id, &book_id).await;
    let _ = manager
        .append_log(&task.id, &format!("📚 注册书籍: {}", title))
        .await;

    manager
        .set_progress(&task.id, total, total, "done")
        .await?;
    let _ = manager.append_log(&task.id, "✅ 完成").await;
    let _ = manager.emit_progress(&task.id).await;
    Ok(Some(book_id))
}
