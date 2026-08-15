use std::sync::MutexGuard;

use tauri::{AppHandle, Emitter, State};

use crate::AppState;
use crate::db::models::Resource;
use crate::db::repositories as repo;
use crate::error::AppError;
use crate::events::EVENT_RESOURCE_CHANGED;
use crate::ipc::CommandResult;
use crate::services::page_service::{self, BlockInput};

fn lock_db<'a>(state: &'a AppState) -> MutexGuard<'a, rusqlite::Connection> {
    state.conn.lock().expect("db lock poisoned")
}

/// 创建页面。
#[tauri::command]
pub fn create_page(
    state: State<AppState>,
    app: AppHandle,
    name: String,
    parent_id: Option<String>,
) -> CommandResult<serde_json::Value> {
    if name.trim().is_empty() {
        return Err(AppError::new("invalid_name", "页面名称不能为空"));
    }
    let conn = lock_db(&state);
    let (resource, page) = page_service::create_page(&conn, &name, parent_id.as_deref())?;
    let _ = app.emit(EVENT_RESOURCE_CHANGED, serde_json::json!({ "parent_id": parent_id }));
    Ok(serde_json::json!({ "resource": resource, "page": page }))
}

/// 获取页面详情（资源 + 页面扩展 + 全部块）。
#[tauri::command]
pub fn get_page(
    state: State<AppState>,
    resource_id: String,
) -> CommandResult<serde_json::Value> {
    let conn = lock_db(&state);
    let resource = repo::get_resource(&conn, &resource_id)?.ok_or_else(|| {
        AppError::new("not_found", format!("页面 {resource_id} 不存在"))
    })?;
    let page = page_service::get_page(&conn, &resource_id)?.ok_or_else(|| {
        AppError::new("not_page", format!("{resource_id} 不是页面"))
    })?;
    let blocks = page_service::list_all_blocks(&conn, &resource_id)?;
    Ok(serde_json::json!({
        "resource": resource,
        "page": page,
        "blocks": blocks,
    }))
}

/// 保存页面全部块（全量替换）。
#[tauri::command]
pub fn save_page_blocks(
    state: State<AppState>,
    resource_id: String,
    blocks: Vec<serde_json::Value>,
) -> CommandResult<()> {
    let inputs: Vec<BlockInput> = blocks
        .iter()
        .map(|b| BlockInput {
            parent_block_id: b.get("parent_block_id").and_then(|v| v.as_str()).map(String::from),
            block_type: b
                .get("block_type")
                .and_then(|v| v.as_str())
                .unwrap_or("paragraph")
                .to_string(),
            content_json: b.get("content_json").map(|v| v.to_string()).unwrap_or_else(|| "{}".into()),
            plain_text: b
                .get("plain_text")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
        .collect();

    let mut conn = lock_db(&state);
    page_service::replace_blocks(&mut conn, &resource_id, &inputs)?;
    Ok(())
}

/// 列出页面树（根层或指定父目录下的页面资源）。
#[tauri::command]
pub fn list_pages(state: State<AppState>, parent_id: Option<String>) -> CommandResult<Vec<Resource>> {
    let conn = lock_db(&state);
    Ok(page_service::list_pages(&conn, parent_id.as_deref())?)
}

/// 重命名页面。
#[tauri::command]
pub fn rename_page(
    state: State<AppState>,
    app: AppHandle,
    resource_id: String,
    new_name: String,
) -> CommandResult<Resource> {
    if new_name.trim().is_empty() {
        return Err(AppError::new("invalid_name", "页面名称不能为空"));
    }
    let conn = lock_db(&state);
    let now = crate::db::connection::now_unix();
    repo::rename_resource(&conn, &resource_id, &new_name, now)?;
    let updated = repo::get_resource(&conn, &resource_id)?.expect("page exists");
    let _ = app.emit(EVENT_RESOURCE_CHANGED, serde_json::json!({ "parent_id": updated.parent_id }));
    Ok(updated)
}

/// 删除页面（软删除到回收站）。
#[tauri::command]
pub fn delete_page(state: State<AppState>, resource_id: String) -> CommandResult<()> {
    let conn = lock_db(&state);
    let now = crate::db::connection::now_unix();
    repo::soft_delete(&conn, &resource_id, now)?;
    Ok(())
}

/// 供测试使用的辅助函数（验证页面创建逻辑）。
#[cfg(test)]
pub fn _page_lifecycle_for_test(conn: &rusqlite::Connection) -> rusqlite::Result<crate::db::models::Page> {
    let (_, page) = page_service::create_page(conn, "测试页", None)?;
    Ok(page)
}
