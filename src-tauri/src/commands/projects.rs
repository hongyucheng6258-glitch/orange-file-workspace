use std::sync::MutexGuard;

use tauri::{AppHandle, State};

use crate::db::models::{Resource, SourceType, Task};
use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::import_service::{start_import, ImportRequest};
use crate::services::project_service;
use crate::services::task_service;
use crate::AppState;

fn lock_db<'a>(state: &'a AppState) -> MutexGuard<'a, rusqlite::Connection> {
    state.conn.lock().expect("db lock poisoned")
}

/// 导入本地目录为代码项目（external 引用 + 后台文件索引）。
#[tauri::command]
pub fn import_project(
    state: State<AppState>,
    app: AppHandle,
    root_path: String,
    name: Option<String>,
) -> CommandResult<Task> {
    let conn = lock_db(&state);
    let dir_name = std::path::Path::new(&root_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "项目".to_string());
    let project_name = name.unwrap_or(dir_name);

    let (_, _project) = project_service::create_project(&conn, &project_name, &root_path, &[])?;
    drop(conn);

    // 找到刚创建的项目资源 ID
    let project_id = {
        let conn = lock_db(&state);
        let normalized =
            crate::services::file_service::normalize_path(std::path::Path::new(&root_path))?;
        let canonical = crate::services::file_service::canonical_path_key(&normalized);
        let loc = crate::db::repositories::find_location_by_path(&conn, &canonical)?;
        loc.map(|l| l.resource_id)
    };
    let Some(pid) = project_id else {
        return Err(AppError::new("project_create_failed", "项目创建失败"));
    };

    let conn = lock_db(&state);
    let task = task_service::create_task(
        &conn,
        "project_scan",
        &format!("扫描项目 {project_name}"),
        None,
        Some(&serde_json::json!({ "root": root_path }).to_string()),
    )?;
    drop(conn);

    let req = ImportRequest {
        paths: vec![root_path],
        mode: SourceType::External,
        parent_id: Some(pid),
    };
    start_import(app, task.id.clone(), req);

    Ok(task)
}

/// 列出全部项目。
#[tauri::command]
pub fn list_projects(state: State<AppState>) -> CommandResult<Vec<Resource>> {
    let conn = lock_db(&state);
    Ok(project_service::list_projects(&conn)?)
}

/// 删除代码项目：软删除项目资源及其全部后代（进入回收站，可恢复）。
#[tauri::command]
pub fn delete_project(state: State<AppState>, project_id: String) -> CommandResult<()> {
    use crate::db::connection::now_unix;

    let conn = lock_db(&state);
    let now = now_unix();

    // 收集项目资源及其所有后代（递归）。
    let mut stmt = conn.prepare(
        "WITH RECURSIVE descendants(id) AS (
             SELECT id FROM resources WHERE id = ?1
             UNION ALL
             SELECT r.id FROM resources r
             JOIN descendants d ON r.parent_id = d.id
         )
         SELECT id FROM descendants",
    )?;
    let ids: Vec<String> = stmt
        .query_map([&project_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if ids.is_empty() {
        return Err(AppError::new(
            "not_found",
            format!("项目 {project_id} 不存在"),
        ));
    }

    let placeholders = vec!["?"; ids.len()].join(",");
    let args: Vec<&dyn rusqlite::ToSql> = std::iter::once(&now as &dyn rusqlite::ToSql)
        .chain(ids.iter().map(|s| s as &dyn rusqlite::ToSql))
        .collect();
    conn.execute(
        &format!(
            "UPDATE resources SET is_deleted = 1, deleted_at = ?1
             WHERE id IN ({placeholders})"
        ),
        rusqlite::params_from_iter(args),
    )?;
    Ok(())
}

/// 获取项目详情（资源 + 扩展记录 + 根目录路径）。
#[tauri::command]
pub fn get_project(state: State<AppState>, project_id: String) -> CommandResult<serde_json::Value> {
    let conn = lock_db(&state);
    let resource = crate::db::repositories::get_resource(&conn, &project_id)?
        .ok_or_else(|| AppError::new("not_found", format!("项目 {project_id} 不存在")))?;
    let project = project_service::get_project(&conn, &project_id)?;
    let locations = crate::db::repositories::list_locations(&conn, &project_id)?;
    Ok(serde_json::json!({
        "resource": resource,
        "project": project,
        "locations": locations,
    }))
}

/// 列出项目节点下的子资源（文件树按需展开）。
#[tauri::command]
pub fn list_project_files(
    state: State<AppState>,
    project_id: String,
    parent_id: Option<String>,
) -> CommandResult<Vec<Resource>> {
    let conn = lock_db(&state);
    let target = parent_id.unwrap_or(project_id);
    Ok(crate::db::repositories::list_children(
        &conn,
        Some(&target),
        false,
    )?)
}
