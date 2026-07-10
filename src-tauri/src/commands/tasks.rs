use std::sync::Arc;

use tauri::State;

use crate::services::task::{TaskPayload, TaskSnapshot};
use crate::services::task_manager::TaskManager;

#[tauri::command]
pub async fn tasks_list(
    manager: State<'_, Arc<TaskManager>>,
) -> Result<Vec<TaskSnapshot>, String> {
    manager.list_tasks().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_pause(
    task_id: String,
    manager: State<'_, Arc<TaskManager>>,
) -> Result<(), String> {
    manager.pause_task(&task_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_resume(
    task_id: String,
    manager: State<'_, Arc<TaskManager>>,
) -> Result<(), String> {
    manager
        .resume_task(&task_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_cancel(
    task_id: String,
    manager: State<'_, Arc<TaskManager>>,
) -> Result<(), String> {
    manager
        .cancel_task(&task_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_delete(
    task_id: String,
    manager: State<'_, Arc<TaskManager>>,
) -> Result<(), String> {
    manager
        .delete_task(&task_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_retry(
    task_id: String,
    manager: State<'_, Arc<TaskManager>>,
) -> Result<(), String> {
    manager
        .retry_task(&task_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_enqueue_pixiv_bookmarks(
    cookie: String,
    user_id: String,
    limit: u64,
    manager: State<'_, Arc<TaskManager>>,
) -> Result<String, String> {
    let payload = TaskPayload::PixivBookmarks {
        cookie,
        user_id: user_id.clone(),
        limit,
    };
    let title = format!("Pixiv bookmarks (user {user_id})");
    manager
        .enqueue(payload, title)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_enqueue_pixiv_user_works(
    cookie: String,
    target_user_id: String,
    limit: u64,
    manager: State<'_, Arc<TaskManager>>,
) -> Result<String, String> {
    let payload = TaskPayload::PixivUserWorks {
        cookie,
        target_user_id: target_user_id.clone(),
        limit,
    };
    let title = format!("Pixiv user works (user {target_user_id})");
    manager
        .enqueue(payload, title)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_enqueue_pixiv_work(
    cookie: String,
    work_id: String,
    title: String,
    manager: State<'_, Arc<TaskManager>>,
) -> Result<String, String> {
    let payload = TaskPayload::PixivSingleWork { cookie, work_id };
    manager
        .enqueue(payload, format!("Pixiv: {title}"))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_enqueue_ehentai_gallery(
    cookie: String,
    gallery_url: String,
    manager: State<'_, Arc<TaskManager>>,
) -> Result<String, String> {
    let (gid, token) =
        crate::services::EhentaiClient::parse_gallery_url(&gallery_url).map_err(|e| e.to_string())?;
    let payload = TaskPayload::EhentaiGallery {
        cookie,
        gallery_url: gallery_url.clone(),
        gid,
        token,
    };
    manager
        .enqueue(payload, format!("EHentai gallery: {gallery_url}"))
        .await
        .map_err(|e| e.to_string())
}
