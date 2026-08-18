use std::path::PathBuf;

use rusqlite::OptionalExtension;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::db::connection::now_unix;
use crate::db::models::{Resource, ResourceKind, ResourceLocation, SourceType};
use crate::db::repositories as repo;
use crate::error::AppError;
use crate::events::*;
use crate::ipc::CommandResult;
use crate::services::file_service as fsutil;
use crate::AppState;

fn lock_db<'a>(state: &'a AppState) -> std::sync::MutexGuard<'a, rusqlite::Connection> {
    state.conn.lock().expect("db lock poisoned")
}

fn emit_resource_changed(app: &AppHandle, payload: &serde_json::Value) {
    let _ = app.emit(EVENT_RESOURCE_CHANGED, payload);
}

fn emit_trash_updated(app: &AppHandle) {
    let _ = app.emit(EVENT_TRASH_UPDATED, ());
}

/// 列出某目录下的资源。parent_id 为空时列出根目录。
/// 正常浏览时过滤「代码项目」和「页面」资源，保持文件、项目、页面三个空间分开。
#[tauri::command]
pub fn list_children(
    state: State<AppState>,
    parent_id: Option<String>,
    include_deleted: Option<bool>,
) -> CommandResult<Vec<Resource>> {
    let conn = lock_db(&state);
    let mut items = repo::list_children(
        &conn,
        parent_id.as_deref(),
        include_deleted.unwrap_or(false),
    )?;
    if !include_deleted.unwrap_or(false) {
        items.retain(|r| r.kind != ResourceKind::Project && r.kind != ResourceKind::Page);
    }
    Ok(items)
}

/// 获取单个资源及其位置。
#[tauri::command]
pub fn get_resource(state: State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let conn = lock_db(&state);
    let resource = repo::get_resource(&conn, &id)?
        .ok_or_else(|| AppError::new("not_found", format!("资源 {id} 不存在")))?;
    let locations = repo::list_locations(&conn, &id)?;
    Ok(serde_json::json!({
        "resource": resource,
        "locations": locations,
    }))
}

/// 按文件路径查找资源（模糊匹配 canonical_path）。
#[tauri::command]
pub fn find_resource_by_path(
    state: State<AppState>,
    path: String,
) -> CommandResult<Option<Resource>> {
    let conn = lock_db(&state);
    // 尝试规范化路径
    let canonical = fsutil::canonical_path_key(&path);
    if let Some(loc) = repo::find_location_by_path(&conn, &canonical)? {
        if let Some(res) = repo::get_resource(&conn, &loc.resource_id)? {
            return Ok(Some(res));
        }
    }
    // 回退：直接用原始路径匹配
    let alt = format!("SELECT * FROM resource_locations WHERE path LIKE ?1 LIMIT 1");
    let row = conn
        .query_row(&alt, [&format!("%{}", path)], |row| {
            row.get::<_, String>("resource_id")
        })
        .optional()
        .map_err(AppError::from)?;
    if let Some(rid) = row {
        return repo::get_resource(&conn, &rid).map_err(AppError::from);
    }
    Ok(None)
}

/// 在文件系统中创建文件夹，并写入资源记录。
#[tauri::command]
pub fn create_folder(
    state: State<AppState>,
    app: AppHandle,
    parent_id: Option<String>,
    name: String,
) -> CommandResult<Resource> {
    if name.trim().is_empty() || name.contains(['/', '\\', ':']) {
        return Err(AppError::new("invalid_name", "文件夹名称不合法"));
    }

    let conn = lock_db(&state);
    let now = now_unix();
    let parent_path = resolve_parent_dir(&state, &conn, parent_id.as_deref())?;
    let new_dir = parent_path.join(&name);
    if new_dir.exists() {
        return Err(AppError::new(
            "path_exists",
            format!("{} 已存在", new_dir.display()),
        ));
    }

    let id = Uuid::new_v4().to_string();
    std::fs::create_dir_all(&new_dir)?;
    let resource = repo::create_folder(&conn, &id, &name, parent_id.as_deref(), now)?;

    let (size, modified) = fsutil::stat_basic(&new_dir)?;
    let normalized = fsutil::normalize_path(&new_dir)?;
    repo::upsert_location(
        &conn,
        &ResourceLocation {
            id: Uuid::new_v4().to_string(),
            resource_id: id.clone(),
            source_type: SourceType::External,
            path: normalized.clone(),
            canonical_path: Some(fsutil::canonical_path_key(&normalized)),
            file_size: Some(size),
            modified_at: modified,
            created_at: now,
            last_verified_at: Some(now),
            content_hash: None,
            hash_algorithm: None,
            is_available: true,
        },
    )?;

    emit_resource_changed(&app, &serde_json::json!({ "parent_id": parent_id }));
    Ok(resource)
}

