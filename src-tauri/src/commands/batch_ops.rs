use std::sync::MutexGuard;

use rusqlite::Connection;
use tauri::State;

use crate::db::models::{DuplicateGroup, OperationHistory};
use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::{batch_service, duplicate_service};
use crate::AppState;

fn lock_db<'a>(state: &'a AppState) -> MutexGuard<'a, Connection> {
    state.conn.lock().expect("db lock poisoned")
}

// ── 重复文件检测 ──

#[tauri::command]
pub fn find_duplicates(state: State<AppState>) -> CommandResult<Vec<DuplicateGroup>> {
    let mut conn = lock_db(&state);
    // 先补齐缺失的哈希，保证能检出重复
    let _ensured =
        duplicate_service::ensure_all_hashed(&mut conn).map_err(|e| AppError::from(e))?;
    duplicate_service::find_duplicates(&conn).map_err(Into::into)
}

#[tauri::command]
pub fn get_hash_stats(state: State<AppState>) -> CommandResult<(i64, i64)> {
    let conn = lock_db(&state);
    duplicate_service::hash_stats(&conn).map_err(Into::into)
}

// ── 批量重命名 ──

/// 批量重命名预览（dry-run）。
/// items: Vec<(resource_id, new_name)>
#[tauri::command]
pub fn preview_batch_rename(
    state: State<AppState>,
    items: Vec<(String, String)>,
) -> CommandResult<Vec<batch_service::RenameItem>> {
    let conn = lock_db(&state);
    batch_service::preview_batch_rename(&conn, &items).map_err(Into::into)
}

/// 执行批量重命名（仅处理 status=="ok" 的条目）。
#[tauri::command]
pub fn execute_batch_rename(
    state: State<AppState>,
    items: Vec<batch_service::RenameItem>,
    description: Option<String>,
) -> CommandResult<OperationHistory> {
    let mut conn = lock_db(&state);
    batch_service::execute_batch_rename(&mut conn, &items, description.as_deref())
        .map_err(Into::into)
}

// ── 操作历史 / 撤销 ──

#[tauri::command]
pub fn list_operation_history(
    state: State<AppState>,
    limit: Option<i64>,
) -> CommandResult<Vec<OperationHistory>> {
    let conn = lock_db(&state);
    batch_service::list_operation_history(&conn, limit.unwrap_or(20)).map_err(Into::into)
}

#[tauri::command]
pub fn undo_operation(state: State<AppState>, op_id: String) -> CommandResult<()> {
    let mut conn = lock_db(&state);
    batch_service::undo_operation(&mut conn, &op_id).map_err(Into::into)
}
