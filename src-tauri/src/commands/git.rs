use crate::error::AppError;
use crate::services::git_service::{self, GitFileStatus, GitRepository};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn register_git_repository(
    state: State<'_, AppState>,
    project_id: String,
    repo_path: String,
    current_branch: Option<String>,
    remote_url: Option<String>,
) -> Result<GitRepository, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    git_service::register_repository(&conn, &project_id, &repo_path)?;
    if current_branch.is_some() || remote_url.is_some() {
        git_service::update_repository_status(
            &conn,
            &project_id,
            current_branch.as_deref(),
            remote_url.as_deref(),
            false,
            0,
            0,
        )?;
    }
    git_service::get_repository(&conn, &project_id)
}

#[tauri::command]
pub async fn get_git_repository(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Option<GitRepository>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    Ok(git_service::get_repository(&conn, &project_id).ok())
}

#[tauri::command]
pub async fn update_git_repository_status(
    state: State<'_, AppState>,
    repo_id: String,
    current_branch: Option<String>,
    remote_url: Option<String>,
    has_uncommitted: bool,
    ahead_count: i64,
    behind_count: i64,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    git_service::update_repository_status(
        &conn,
        &repo_id,
        current_branch.as_deref(),
        remote_url.as_deref(),
        has_uncommitted,
        ahead_count,
        behind_count,
    )
}

#[tauri::command]
pub async fn mark_git_repository_fetched(
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    git_service::mark_fetched(&conn, &repo_id)
}

#[tauri::command]
pub async fn save_git_file_status(
    state: State<'_, AppState>,
    repo_id: String,
    file_path: String,
    status: String,
    staged: bool,
) -> Result<GitFileStatus, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    git_service::save_file_status(&conn, &repo_id, &file_path, &status, staged)
}

#[tauri::command]
pub async fn get_git_file_status(
    state: State<'_, AppState>,
    repo_id: String,
    file_path: String,
) -> Result<Option<GitFileStatus>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    Ok(git_service::get_file_status(&conn, &repo_id, &file_path).ok())
}

#[tauri::command]
pub async fn list_git_file_statuses(
    state: State<'_, AppState>,
    repo_id: String,
    status_filter: Option<String>,
    staged_filter: Option<bool>,
) -> Result<Vec<GitFileStatus>, AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    let statuses = git_service::list_file_statuses(&conn, &repo_id)?;
    let filtered: Vec<GitFileStatus> = statuses
        .into_iter()
        .filter(|s| status_filter.as_ref().map_or(true, |f| s.status == *f))
        .filter(|s| staged_filter.map_or(true, |f| s.staged == f))
        .collect();
    Ok(filtered)
}

#[tauri::command]
pub async fn clear_git_file_statuses(
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    git_service::clear_file_statuses(&conn, &repo_id).map(|_| ())
}

#[tauri::command]
pub async fn delete_git_repository(
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock poisoned");
    git_service::delete_repository(&conn, &repo_id)
}
