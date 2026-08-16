use tauri::State;

use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::migration_service::{self, MigrateTarget};
use crate::AppState;

fn parse_which(s: &str) -> Result<MigrateTarget, AppError> {
    match s {
        "data_dir" => Ok(MigrateTarget::DataDir),
        "managed_dir" => Ok(MigrateTarget::ManagedDir),
        _ => Err(AppError::new("invalid_target", "迁移目标类型错误")),
    }
}

/// 校验迁移目标目录（不实际执行）。
#[tauri::command]
pub fn validate_migration_target(
    state: State<AppState>,
    target: String,
    which: String,
) -> CommandResult<()> {
    let which = parse_which(&which)?;
    migration_service::validate_target_dir(&state, std::path::Path::new(&target), which)?;
    Ok(())
}

/// 启动迁移，返回任务 ID。
#[tauri::command]
pub fn start_migration(
    app: tauri::AppHandle,
    target: String,
    which: String,
) -> CommandResult<String> {
    let which = parse_which(&which)?;
    migration_service::start_migration(app, target, which)
}

/// 查询迁移状态。
#[tauri::command]
pub fn get_migration_status(state: State<AppState>) -> CommandResult<Option<serde_json::Value>> {
    migration_service::get_migration_status(&state)
}

/// 取消迁移。
#[tauri::command]
pub fn cancel_migration(state: State<AppState>, task_id: String) -> CommandResult<()> {
    migration_service::cancel_migration(&state, &task_id)
}
