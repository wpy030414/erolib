use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Placeholder default used only for `#[serde(skip, default)]` on Task.payload
/// so that `serde::Deserialize` for Task compiles. The field is never
/// deserialized from JSON in practice.
#[allow(dead_code)] // referenced via #[serde(default = "..")] on Task.payload
pub fn default_task_payload() -> TaskPayload {
    TaskPayload::PixivBookmarks {
        cookie: String::new(),
        user_id: String::new(),
        limit: 0,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    Pixiv,
    Ehentai,
}

impl fmt::Display for TaskSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskSource::Pixiv => write!(f, "pixiv"),
            TaskSource::Ehentai => write!(f, "ehentai"),
        }
    }
}

impl FromStr for TaskSource {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pixiv" => Ok(TaskSource::Pixiv),
            "ehentai" => Ok(TaskSource::Ehentai),
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
    PixivBookmarks {
        cookie: String,
        user_id: String,
        limit: u64,
    },
    PixivUserWorks {
        cookie: String,
        target_user_id: String,
        limit: u64,
    },
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
}

impl TaskPayload {
    pub fn source(&self) -> TaskSource {
        match self {
            TaskPayload::PixivBookmarks { .. }
            | TaskPayload::PixivUserWorks { .. }
            | TaskPayload::PixivSingleWork { .. } => TaskSource::Pixiv,
            TaskPayload::EhentaiGallery { .. } => TaskSource::Ehentai,
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
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The deserialized payload — never stored as a column.
    #[serde(skip)]
    #[serde(default = "default_task_payload")]
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
            created_at: t.created_at.to_rfc3339(),
            updated_at: t.updated_at.to_rfc3339(),
            completed_at: t.completed_at.map(|d| d.to_rfc3339()),
        }
    }
}
