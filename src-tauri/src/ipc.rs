use crate::error::AppError;

/// 所有 Tauri 命令的返回值。错误会序列化为 `{ code, message }`。
pub type CommandResult<T> = Result<T, AppError>;
