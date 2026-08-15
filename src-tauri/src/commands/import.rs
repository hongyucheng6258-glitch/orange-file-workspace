use tauri::{AppHandle, State};

use crate::AppState;
use crate::db::models::{new_id, SourceType, Task};
use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::import_service::{start_import, ImportRequest};
use crate::services::task_service as tasks;

/// 导入文件/文件夹。mode: "managed" 复制到仓库，或 "external" 仅引用路径。
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
        Some(&serde_json::json!({ "paths": paths, "mode": mode }).to_string()),
    )?;
    drop(conn);

    let req = ImportRequest {
        paths,
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

#[allow(dead_code)]
fn _ensure_new_id_available() -> String {
    new_id()
}
