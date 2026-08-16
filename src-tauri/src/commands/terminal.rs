//! 内置终端 Tauri 命令：spawn / write / resize / close / list / list_shells / history。

use std::path::PathBuf;

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::ipc::CommandResult;
use crate::services::terminal_history_service::{
    clear_history, list_history, record_command, TerminalHistoryEntry,
};
use crate::services::terminal_service::{
    list_sessions, list_shells, ShellInfo, TerminalEvent, TerminalSessionInfo,
};
use crate::AppState;

/// 启动终端会话。`shell` 接受 powershell/cmd/gitbash/wsl；`cwd` 不存在时回退用户主目录。
#[tauri::command]
pub fn terminal_spawn(
    app: AppHandle,
    shell: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    channel: Channel<TerminalEvent>,
) -> CommandResult<TerminalSessionInfo> {
    let cwd_path = cwd.map(PathBuf::from);
    crate::services::terminal_service::spawn_session(
        &app,
        &shell,
        cwd_path,
        cols.max(20),
        rows.max(5),
        channel,
    )
}

/// 探测本机可用 Shell（内置 + Git Bash/WSL），供终端页下拉展示。
#[tauri::command]
pub fn terminal_list_shells() -> CommandResult<Vec<ShellInfo>> {
    Ok(list_shells())
}

/// 向会话写入输入（UTF-8）。
#[tauri::command]
pub fn terminal_write(state: State<AppState>, session_id: u64, data: String) -> CommandResult<()> {
    crate::services::terminal_service::write_session(&state.terminal, session_id, &data)
}

/// 调整会话窗口大小。
#[tauri::command]
pub fn terminal_resize(
    state: State<AppState>,
    session_id: u64,
    cols: u16,
    rows: u16,
) -> CommandResult<()> {
    crate::services::terminal_service::resize_session(
        &state.terminal,
        session_id,
        cols.max(20),
        rows.max(5),
    )
}

/// 结束会话（幂等）。
#[tauri::command]
pub fn terminal_close(state: State<AppState>, session_id: u64) -> CommandResult<()> {
    crate::services::terminal_service::close_session(&state.terminal, session_id)
}

/// 列出存活会话。
#[tauri::command]
pub fn terminal_list(state: State<AppState>) -> CommandResult<Vec<TerminalSessionInfo>> {
    Ok(list_sessions(&state.terminal))
}

/// 记录一条终端命令历史（空命令忽略，连续重复去重）。
#[tauri::command]
pub fn terminal_history_record(
    state: State<AppState>,
    shell: String,
    command: String,
    cwd: Option<String>,
) -> CommandResult<()> {
    let conn = state.conn.lock().expect("db lock poisoned");
    record_command(&conn, &shell, &command, cwd.as_deref().unwrap_or(""))
        .map_err(crate::error::AppError::from)?;
    Ok(())
}

/// 查询某 Shell 的命令历史（按时间倒序，最多 limit 条）。
#[tauri::command]
pub fn terminal_history_list(
    state: State<AppState>,
    shell: String,
    limit: Option<usize>,
) -> CommandResult<Vec<TerminalHistoryEntry>> {
    let conn = state.conn.lock().expect("db lock poisoned");
    let entries = list_history(&conn, &shell, limit.unwrap_or(100))
        .map_err(crate::error::AppError::from)?;
    Ok(entries)
}

/// 清空命令历史；shell 为 None 时清空全部。
#[tauri::command]
pub fn terminal_history_clear(
    state: State<AppState>,
    shell: Option<String>,
) -> CommandResult<()> {
    let conn = state.conn.lock().expect("db lock poisoned");
    clear_history(&conn, shell.as_deref()).map_err(crate::error::AppError::from)?;
    Ok(())
}
