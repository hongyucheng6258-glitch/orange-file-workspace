use crate::ipc::CommandResult;
use crate::services::autostart;

/// 设置开机自启，返回设置后的实际状态。
#[tauri::command]
pub fn set_autostart(enabled: bool) -> CommandResult<bool> {
    autostart::set_enabled(enabled)?;
    Ok(autostart::is_enabled())
}

/// 查询当前开机自启状态。
#[tauri::command]
pub fn get_autostart() -> CommandResult<bool> {
    Ok(autostart::is_enabled())
}