/// 重命名资源，同时重命名磁盘上的文件或文件夹。
#[tauri::command]
pub fn rename_resource(
    state: State<AppState>,
    app: AppHandle,
    id: String,
    new_name: String,
) -> CommandResult<Resource> {
    if new_name.trim().is_empty() || new_name.contains(['/', '\\', ':']) {
        return Err(AppError::new("invalid_name", "名称不合法"));
    }

    let conn = lock_db(&state);
    let now = now_unix();
    let _resource = repo::get_resource(&conn, &id)?
        .ok_or_else(|| AppError::new("not_found", format!("资源 {id} 不存在")))?;

    let locations = repo::list_locations(&conn, &id)?;
    if let Some(loc) = locations.first() {
        let src = PathBuf::from(&loc.path);
        if src.exists() {
            let dst = src
                .parent()
                .ok_or_else(|| AppError::new("path_error", "无法获取父路径"))?
                .join(&new_name);
            if dst.exists() {
                return Err(AppError::new(
                    "path_exists",
                    format!("{} 已存在", dst.display()),
                ));
            }
            std::fs::rename(&src, &dst)?;
            let normalized = fsutil::normalize_path(&dst)?;
            repo::upsert_location(
                &conn,
                &ResourceLocation {
                    id: loc.id.clone(),
                    resource_id: id.clone(),
                    source_type: loc.source_type,
                    path: normalized.clone(),
                    canonical_path: Some(fsutil::canonical_path_key(&normalized)),
                    file_size: loc.file_size,
                    modified_at: loc.modified_at,
                    created_at: loc.created_at,
                    last_verified_at: loc.last_verified_at,
                    content_hash: loc.content_hash.clone(),
                    hash_algorithm: loc.hash_algorithm.clone(),
                    is_available: true,
                },
            )?;
        }
    }

    repo::rename_resource(&conn, &id, &new_name, now)?;
    let updated = repo::get_resource(&conn, &id)?.expect("resource exists after rename");
    emit_resource_changed(&app, &serde_json::json!({ "parent_id": updated.parent_id }));
    Ok(updated)
}

/// 移动资源到新父目录（文件系统 + 数据库）。
#[tauri::command]
pub fn move_resource(
    state: State<AppState>,
    app: AppHandle,
    id: String,
    new_parent_id: Option<String>,
) -> CommandResult<Resource> {
    let conn = lock_db(&state);
    let now = now_unix();

    if let Some(pid) = new_parent_id.as_deref() {
        if pid == id {
            return Err(AppError::new("invalid_move", "不能移动到自身"));
        }
    }

    let resource = repo::get_resource(&conn, &id)?
        .ok_or_else(|| AppError::new("not_found", format!("资源 {id} 不存在")))?;
    let locations = repo::list_locations(&conn, &id)?;

    if let Some(loc) = locations.first() {
        let src = PathBuf::from(&loc.path);
        let dst_root = resolve_parent_dir(&state, &conn, new_parent_id.as_deref())?;
        let dst = dst_root.join(&resource.name);
        if src.exists() && dst != src {
            if dst.exists() {
                return Err(AppError::new(
                    "path_exists",
                    format!("{} 已存在", dst.display()),
                ));
            }
            std::fs::rename(&src, &dst)?;
            let normalized = fsutil::normalize_path(&dst)?;
            repo::upsert_location(
                &conn,
                &ResourceLocation {
                    id: loc.id.clone(),
                    resource_id: id.clone(),
                    source_type: loc.source_type,
                    path: normalized.clone(),
                    canonical_path: Some(fsutil::canonical_path_key(&normalized)),
                    file_size: loc.file_size,
                    modified_at: loc.modified_at,
                    created_at: loc.created_at,
                    last_verified_at: loc.last_verified_at,
                    content_hash: loc.content_hash.clone(),
                    hash_algorithm: loc.hash_algorithm.clone(),
                    is_available: true,
                },
            )?;
        }
    }

    repo::move_resource(&conn, &id, new_parent_id.as_deref(), now)?;
    let updated = repo::get_resource(&conn, &id)?.expect("resource exists after move");
    emit_resource_changed(&app, &serde_json::json!({ "parent_id": updated.parent_id }));
    Ok(updated)
}

