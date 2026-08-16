use std::path::PathBuf;
use std::sync::mpsc;

use tauri::{AppHandle, State, WebviewWindow};

use crate::db::repositories as repo;
use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::AppState;

/// 从应用内拖出文件/文件夹到系统（资源管理器、桌面等）。
/// 阻塞直到系统拖拽启动完成，返回参与拖出的资源数量。
#[tauri::command]
pub fn drag_out(
    app: AppHandle,
    window: WebviewWindow,
    state: State<AppState>,
    ids: Vec<String>,
) -> CommandResult<usize> {
    if ids.is_empty() {
        return Err(AppError::new("no_selection", "没有选择要拖出的资源"));
    }

    let paths = resolve_drag_paths(&state, &ids)?;
    let count = paths.len();
    let preview_icon = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons/64x64.png");
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    app.run_on_main_thread(move || {
        let result = drag::start_drag(
            &window,
            drag::DragItem::Files(paths),
            drag::Image::File(preview_icon),
            |_result, _cursor| {},
            drag::Options::default(),
        )
        .map_err(|error| format!("系统拖拽启动失败：{error}"));
        let _ = tx.send(result);
    })
    .map_err(|error| AppError::new("drag_out_failed", format!("调度系统拖拽失败：{error}")))?;

    match rx.recv() {
        Ok(Ok(())) => Ok(count),
        Ok(Err(message)) => Err(AppError::new("drag_out_failed", message)),
        Err(_) => Err(AppError::new("drag_out_failed", "拖拽线程意外退出")),
    }
}

fn resolve_drag_paths(state: &State<AppState>, ids: &[String]) -> CommandResult<Vec<PathBuf>> {
    let conn = state.conn.lock().expect("db lock poisoned");
    let paths = ids
        .iter()
        .filter_map(|id| repo::list_locations(&conn, id).ok())
        .filter_map(|locations| {
            locations
                .iter()
                .find(|location| location.is_available)
                .or_else(|| locations.first())
                .map(|location| PathBuf::from(&location.path))
        })
        .filter(|path| path.exists())
        .collect::<Vec<_>>();

    if paths.is_empty() {
        return Err(AppError::new(
            "drag_out_failed",
            "所选资源没有可用的本地位置",
        ));
    }
    Ok(paths)
}
