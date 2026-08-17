use crate::error::AppError;
use crate::services::tag_service::{self, Tag};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn create_tag(
    state: State<'_, AppState>,
    name: String,
    color: Option<String>,
) -> Result<Tag, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    tag_service::create_tag(&conn, &name, color.as_deref())
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<Tag>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    tag_service::list_tags(&conn)
}

#[tauri::command]
pub async fn get_tag(
    state: State<'_, AppState>,
    tag_id: String,
) -> Result<Option<Tag>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    tag_service::get_tag(&conn, &tag_id)
}

#[tauri::command]
pub async fn update_tag(
    state: State<'_, AppState>,
    tag_id: String,
    name: Option<String>,
    color: Option<String>,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    let color_param: Option<Option<&str>> = color.as_deref().map(Some);
    tag_service::update_tag(&conn, &tag_id, name.as_deref(), color_param)
}

#[tauri::command]
pub async fn delete_tag(state: State<'_, AppState>, tag_id: String) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    tag_service::delete_tag(&conn, &tag_id)
}

#[tauri::command]
pub async fn add_tag_to_resource(
    state: State<'_, AppState>,
    tag_id: String,
    resource_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    tag_service::add_tag_to_resource(&conn, &resource_id, &tag_id)
}

#[tauri::command]
pub async fn remove_tag_from_resource(
    state: State<'_, AppState>,
    tag_id: String,
    resource_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    tag_service::remove_tag_from_resource(&conn, &resource_id, &tag_id)
}

#[tauri::command]
pub async fn get_resource_tags(
    state: State<'_, AppState>,
    resource_id: String,
) -> Result<Vec<Tag>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    tag_service::get_resource_tags(&conn, &resource_id)
}

#[tauri::command]
pub async fn get_resources_by_tag(
    state: State<'_, AppState>,
    tag_id: String,
) -> Result<Vec<String>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    tag_service::get_resources_by_tag(&conn, &tag_id)
}
