use std::sync::MutexGuard;

use rusqlite::Connection;
use tauri::State;

use crate::commands::search::SearchHit;
use crate::ipc::CommandResult;
use crate::services::saved_search_service;
use crate::AppState;

fn lock_db<'a>(state: &'a AppState) -> MutexGuard<'a, Connection> {
    state.conn.lock().expect("db lock poisoned")
}

#[tauri::command]
pub fn create_saved_search(
    state: State<AppState>,
    name: String,
    query: Option<String>,
    filters_json: Option<String>,
    color: Option<String>,
    icon: Option<String>,
) -> CommandResult<crate::db::models::SavedSearch> {
    let conn = lock_db(&state);
    saved_search_service::create_saved_search(
        &conn,
        &name,
        query.as_deref(),
        &filters_json.unwrap_or_default(),
        color.as_deref(),
        icon.as_deref(),
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn list_saved_searches(
    state: State<AppState>,
) -> CommandResult<Vec<crate::db::models::SavedSearch>> {
    let conn = lock_db(&state);
    saved_search_service::list_saved_searches(&conn).map_err(Into::into)
}

#[tauri::command]
pub fn list_pinned_saved_searches(
    state: State<AppState>,
) -> CommandResult<Vec<crate::db::models::SavedSearch>> {
    let conn = lock_db(&state);
    saved_search_service::list_pinned(&conn).map_err(Into::into)
}

#[tauri::command]
pub fn update_saved_search(
    state: State<AppState>,
    id: String,
    name: Option<String>,
    query: Option<Option<String>>,
    filters_json: Option<String>,
    color: Option<Option<String>>,
    icon: Option<Option<String>>,
) -> CommandResult<Option<crate::db::models::SavedSearch>> {
    let conn = lock_db(&state);
    saved_search_service::update_saved_search(
        &conn,
        &id,
        name.as_deref(),
        query.as_ref().map(|o| o.as_deref()),
        filters_json.as_deref(),
        color.as_ref().map(|o| o.as_deref()),
        icon.as_ref().map(|o| o.as_deref()),
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn delete_saved_search(state: State<AppState>, id: String) -> CommandResult<()> {
    let conn = lock_db(&state);
    saved_search_service::delete_saved_search(&conn, &id).map_err(Into::into)
}

#[tauri::command]
pub fn toggle_saved_search_pinned(
    state: State<AppState>,
    id: String,
    pinned: bool,
) -> CommandResult<Option<crate::db::models::SavedSearch>> {
    let conn = lock_db(&state);
    saved_search_service::toggle_pinned(&conn, &id, pinned).map_err(Into::into)
}

#[tauri::command]
pub fn reorder_pinned_saved_searches(
    state: State<AppState>,
    ids: Vec<String>,
) -> CommandResult<()> {
    let conn = lock_db(&state);
    saved_search_service::reorder_pinned(&conn, &ids).map_err(Into::into)
}

#[tauri::command]
pub fn execute_saved_search(
    state: State<AppState>,
    id: String,
    limit: Option<i64>,
) -> CommandResult<Vec<SearchHit>> {
    let conn = lock_db(&state);
    saved_search_service::execute_saved_search(&conn, &id, limit.unwrap_or(200))
        .map_err(Into::into)
}
