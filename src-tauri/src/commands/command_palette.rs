use crate::error::AppError;
use crate::services::command_palette_service::{self, CommandHistory, CommandStatistics};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn record_command_execution(
    state: State<'_, AppState>,
    command_id: String,
    command_label: String,
    command_category: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    command_palette_service::record_command_execution(
        &conn,
        &command_id,
        &command_label,
        &command_category,
    )
}

#[tauri::command]
pub async fn get_frequent_commands(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<Vec<CommandHistory>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    command_palette_service::get_frequent_commands(&conn, limit)
}

#[tauri::command]
pub async fn get_recent_commands(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<Vec<CommandHistory>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    command_palette_service::get_recent_commands(&conn, limit)
}

#[tauri::command]
pub async fn get_commands_by_category(
    state: State<'_, AppState>,
    category: String,
    limit: usize,
) -> Result<Vec<CommandHistory>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    command_palette_service::get_commands_by_category(&conn, &category, limit)
}

#[tauri::command]
pub async fn search_commands(
    state: State<'_, AppState>,
    query: String,
    limit: usize,
) -> Result<Vec<CommandHistory>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    command_palette_service::search_commands(&conn, &query, limit)
}

#[tauri::command]
pub async fn clear_command_history(state: State<'_, AppState>) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    command_palette_service::clear_command_history(&conn).map(|_| ())
}

#[tauri::command]
pub async fn delete_command_history(
    state: State<'_, AppState>,
    command_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    command_palette_service::delete_command_history(&conn, &command_id)
}

#[tauri::command]
pub async fn get_command_statistics(
    state: State<'_, AppState>,
) -> Result<CommandStatistics, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    command_palette_service::get_command_statistics(&conn)
}
