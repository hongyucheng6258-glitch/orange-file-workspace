use std::sync::MutexGuard;

use tauri::State;

use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::editor_service;
use crate::services::file_service;
use crate::AppState;

fn lock_db<'a>(state: &'a AppState) -> MutexGuard<'a, rusqlite::Connection> {
    state.conn.lock().expect("db lock poisoned")
}

/// 打开代码文件：读取内容并建立编辑会话。
#[tauri::command]
pub fn open_file(state: State<AppState>, resource_id: String) -> CommandResult<serde_json::Value> {
    let conn = lock_db(&state);
    let resource = crate::db::repositories::get_resource(&conn, &resource_id)?
        .ok_or_else(|| AppError::new("not_found", format!("资源 {resource_id} 不存在")))?;
    let locations = crate::db::repositories::list_locations(&conn, &resource_id)?;
    let Some(loc) = locations.first() else {
        return Err(AppError::new("location_missing", "资源缺少物理位置"));
    };
    let path = std::path::PathBuf::from(&loc.path);
    if !path.is_file() {
        return Err(AppError::new("not_file", "该资源不是文件"));
    }

    let (content, session) = editor_service::open_session(&conn, &resource_id, &path)?;
    // 打开后读取未保存草稿（若有），供前端提示恢复
    let draft = editor_service::get_draft(&conn, &resource_id)?;
    Ok(serde_json::json!({
        "resource": resource,
        "content": content,
        "session": session,
        "path": loc.path,
        "draft": draft,
    }))
}

/// 保存编辑草稿（前端编辑时防抖调用，用于崩溃恢复）。
#[tauri::command]
pub fn save_draft(
    state: State<AppState>,
    resource_id: String,
    content: String,
) -> CommandResult<()> {
    let conn = lock_db(&state);
    let locations = crate::db::repositories::list_locations(&conn, &resource_id)?;
    let Some(loc) = locations.first() else {
        return Err(AppError::new("location_missing", "资源缺少物理位置"));
    };
    let path = std::path::PathBuf::from(&loc.path);
    editor_service::save_draft(&conn, &resource_id, &path, &content)?;
    Ok(())
}

/// 读取磁盘上文件的当前完整内容（与编辑器同上限），用于冲突对比。
#[tauri::command]
pub fn read_disk_content(state: State<AppState>, resource_id: String) -> CommandResult<String> {
    let conn = lock_db(&state);
    let locations = crate::db::repositories::list_locations(&conn, &resource_id)?;
    let Some(loc) = locations.first() else {
        return Err(AppError::new("location_missing", "资源缺少物理位置"));
    };
    let path = std::path::PathBuf::from(&loc.path);
    if !path.is_file() {
        return Err(AppError::new("not_file", "该资源不是文件"));
    }
    editor_service::read_disk_full(&path)
}

/// 最近打开的文件（编辑器会话按更新时间倒序）。
#[tauri::command]
pub fn list_recent_files(state: State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = lock_db(&state);
    let rows = editor_service::list_recent_files(&conn, 20)?;
    Ok(rows
        .into_iter()
        .map(|(id, name, path, updated_at)| {
            serde_json::json!({ "id": id, "name": name, "path": path, "updated_at": updated_at })
        })
        .collect())
}

/// 保存文件。磁盘指纹未变化则写回，变化时返回 conflict。
#[tauri::command]
pub fn save_file(
    state: State<AppState>,
    resource_id: String,
    content: String,
) -> CommandResult<serde_json::Value> {
    save_impl(&state, &resource_id, &content, false)
}

/// 强制覆盖保存（用户确认冲突后）。
#[tauri::command]
pub fn save_file_force(
    state: State<AppState>,
    resource_id: String,
    content: String,
) -> CommandResult<serde_json::Value> {
    save_impl(&state, &resource_id, &content, true)
}

fn save_impl(
    state: &AppState,
    resource_id: &str,
    content: &str,
    force: bool,
) -> CommandResult<serde_json::Value> {
    let conn = lock_db(state);
    let locations = crate::db::repositories::list_locations(&conn, resource_id)?;
    let Some(loc) = locations.first() else {
        return Err(AppError::new("location_missing", "资源缺少物理位置"));
    };
    let path = std::path::PathBuf::from(&loc.path);

    match editor_service::save_session(&conn, resource_id, &path, content, force)? {
        editor_service::SaveOutcome::Saved => {
            let (size, modified) = file_service::stat_basic(&path)?;
            crate::db::repositories::update_location_stat(&conn, &loc.id, size, modified)?;
            Ok(serde_json::json!({ "status": "saved" }))
        }
        editor_service::SaveOutcome::Conflict {
            message,
            current_size,
        } => Ok(serde_json::json!({
            "status": "conflict",
            "message": message,
            "current_size": current_size,
        })),
    }
}

/// 放弃当前编辑会话（丢弃未保存内容）。
#[tauri::command]
pub fn discard_session(state: State<AppState>, resource_id: String) -> CommandResult<()> {
    let conn = lock_db(&state);
    editor_service::discard_session(&conn, &resource_id)?;
    Ok(())
}
