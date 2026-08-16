//! Web 端口预览命令：打开预览并返回已验证目标。

use tauri::State;

use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::web_preview_service::PreviewTarget;
use crate::AppState;

/// 打开项目运行预览：解析目标 → 校验端口监听 → 校验 Job 归属。
///
/// 端口未监听或无可用目标时返回 `preview_unavailable`；
/// 归属无法确认时返回 `ownership = unconfirmed`，由前端提示用户手动打开。
#[tauri::command]
pub fn open_project_preview(
    state: State<AppState>,
    run_id: String,
) -> CommandResult<PreviewTarget> {
    state
        .preview
        .open_preview(&run_id)
        .map_err(|e| AppError::new(e.code, e.message))
}
