use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::error::AppError;

/// 打开数据库连接并应用运行时 PRAGMA。
pub fn open(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;

    // WAL：读写并发且提高写入吞吐，仅适用于本机文件系统
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // 外键约束必须由应用显式开启
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // NORMAL：WAL 下崩溃最多丢最近提交，不损坏数据库
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // 锁等待 5 秒，避免后台任务争用时报 SQLITE_BUSY
    conn.busy_timeout(Duration::from_secs(5))?;
    // 临时表和排序放内存，减少磁盘 I/O
    conn.pragma_update(None, "temp_store", "MEMORY")?;

    Ok(conn)
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
