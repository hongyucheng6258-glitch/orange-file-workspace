use rusqlite::OptionalExtension;
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
    // 目录迁移进行中拒绝备份
    {
        let conn = state.conn.lock().expect("db lock");
        if crate::db::repositories::get_setting(
            &conn,
            crate::services::migration_service::KEY_INFLIGHT,
        )?
        .is_some()
        {
            return Err(crate::error::AppError::new(
                "migration_running",
                "目录迁移进行中，请稍后再试",
            ));
        }
    }
    let (_, record) = backup_service::create_backup(&state, include_files, "manual")?;
    Ok(record)
}

/// 列出备份记录。
#[tauri::command]
pub fn list_backups(state: State<AppState>) -> CommandResult<Vec<crate::db::models::BackupRecord>> {
    backup_service::list_backups(&state)
}

/// 恢复指定备份（含完整性校验、保护备份与失败回滚）。
#[tauri::command]
pub fn restore_backup(state: State<AppState>, backup_path: String) -> CommandResult<()> {
    backup_service::restore_from_dir(&state, std::path::Path::new(&backup_path))?;
    Ok(())
}

/// 删除一条备份记录及其目录。
#[tauri::command]
pub fn delete_backup(state: State<AppState>, backup_id: String) -> CommandResult<()> {
    backup_service::delete_backup(&state, &backup_id)?;
    Ok(())
}

/// 导出备份到指定目录。
#[tauri::command]
pub fn export_backup(
    state: State<AppState>,
    backup_id: String,
    dest_dir: String,
) -> CommandResult<()> {
    backup_service::export_backup(&state, &backup_id, std::path::Path::new(&dest_dir))?;
    Ok(())
}

/// 返回备份根目录路径（前端用资源管理器打开）。
#[tauri::command]
pub fn reveal_backups(state: State<AppState>) -> CommandResult<String> {
    let dir = state.data_dir.lock().expect("dir lock").join("backups");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.to_string_lossy().to_string())
}

/// 校验备份是否可恢复（前端在确认恢复前调用）。
#[tauri::command]
pub fn validate_backup(
    state: State<AppState>,
    backup_id: String,
) -> CommandResult<serde_json::Value> {
    let conn = state.conn.lock().expect("db lock");
    let path: String = conn
        .query_row("SELECT path FROM backup_records WHERE id = ?1", [&backup_id], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or_else(|| crate::error::AppError::new("not_found", "备份不存在"))?;
    drop(conn);
    backup_service::validate_backup(std::path::Path::new(&path))
}

/// 应用环境信息（设置页使用）。
#[tauri::command]
pub fn app_environment(state: State<AppState>) -> CommandResult<serde_json::Value> {
    let data_dir = state.data_dir.lock().expect("dir lock");
    let managed_dir = state.managed_dir.lock().expect("dir lock");
    Ok(serde_json::json!({
        "name": "Orange",
        "version": env!("CARGO_PKG_VERSION"),
        "data_dir": data_dir.to_string_lossy(),
        "managed_dir": managed_dir.to_string_lossy(),
        "db_path": data_dir.join("workspace.db").to_string_lossy(),
    }))
}
