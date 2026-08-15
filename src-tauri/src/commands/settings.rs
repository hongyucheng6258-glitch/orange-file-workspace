use serde_json::Value;
use tauri::State;

use crate::AppState;
use crate::ipc::CommandResult;
use crate::services::settings_service::{self, AppSettings};

fn lock_db<'a>(state: &'a AppState) -> std::sync::MutexGuard<'a, rusqlite::Connection> {
    state.conn.lock().expect("db lock poisoned")
}

/// 读取完整设置（storage 附带当前目录信息）。
#[tauri::command]
pub fn get_settings(state: State<AppState>) -> CommandResult<AppSettings> {
    let conn = lock_db(&state);
    let mut s = settings_service::load_settings(&conn)?;
    fill_storage(&state, &mut s);
    Ok(s)
}

/// 更新单个设置项，返回最新完整设置。
#[tauri::command]
pub fn update_setting(
    state: State<AppState>,
    key: String,
    value: Value,
) -> CommandResult<AppSettings> {
    let conn = lock_db(&state);
    let mut s = settings_service::update_setting(&conn, &key, value)?;
    fill_storage(&state, &mut s);
    Ok(s)
}

/// 重置某个分类为默认值，返回最新完整设置。
#[tauri::command]
pub fn reset_settings_category(
    state: State<AppState>,
    category: String,
) -> CommandResult<AppSettings> {
    let conn = lock_db(&state);
    let mut s = settings_service::reset_category(&conn, &category)?;
    fill_storage(&state, &mut s);
    Ok(s)
}

/// 用当前目录信息填充 storage 字段。
fn fill_storage(state: &AppState, s: &mut AppSettings) {
    let data_dir = state.data_dir.lock().expect("dir lock");
    let managed_dir = state.managed_dir.lock().expect("dir lock");
    s.storage.data_dir = data_dir.to_string_lossy().to_string();
    s.storage.managed_dir = managed_dir.to_string_lossy().to_string();
}
