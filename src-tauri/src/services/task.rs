use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    Pixiv,
    Ehentai,
    Ahentai,
    Nicecat,
}

impl fmt::Display for TaskSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskSource::Pixiv => write!(f, "pixiv"),
            TaskSource::Ehentai => write!(f, "ehentai"),
            TaskSource::Ahentai => write!(f, "ahentai"),
            TaskSource::Nicecat => write!(f, "nicecat"),
        }
    }
}

impl FromStr for TaskSource {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pixiv" => Ok(TaskSource::Pixiv),
            "ehentai" => Ok(TaskSource::Ehentai),
            "ahentai" => Ok(TaskSource::Ahentai),
            "nicecat" => Ok(TaskSource::Nicecat),
            _ => Err(format!("unknown task source: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Outcome of a completed-task re-download request, returned to the frontend
/// so it can pick the right toast. `restarted`/`redownloaded` both kick off a
/// download (the book is wholly re-fetched); `already_complete` is a no-op.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RedownloadAction {
    /// No matching book was in the library — a plain retry (full download).
    Restarted,
    /// A book existed but was incomplete — it was removed and fully re-fetched.
    Redownloaded,
    /// The local archive already matched the remote page count — nothing done.
    AlreadyComplete,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Paused => write!(f, "paused"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for TaskStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(TaskStatus::Pending),
            "running" => Ok(TaskStatus::Running),
            "paused" => Ok(TaskStatus::Paused),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            "cancelled" => Ok(TaskStatus::Cancelled),
            _ => Err(format!("unknown task status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TaskPayload {
    EhentaiGallery {
        cookie: String,
        gallery_url: String,
        gid: String,
        token: String,
    },
    PixivSingleWork {
        cookie: String,
        work_id: String,
    },
    AhentaiGallery {
        gallery_id: String,
        title: String,
    },
    NicecatGallery {
        comic_id: String,
        title: String,
    },
}

impl TaskPayload {
    pub fn source(&self) -> TaskSource {
        match self {
            TaskPayload::PixivSingleWork { .. } => TaskSource::Pixiv,
            TaskPayload::EhentaiGallery { .. } => TaskSource::Ehentai,
            TaskPayload::AhentaiGallery { .. } => TaskSource::Ahentai,
            TaskPayload::NicecatGallery { .. } => TaskSource::Nicecat,
        }
    }

    /// Canonical `books.source_url` this payload registers under — must stay
    /// byte-identical to what each `process_*` stamps (task_manager.rs:
    /// EHentai ~:1837, Pixiv ~:1250, AHentai ~:1933, NiceCat ~:2302), since
    /// the re-download scan looks books up by exact string match.
    pub fn source_url(&self) -> String {
        match self {
            TaskPayload::EhentaiGallery { gallery_url, gid, token, .. } => {
                let host = if gallery_url.contains("exhentai") {
                    "exhentai.org"
                } else {
                    "e-hentai.org"
                };
                format!("https://{host}/g/{gid}/{token}/")
            }
            TaskPayload::PixivSingleWork { work_id, .. } => {
                format!("https://www.pixiv.net/artworks/{work_id}")
            }
            TaskPayload::AhentaiGallery { gallery_id, .. } => {
                format!("{}/g/{}/", crate::services::ahentai::AHENTAI_BASE, gallery_id)
            }
            TaskPayload::NicecatGallery { comic_id, .. } => {
                format!("https://ncmm.cc/comic/info/id.{comic_id}")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: String,
    pub source: TaskSource,
    pub status: TaskStatus,
    pub title: String,
    pub detail: String,
    pub progress_current: i64,
    pub progress_total: i64,
    pub retry_count: i32,
    pub max_retries: i32,
    pub speed: i64,
    pub logs: Vec<String>,
    pub book_id: Option<String>,
    pub total_bytes: i64,
    pub elapsed_ms: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The deserialized payload — never stored as a column.
    #[serde(skip)]
    pub payload: TaskPayload,
}

/// Snapshot sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub id: String,
    pub source: String,
    pub status: String,
    pub title: String,
    pub detail: String,
    pub progress_current: i64,
    pub progress_total: i64,
    pub retry_count: i32,
    pub max_retries: i32,
    pub speed: i64,
    pub logs: Vec<String>,
    pub book_id: Option<String>,
    pub total_bytes: i64,
    pub elapsed_ms: i64,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

impl From<Task> for TaskSnapshot {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            source: t.source.to_string(),
            status: t.status.to_string(),
            title: t.title,
            detail: t.detail,
            progress_current: t.progress_current,
            progress_total: t.progress_total,
            retry_count: t.retry_count,
            max_retries: t.max_retries,
            speed: t.speed,
            logs: t.logs,
            book_id: t.book_id,
            total_bytes: t.total_bytes,
            elapsed_ms: t.elapsed_ms,
            created_at: t.created_at.to_rfc3339(),
            updated_at: t.updated_at.to_rfc3339(),
            completed_at: t.completed_at.map(|d| d.to_rfc3339()),
        }
    }
}