/// 批量移入回收站（软删除）。
#[tauri::command]
pub fn trash_resources(
    state: State<AppState>,
    app: AppHandle,
    ids: Vec<String>,
) -> CommandResult<usize> {
    let mut conn = lock_db(&state);
    let now = now_unix();
    let tx = conn.transaction()?;
    let mut count = 0;
    for id in &ids {
        tx.execute(
            "UPDATE resources SET is_deleted = 1, deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            rusqlite::params![id, now],
        )?;
        count += 1;
    }
    tx.commit()?;
    emit_trash_updated(&app);
    Ok(count)
}

/// 从回收站恢复。原父目录不存在时恢复到根目录。
#[tauri::command]
pub fn restore_resource(
    state: State<AppState>,
    app: AppHandle,
    id: String,
) -> CommandResult<Resource> {
    let conn = lock_db(&state);
    let now = now_unix();
    let resource = repo::get_resource(&conn, &id)?
        .ok_or_else(|| AppError::new("not_found", format!("资源 {id} 不存在")))?;

    let mut target_parent = resource.parent_id.clone();
    if let Some(pid) = &resource.parent_id {
        let parent = repo::get_resource(&conn, pid)?
            .ok_or_else(|| AppError::new("parent_missing", "原父目录已删除"))?;
        if parent.is_deleted {
            target_parent = None;
        }
    }
    repo::move_resource(&conn, &id, target_parent.as_deref(), now)?;
    repo::restore(&conn, &id, now)?;

    let updated = repo::get_resource(&conn, &id)?.expect("resource exists after restore");
    emit_resource_changed(&app, &serde_json::json!({ "parent_id": updated.parent_id }));
    emit_trash_updated(&app);
    Ok(updated)
}

/// 从回收站批量恢复。原父目录不存在时恢复到根目录。
#[tauri::command]
pub fn restore_resources(
    state: State<AppState>,
    app: AppHandle,
    ids: Vec<String>,
) -> CommandResult<usize> {
    let conn = lock_db(&state);
    let now = now_unix();
    let mut count = 0;
    for id in &ids {
        let Some(resource) = repo::get_resource(&conn, id)? else {
            continue;
        };
        let mut target_parent = resource.parent_id.clone();
        if let Some(pid) = &resource.parent_id {
            if let Ok(Some(parent)) = repo::get_resource(&conn, pid) {
                if parent.is_deleted {
                    target_parent = None;
                }
            }
        }
        repo::move_resource(&conn, id, target_parent.as_deref(), now)?;
        repo::restore(&conn, id, now)?;
        count += 1;
    }
    emit_trash_updated(&app);
    emit_resource_changed(&app, &serde_json::json!({ "parent_id": null }));
    Ok(count)
}

/// 列出回收站资源。
#[tauri::command]
pub fn list_trash(state: State<AppState>) -> CommandResult<Vec<Resource>> {
    let conn = lock_db(&state);
    Ok(repo::list_trash(&conn)?)
}

/// 切换资源收藏状态，返回新状态。
#[tauri::command]
pub fn toggle_favorite(state: State<AppState>, id: String) -> CommandResult<bool> {
    let conn = lock_db(&state);
    Ok(repo::toggle_favorite(&conn, &id, now_unix())?)
}

