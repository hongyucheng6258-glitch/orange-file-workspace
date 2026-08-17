use crate::error::AppError;
use crate::services::project_task_service;
use crate::AppState;
use tauri::State;

use crate::db::models::{ProjectTask, ProjectTaskLink};

#[tauri::command]
pub async fn create_project_task(
    state: State<'_, AppState>,
    project_id: String,
    title: String,
    description: Option<String>,
    priority: Option<String>,
) -> Result<ProjectTask, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    project_task_service::create_task(
        &conn,
        &project_id,
        &title,
        description.as_deref(),
        priority.as_deref().unwrap_or("medium"),
    )
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn list_project_tasks(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<ProjectTask>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    project_task_service::list_tasks(&conn, &project_id).map_err(AppError::from)
}

#[tauri::command]
pub async fn update_project_task(
    state: State<'_, AppState>,
    id: String,
    title: Option<String>,
    description: Option<Option<String>>,
    status: Option<String>,
    priority: Option<String>,
    due_date: Option<Option<i64>>,
) -> Result<Option<ProjectTask>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    project_task_service::update_task(
        &conn,
        &id,
        title.as_deref(),
        description.as_ref().map(|d| d.as_deref()),
        status.as_deref(),
        priority.as_deref(),
        due_date,
    )
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn delete_project_task(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    project_task_service::delete_task(&conn, &id).map_err(AppError::from)
}

#[tauri::command]
pub async fn reorder_project_tasks(
    state: State<'_, AppState>,
    task_ids: Vec<String>,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    project_task_service::reorder_tasks(&conn, &task_ids).map_err(AppError::from)
}

#[tauri::command]
pub async fn link_task_resource(
    state: State<'_, AppState>,
    task_id: String,
    resource_id: String,
    link_type: Option<String>,
) -> Result<ProjectTaskLink, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    project_task_service::link_resource(
        &conn,
        &task_id,
        &resource_id,
        link_type.as_deref().unwrap_or("reference"),
    )
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn unlink_task_resource(
    state: State<'_, AppState>,
    task_id: String,
    resource_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    project_task_service::unlink_resource(&conn, &task_id, &resource_id).map_err(AppError::from)
}

#[tauri::command]
pub async fn list_task_links(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<ProjectTaskLink>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    project_task_service::list_task_links(&conn, &task_id).map_err(AppError::from)
}

#[tauri::command]
pub async fn list_links_by_resource(
    state: State<'_, AppState>,
    resource_id: String,
) -> Result<Vec<ProjectTaskLink>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    project_task_service::list_links_by_resource(&conn, &resource_id).map_err(AppError::from)
}
