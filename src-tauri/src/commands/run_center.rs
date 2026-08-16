//! 运行中心命令：跨项目运行列表。

use tauri::State;

use crate::ipc::CommandResult;
use crate::services::project_runtime::RunSnapshot;
use crate::AppState;

/// 列出运行实例：活动 + 清理中；`include_exited` 时追加已退出记录
/// （内存最近一条 + 落库历史，按启动时间倒序，run_id 去重）。
#[tauri::command]
pub fn list_project_runs(
    state: State<AppState>,
    include_exited: bool,
) -> CommandResult<Vec<RunSnapshot>> {
    Ok(state.runtime.list_runs(include_exited))
}
