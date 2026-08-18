use std::path::Path;

use rusqlite::Connection;

use crate::error::AppError;

/// 打开数据库连接。
pub fn open(path: &Path) -> Result<Connection, AppError> {
    Connection::open(path).map_err(|e| AppError {
        code: "db_error".into(),
        message: format!("unable to open database file: {e}"),
    })
}

/// 打开内存数据库（用于测试），应用与生产相同的 PRAGMA。
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(conn)
}

/// 当前 UNIX 时间戳（秒），作为所有时间字段的统一存储格式。
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