/// 列出全部收藏资源。
#[tauri::command]
pub fn list_favorites(state: State<AppState>) -> CommandResult<Vec<Resource>> {
    let conn = lock_db(&state);
    Ok(repo::list_favorites(&conn)?)
}

/// 获取资源从根到父级的祖先链（不含资源自身），用于跨页面跳转定位目录。
#[tauri::command]
pub fn get_ancestors(state: State<AppState>, id: String) -> CommandResult<Vec<Resource>> {
    let conn = lock_db(&state);
    let mut chain: Vec<Resource> = Vec::new();
    let mut cursor: Option<String> = repo::get_resource(&conn, &id)?.and_then(|r| r.parent_id);
    while let Some(pid) = cursor {
        let parent = repo::get_resource(&conn, &pid)?
            .ok_or_else(|| AppError::new("parent_missing", format!("父资源 {pid} 不存在")))?;
        cursor = parent.parent_id.clone();
        chain.push(parent);
    }
    chain.reverse();
    Ok(chain)
}

/// 永久删除（先删磁盘文件/目录，全部成功后再删除数据库记录）。
/// 磁盘删除失败时保留数据库记录并返回错误，供用户重试定位。
#[tauri::command]
pub fn delete_permanently(
    state: State<AppState>,
    app: AppHandle,
    ids: Vec<String>,
) -> CommandResult<usize> {
    let mut conn = lock_db(&state);
    let tx = conn.transaction()?;
    let mut count = 0;
    let mut errors: Vec<String> = Vec::new();

    for id in &ids {
        let locations = repo::list_locations(&tx, id)?;
        for loc in &locations {
            let path = PathBuf::from(&loc.path);
            if !path.exists() {
                continue;
            }
            let res = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            if let Err(e) = res {
                // 物理删除失败：保留数据库记录，便于重试
                errors.push(format!("{}: {e}", loc.path));
            }
        }
        if errors.is_empty() {
            tx.execute("DELETE FROM file_metadata WHERE resource_id = ?1", [id])?;
            tx.execute(
                "DELETE FROM resource_locations WHERE resource_id = ?1",
                [id],
            )?;
            tx.execute("DELETE FROM resources WHERE id = ?1", [id])?;
            count += 1;
        }
    }

    if !errors.is_empty() {
        let joined = errors.join("; ");
        drop(tx);
        return Err(AppError::new(
            "delete_failed",
            format!("部分文件删除失败，记录已保留可重试: {joined}"),
        ));
    }

    tx.commit()?;
    emit_trash_updated(&app);
    Ok(count)
}

/// 验证资源所有物理位置是否可用，并更新数据库状态。
#[tauri::command]
pub fn verify_location(state: State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let conn = lock_db(&state);
    let now = now_unix();
    let locations = repo::list_locations(&conn, &id)?;
    let mut results = Vec::new();
    for loc in &locations {
        let available = PathBuf::from(&loc.path).exists();
        repo::set_location_availability(&conn, &loc.id, available, now)?;
        results.push(serde_json::json!({
            "id": loc.id,
            "path": loc.path,
            "is_available": available,
        }));
    }
    Ok(serde_json::json!({ "results": results }))
}

/// 解析父资源在文件系统中的目录路径。根目录使用应用数据目录下的 managed-files。
fn resolve_parent_dir(
    state: &AppState,
    conn: &rusqlite::Connection,
    parent_id: Option<&str>,
) -> Result<PathBuf, AppError> {
    if let Some(pid) = parent_id {
        let parent = repo::get_resource(conn, pid)?
            .ok_or_else(|| AppError::new("parent_missing", format!("父目录 {pid} 不存在")))?;
        if parent.kind != ResourceKind::Folder {
            return Err(AppError::new("not_folder", "父资源不是文件夹"));
        }
        let locations = repo::list_locations(conn, pid)?;
        if let Some(loc) = locations.first() {
            return Ok(PathBuf::from(&loc.path));
        }
        return Err(AppError::new(
            "location_missing",
            format!("父目录 {pid} 缺少物理位置"),
        ));
    }

    let root = state.managed_dir.lock().expect("dir lock").clone();
    std::fs::create_dir_all(&root)?;
    Ok(root)
}
