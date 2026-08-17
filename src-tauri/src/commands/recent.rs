use crate::error::AppError;
use crate::services::recent_service::{self, RecentItem};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn record_resource_access(
    state: State<'_, AppState>,
    resource_id: String,
    resource_type: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    recent_service::record_access(&conn, &resource_id, &resource_type)
}

#[tauri::command]
pub async fn get_recent_items(
    state: State<'_, AppState>,
    resource_type_filter: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<RecentItem>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    recent_service::get_recent_items(
        &conn,
        resource_type_filter.as_deref(),
        limit.unwrap_or(20),
    )
}

#[tauri::command]
pub async fn remove_recent_item(
    state: State<'_, AppState>,
    resource_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    recent_service::remove_recent_item(&conn, &resource_id)
}

#[tauri::command]
pub async fn clear_recent_items(state: State<'_, AppState>) -> Result<usize, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    recent_service::clear_recent_items(&conn)
}

#[tauri::command]
pub async fn cleanup_old_recent_items(
    state: State<'_, AppState>,
    older_than_days: i64,
) -> Result<usize, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    recent_service::cleanup_old_items(&conn, older_than_days)
}
