//! 软件使用时间统计命令：查询统计、暂停/恢复、设置空闲阈值、清除数据。

use tauri::State;

use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::app_usage_service::{self, AppUsageSummary};
use crate::AppState;

/// 查询软件使用时间统计。days: 1=今日, 7=近7天, 30=近30天。
#[tauri::command]
pub fn get_app_usage(state: State<AppState>, days: Option<u32>) -> CommandResult<AppUsageSummary> {
    let conn = state.conn.lock().expect("conn lock poisoned");
    let days = days.unwrap_or(1).clamp(1, 90);
    app_usage_service::query_usage(&conn, days)
}

/// 暂停使用时间统计。
#[tauri::command]
pub fn pause_app_usage(state: State<AppState>) -> CommandResult<()> {
    state.app_usage.pause();
    Ok(())
}

/// 恢复使用时间统计。
#[tauri::command]
pub fn resume_app_usage(state: State<AppState>) -> CommandResult<()> {
    state.app_usage.resume();
    Ok(())
}

/// 设置空闲阈值（秒）。低于此值认为用户在活跃使用。
#[tauri::command]
pub fn set_app_usage_idle_threshold(state: State<AppState>, seconds: i64) -> CommandResult<()> {
    state.app_usage.set_idle_threshold(seconds);
    Ok(())
}

/// 获取当前统计状态：是否暂停、空闲阈值。
#[tauri::command]
pub fn get_app_usage_status(state: State<AppState>) -> CommandResult<AppUsageStatus> {
    Ok(AppUsageStatus {
        paused: state.app_usage.is_paused(),
        idle_threshold_secs: state.app_usage.idle_threshold(),
    })
}

/// 清除所有使用时间统计数据。
#[tauri::command]
pub fn clear_app_usage(state: State<AppState>) -> CommandResult<()> {
    let conn = state.conn.lock().expect("conn lock poisoned");
    app_usage_service::clear_all_usage(&conn)
        .map_err(|m| AppError::new("clear_failed", m.to_string()))
}

/// 统计状态。
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AppUsageStatus {
    pub paused: bool,
    pub idle_threshold_secs: i64,
}
