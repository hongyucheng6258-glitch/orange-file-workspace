use tauri::State;

use crate::AppState;
use crate::ipc::CommandResult;
use crate::services::backup_service;

/// 创建备份。include_files=true 时包含托管文件（完整备份）。
#[tauri::command]
pub fn create_backup(
    state: State<AppState>,
    include_files: bool,
) -> CommandResult<crate::db::models::BackupRecord> {
    let (_, record) = backup_service::create_backup(&state, include_files)?;
    Ok(record)
}

/// 列出备份记录。
#[tauri::command]
pub fn list_backups(state: State<AppState>) -> CommandResult<Vec<crate::db::models::BackupRecord>> {
    backup_service::list_backups(&state)
}

/// 恢复指定备份。
#[tauri::command]
pub fn restore_backup(state: State<AppState>, backup_path: String) -> CommandResult<()> {
    backup_service::restore_from_dir(&state, std::path::Path::new(&backup_path))?;
    Ok(())
}

/// 应用环境信息（设置页使用）。
#[tauri::command]
pub fn app_environment(state: State<AppState>) -> CommandResult<serde_json::Value> {
    Ok(serde_json::json!({
        "name": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "data_dir": state.data_dir.to_string_lossy(),
        "db_path": state.data_dir.join("workspace.db").to_string_lossy(),
    }))
}
