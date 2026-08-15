use tauri::State;

use crate::db::repositories as repo;
use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::drag_service;
use crate::AppState;

/// 从应用内拖出文件/文件夹到系统（资源管理器、桌面等）。
/// 阻塞直到拖拽结束，返回参与拖出的资源数量。
#[tauri::command]
pub fn drag_out(state: State<AppState>, ids: Vec<String>) -> CommandResult<usize> {
    if ids.is_empty() {
        return Err(AppError::new("no_selection", "没有选择要拖出的资源"));
    }

    let mut paths: Vec<String> = Vec::new();
    {
        let conn = state.conn.lock().expect("db lock poisoned");
        for id in &ids {
            if let Ok(locations) = repo::list_locations(&conn, id) {
                if let Some(loc) = locations
                    .iter()
                    .find(|l| l.is_available)
                    .or_else(|| locations.first())
                {
                    paths.push(loc.path.clone());
                }
            }
        }
    }

    if paths.is_empty() {
        return Err(AppError::new("drag_out_failed", "所选资源没有可用的位置"));
    }

    let count = paths.len();
    drag_service::start_drag_out(paths)
        .map_err(|message| AppError::new("drag_out_failed", message))?;
    Ok(count)
}
