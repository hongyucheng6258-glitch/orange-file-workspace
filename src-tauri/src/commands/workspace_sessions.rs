use crate::error::AppError;
use crate::services::workspace_session_service::{self, WorkspaceSession, WorkspaceSessionUpdate};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn save_workspace_session(
    state: State<'_, AppState>,
    project_id: String,
    open_files_json: Option<String>,
    active_file_id: Option<String>,
    terminal_tabs_json: Option<String>,
    active_terminal_index: Option<i64>,
    running_tasks_json: Option<String>,
    panel_layout_json: Option<String>,
    scroll_positions_json: Option<String>,
) -> Result<WorkspaceSession, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");

    let update = WorkspaceSessionUpdate {
        open_files_json,
        active_file_id,
        terminal_tabs_json,
        active_terminal_index,
        running_tasks_json,
        panel_layout_json,
        scroll_positions_json,
    };

    workspace_session_service::save_session(&conn, &project_id, &update)
}

#[tauri::command]
pub async fn get_workspace_session(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<WorkspaceSession, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    workspace_session_service::get_session(&conn, &project_id)
}

#[tauri::command]
pub async fn mark_session_restored(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    workspace_session_service::mark_restored(&conn, &project_id)
}

#[tauri::command]
pub async fn delete_workspace_session(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    workspace_session_service::delete_session(&conn, &project_id)
}

#[tauri::command]
pub async fn list_workspace_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<WorkspaceSession>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    workspace_session_service::list_sessions(&conn)
}
