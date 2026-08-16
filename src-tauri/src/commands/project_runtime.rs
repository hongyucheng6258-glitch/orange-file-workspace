//! 项目运行 Tauri 命令：识别、确认、启动/停止/重启、日志查询。
//!
//! 命令层只做项目资源解析、参数映射和错误转换；进程管理全部委托
//! `RuntimeManager`，不直接调用 Win32 FFI，也不持有 SQLite 锁等待进程。

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::project_detector::{self, DiskProjectFs};
use crate::services::project_runtime::{
    ErrorPayload, ExitedPayload, OutputPayload, RunEventSink, RunSnapshot, RuntimeManager,
    StatusPayload,
};
use crate::services::run_confirmation::{ConfirmationGrant, ConfirmationPreview, RunConfig};
use crate::AppState;

/// 生产事件输出：通过 AppHandle 广播给前端。
pub struct AppRunEventSink {
    app: AppHandle,
}

impl AppRunEventSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl RunEventSink for AppRunEventSink {
    fn emit_status(&self, p: &StatusPayload) {
        let _ = self.app.emit(crate::events::EVENT_PROJECT_PROCESS_STATUS, p);
    }
    fn emit_output(&self, p: &OutputPayload) {
        let _ = self.app.emit(crate::events::EVENT_PROJECT_PROCESS_OUTPUT, p);
    }
    fn emit_exited(&self, p: &ExitedPayload) {
        let _ = self.app.emit(crate::events::EVENT_PROJECT_PROCESS_EXITED, p);
    }
    fn emit_error(&self, p: &ErrorPayload) {
        let _ = self.app.emit(crate::events::EVENT_PROJECT_PROCESS_ERROR, p);
    }
    fn emit_preview(&self, p: &crate::services::web_preview_service::PreviewTarget) {
        let _ = self.app.emit(crate::events::EVENT_PROJECT_PREVIEW_READY, p);
    }
}

fn lock_db<'a>(state: &'a AppState) -> std::sync::MutexGuard<'a, rusqlite::Connection> {
    state.conn.lock().expect("db lock poisoned")
}

/// 解析项目根目录路径。
fn project_root(conn: &rusqlite::Connection, project_id: &str) -> Result<PathBuf, AppError> {
    let resource = crate::db::repositories::get_resource(conn, project_id)?
        .ok_or_else(|| AppError::new("not_found", format!("项目 {project_id} 不存在")))?;
    let locations = crate::db::repositories::list_locations(conn, &resource.id)?;
    let loc = locations
        .first()
        .ok_or_else(|| AppError::new("not_found", "项目位置记录缺失"))?;
    Ok(PathBuf::from(&loc.path))
}

fn runtime(state: &AppState) -> Arc<RuntimeManager> {
    state.runtime.clone()
}

/// 识别项目运行时并生成候选命令。
#[tauri::command]
pub fn detect_project_runtime(
    state: State<AppState>,
    project_id: String,
) -> CommandResult<project_detector::DetectionResult> {
    let conn = lock_db(&state);
    let root = project_root(&conn, &project_id)?;
    Ok(project_detector::detect(&DiskProjectFs, &root))
}

/// 生成脱敏确认预览与一次性确认票据。
#[tauri::command]
pub fn prepare_run_confirmation(
    state: State<AppState>,
    project_id: String,
    config: RunConfig,
) -> CommandResult<ConfirmationPreview> {
    let conn = lock_db(&state);
    let root = project_root(&conn, &project_id)?;
    drop(conn);
    let manager = runtime(&state);
    manager
        .prepare_confirmation(&config, &root)
        .map_err(Into::into)
}

/// 兑换一次性确认票据并签发确认哈希。
#[tauri::command]
pub fn confirm_run_config(
    state: State<AppState>,
    confirmation_id: String,
) -> CommandResult<ConfirmationGrant> {
    runtime(&state)
        .confirm_config(&confirmation_id)
        .map_err(Into::into)
}

/// 校验确认哈希后启动项目进程。
#[tauri::command]
pub fn start_project_process(
    state: State<AppState>,
    project_id: String,
    config: RunConfig,
    confirmation_hash: String,
) -> CommandResult<RunSnapshot> {
    let conn = lock_db(&state);
    let root = project_root(&conn, &project_id)?;
    drop(conn);
    let manager = runtime(&state);
    manager
        .start(&config, &root, &confirmation_hash)
        .map_err(Into::into)
}

/// 停止运行实例（幂等）。
#[tauri::command]
pub fn stop_project_process(state: State<AppState>, run_id: String) -> CommandResult<RunSnapshot> {
    runtime(&state).stop(&run_id).map_err(Into::into)
}

/// 重启运行实例（复用后端保存的规范化配置快照）。
#[tauri::command]
pub fn restart_project_process(
    state: State<AppState>,
    run_id: String,
) -> CommandResult<RunSnapshot> {
    runtime(&state).restart(&run_id).map_err(Into::into)
}

/// 查询项目当前运行状态（活动或最近终态）。
#[tauri::command]
pub fn get_project_run(
    state: State<AppState>,
    project_id: String,
) -> CommandResult<Option<RunSnapshot>> {
    let conn = lock_db(&state);
    let root = project_root(&conn, &project_id)?;
    drop(conn);
    let key = crate::services::run_confirmation::canonical_key(&root)
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| AppError::new("invalid_working_directory", "项目根目录无法解析"))?;
    Ok(runtime(&state).get_run_by_project_key(&key))
}

/// 获取运行实例当前保留的日志分页。
#[tauri::command]
pub fn get_process_logs(
    state: State<AppState>,
    run_id: String,
    after_seq: u64,
) -> CommandResult<crate::services::project_runtime::LogPage> {
    runtime(&state)
        .get_logs(&run_id, after_seq)
        .map_err(Into::into)
}
