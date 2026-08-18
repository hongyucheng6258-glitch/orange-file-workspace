use tauri::{AppHandle, State};

use crate::db::models::{new_id, SourceType, Task, TaskItem};
use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::import_service::{resolve_shortcut, start_import, ImportRequest};
use crate::services::migration_service::KEY_INFLIGHT;
use crate::services::task_service as tasks;
use crate::AppState;

/// 导入文件/文件夹。mode: "managed" 复制到仓库，或 "external" 仅引用路径。
/// 传入的 Windows 快捷方式（.lnk）会先解析为目标路径再导入。
#[tauri::command]
pub fn import_paths(
    app: AppHandle,
    state: State<AppState>,
    paths: Vec<String>,
    mode: String,
    parent_id: Option<String>,
) -> CommandResult<Task> {
    if paths.is_empty() {
        return Err(AppError::new("empty_paths", "没有可导入的文件"));
    }
    // 目录迁移进行中拒绝导入
    {
        let conn = state.conn.lock().expect("db lock");
        if crate::db::repositories::get_setting(&conn, KEY_INFLIGHT)?.is_some() {
            return Err(AppError::new(
                "migration_running",
                "目录迁移进行中，请稍后再导入",
            ));
        }
    }
    // 解析快捷方式：.lnk 指向文件夹时按文件夹导入其内容
    let resolved_paths: Vec<String> = paths
        .iter()
        .map(|p| {
            resolve_shortcut(std::path::Path::new(p))
                .to_string_lossy()
                .to_string()
        })
        .collect();
    let source_type = match mode.as_str() {
        "managed" => SourceType::Managed,
        "external" => SourceType::External,
        _ => {
            return Err(AppError::new(
                "invalid_mode",
                "导入模式必须为 managed 或 external",
            ))
        }
    };

    let conn = state.conn.lock().expect("db lock");
    let task = tasks::create_task(
        &conn,
        "import",
        &format!("导入 {} 个路径", paths.len()),
        None,
        Some(&serde_json::json!({ "paths": resolved_paths, "mode": mode }).to_string()),
    )?;
    drop(conn);

    let req = ImportRequest {
        paths: resolved_paths,
        mode: source_type,
        parent_id,
    };
    start_import(app, task.id.clone(), req);

    Ok(task)
}

/// 取消一个排队或运行中的任务。
#[tauri::command]
pub fn cancel_task(state: State<AppState>, task_id: String) -> CommandResult<()> {
    let conn = state.conn.lock().expect("db lock");
    tasks::request_cancel(&conn, &task_id)?;
    Ok(())
}

/// 查询最近任务列表。
#[tauri::command]
pub fn list_tasks(state: State<AppState>, limit: Option<i64>) -> CommandResult<Vec<Task>> {
    let conn = state.conn.lock().expect("db lock");
    Ok(tasks::list_tasks(&conn, limit.unwrap_or(50))?)
}

/// 查询任务的单项结果（含失败文件清单）。
#[tauri::command]
pub fn list_task_items(
    state: State<AppState>,
    task_id: String,
    limit: Option<i64>,
) -> CommandResult<Vec<TaskItem>> {
    let conn = state.conn.lock().expect("db lock");
    Ok(tasks::list_task_items(
        &conn,
        &task_id,
        limit.unwrap_or(200),
    )?)
}

#[allow(dead_code)]
fn _ensure_new_id_available() -> String {
    new_id()
}
