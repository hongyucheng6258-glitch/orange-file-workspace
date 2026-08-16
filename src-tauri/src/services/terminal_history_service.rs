//! 终端命令历史存储：按 Shell 持久化用户执行过的命令。
//! 记录用于方向键/历史面板恢复，跨应用重启保留。

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppError;

/// 单条历史记录。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TerminalHistoryEntry {
    pub id: i64,
    pub shell: String,
    pub command: String,
    pub cwd: String,
    pub created_at: i64,
}

/// 保存一条命令历史。空命令忽略；与上一条相同则只更新时间（避免连续重复刷屏）。
pub fn record_command(
    conn: &Connection,
    shell: &str,
    command: &str,
    cwd: &str,
) -> Result<(), AppError> {
    let cmd = command.trim();
    if cmd.is_empty() {
        return Ok(());
    }
    // 与最近一条相同则仅刷新时间。
    let last: Option<String> = conn
        .query_row(
            "SELECT command FROM terminal_history WHERE shell = ?1 ORDER BY created_at DESC, id DESC LIMIT 1",
            [shell],
            |r| r.get(0),
        )
        .optional()?;
    if last.as_deref() == Some(cmd) {
        conn.execute(
            "UPDATE terminal_history SET created_at = ?1
             WHERE id = (SELECT id FROM terminal_history WHERE shell = ?2
                         ORDER BY created_at DESC, id DESC LIMIT 1)",
            params![crate::db::connection::now_unix(), shell],
        )?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO terminal_history (shell, command, cwd, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![shell, cmd, cwd, crate::db::connection::now_unix()],
    )?;
    Ok(())
}

/// 查询某 Shell 的历史，按时间倒序返回最近 `limit` 条。
pub fn list_history(
    conn: &Connection,
    shell: &str,
    limit: usize,
) -> Result<Vec<TerminalHistoryEntry>, AppError> {
    let limit = limit.clamp(1, 500) as i64;
    let mut stmt = conn.prepare(
        "SELECT id, shell, command, cwd, created_at
         FROM terminal_history
         WHERE shell = ?1
         ORDER BY created_at DESC, id DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![shell, limit], |row| {
        Ok(TerminalHistoryEntry {
            id: row.get(0)?,
            shell: row.get(1)?,
            command: row.get(2)?,
            cwd: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 清空某 Shell 的历史；shell 为 None 时清空全部。
pub fn clear_history(conn: &Connection, shell: Option<&str>) -> Result<(), AppError> {
    match shell {
        Some(s) => conn.execute("DELETE FROM terminal_history WHERE shell = ?1", [s])?,
        None => conn.execute("DELETE FROM terminal_history", [])?,
    };
    Ok(())
}

/// 统计某 Shell 的历史条数（测试辅助）。
#[cfg(test)]
pub fn count_history(conn: &Connection, shell: &str) -> Result<i64, AppError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM terminal_history WHERE shell = ?1",
        [shell],
        |r| r.get(0),
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open memory db");
        conn.execute_batch(
            "CREATE TABLE terminal_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                shell TEXT NOT NULL,
                command TEXT NOT NULL,
                cwd TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL
            );
            CREATE INDEX idx_terminal_history_shell_time
                ON terminal_history (shell, created_at DESC);",
        )
        .expect("create table");
        conn
    }

    #[test]
    fn record_and_list_roundtrip() {
        let conn = conn();
        record_command(&conn, "powershell", "Get-ChildItem", "C:\\").unwrap();
        record_command(&conn, "powershell", "git status", "C:\\proj").unwrap();

        let list = list_history(&conn, "powershell", 10).unwrap();
        assert_eq!(list.len(), 2);
        // 按时间倒序：后插入的在前。
        assert_eq!(list[0].command, "git status");
        assert_eq!(list[0].shell, "powershell");
        assert_eq!(list[0].cwd, "C:\\proj");
        assert_eq!(list[1].command, "Get-ChildItem");
    }

    #[test]
    fn empty_command_is_ignored() {
        let conn = conn();
        record_command(&conn, "powershell", "   ", "C:\\").unwrap();
        assert_eq!(count_history(&conn, "powershell").unwrap(), 0);
    }

    #[test]
    fn consecutive_duplicate_is_deduped() {
        let conn = conn();
        record_command(&conn, "powershell", "dir", "C:\\").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        record_command(&conn, "powershell", "dir", "C:\\").unwrap();
        assert_eq!(count_history(&conn, "powershell").unwrap(), 1);
    }

    #[test]
    fn history_is_filtered_by_shell() {
        let conn = conn();
        record_command(&conn, "powershell", "Get-ChildItem", "C:\\").unwrap();
        record_command(&conn, "cmd", "dir", "C:\\").unwrap();
        record_command(&conn, "gitbash", "ls", "C:\\").unwrap();

        let ps = list_history(&conn, "powershell", 10).unwrap();
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].command, "Get-ChildItem");
    }

    #[test]
    fn list_respects_limit() {
        let conn = conn();
        for i in 0..5 {
            record_command(&conn, "powershell", &format!("cmd-{i}"), "C:\\").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let list = list_history(&conn, "powershell", 3).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].command, "cmd-4");
    }

    #[test]
    fn clear_removes_shell_or_all() {
        let conn = conn();
        record_command(&conn, "powershell", "a", "C:\\").unwrap();
        record_command(&conn, "cmd", "b", "C:\\").unwrap();

        clear_history(&conn, Some("powershell")).unwrap();
        assert_eq!(count_history(&conn, "powershell").unwrap(), 0);
        assert_eq!(count_history(&conn, "cmd").unwrap(), 1);

        clear_history(&conn, None).unwrap();
        assert_eq!(count_history(&conn, "cmd").unwrap(), 0);
    }
}
