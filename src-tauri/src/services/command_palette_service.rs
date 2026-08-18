use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandHistory {
    pub id: String,
    pub command_id: String,
    pub command_label: String,
    pub command_category: String,
    pub execution_count: i64,
    pub last_executed_at: i64,
    pub created_at: i64,
}

/// Record command execution
pub fn record_command_execution(
    conn: &Connection,
    command_id: &str,
    command_label: &str,
    command_category: &str,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();

    // Check if command exists
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM command_history WHERE command_id = ?1",
            params![command_id],
            |row| row.get(0),
        )
        .ok();

    if let Some(history_id) = existing {
        // Update execution count and timestamp
        conn.execute(
            r#"
            UPDATE command_history
            SET execution_count = execution_count + 1,
                last_executed_at = ?1,
                command_label = ?2,
                command_category = ?3
            WHERE id = ?4
            "#,
            params![now, command_label, command_category, history_id],
        )?;
    } else {
        // Insert new record
        let history_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            r#"
            INSERT INTO command_history (
                id, command_id, command_label, command_category,
                execution_count, last_executed_at, created_at
            ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)
            "#,
            params![history_id, command_id, command_label, command_category, now],
        )?;
    }

    Ok(())
}

/// Get frequently used commands
pub fn get_frequent_commands(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<CommandHistory>, AppError> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            id, command_id, command_label, command_category,
            execution_count, last_executed_at, created_at
        FROM command_history
        ORDER BY execution_count DESC, last_executed_at DESC, rowid DESC
        LIMIT ?1
        "#,
    )?;

    let commands = stmt
        .query_map(params![limit as i64], |row| {
            Ok(CommandHistory {
                id: row.get(0)?,
                command_id: row.get(1)?,
                command_label: row.get(2)?,
                command_category: row.get(3)?,
                execution_count: row.get(4)?,
                last_executed_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(commands)
}

/// Get recent commands
pub fn get_recent_commands(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<CommandHistory>, AppError> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            id, command_id, command_label, command_category,
            execution_count, last_executed_at, created_at
        FROM command_history
        ORDER BY last_executed_at DESC, rowid DESC
        LIMIT ?1
        "#,
    )?;

    let commands = stmt
        .query_map(params![limit as i64], |row| {
            Ok(CommandHistory {
                id: row.get(0)?,
                command_id: row.get(1)?,
                command_label: row.get(2)?,
                command_category: row.get(3)?,
                execution_count: row.get(4)?,
                last_executed_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(commands)
}

/// Get commands by category
pub fn get_commands_by_category(
    conn: &Connection,
    category: &str,
    limit: usize,
) -> Result<Vec<CommandHistory>, AppError> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            id, command_id, command_label, command_category,
            execution_count, last_executed_at, created_at
        FROM command_history
        WHERE command_category = ?1
        ORDER BY execution_count DESC, last_executed_at DESC
        LIMIT ?2
        "#,
    )?;

    let commands = stmt
        .query_map(params![category, limit as i64], |row| {
            Ok(CommandHistory {
                id: row.get(0)?,
                command_id: row.get(1)?,
                command_label: row.get(2)?,
                command_category: row.get(3)?,
                execution_count: row.get(4)?,
                last_executed_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(commands)
}

/// Search commands by label
pub fn search_commands(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<CommandHistory>, AppError> {
    let pattern = format!("%{}%", query);

    let mut stmt = conn.prepare(
        r#"
        SELECT
            id, command_id, command_label, command_category,
            execution_count, last_executed_at, created_at
        FROM command_history
        WHERE command_label LIKE ?1 OR command_id LIKE ?1
        ORDER BY execution_count DESC, last_executed_at DESC
        LIMIT ?2
        "#,
    )?;

    let commands = stmt
        .query_map(params![pattern, limit as i64], |row| {
            Ok(CommandHistory {
                id: row.get(0)?,
                command_id: row.get(1)?,
                command_label: row.get(2)?,
                command_category: row.get(3)?,
                execution_count: row.get(4)?,
                last_executed_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(commands)
}

/// Clear command history
pub fn clear_command_history(conn: &Connection) -> Result<usize, AppError> {
    let affected = conn.execute("DELETE FROM command_history", [])?;
    Ok(affected)
}

/// Delete specific command from history
pub fn delete_command_history(conn: &Connection, command_id: &str) -> Result<(), AppError> {
    let affected = conn.execute(
        "DELETE FROM command_history WHERE command_id = ?1",
        params![command_id],
    )?;

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Command {} not found in history",
            command_id
        )));
    }

    Ok(())
}

/// Get command statistics
pub fn get_command_statistics(conn: &Connection) -> Result<CommandStatistics, AppError> {
    let total_commands: i64 =
        conn.query_row("SELECT COUNT(*) FROM command_history", [], |row| row.get(0))?;

    let total_executions: i64 = conn.query_row(
        "SELECT SUM(execution_count) FROM command_history",
        [],
        |row| row.get(0),
    )?;

    let most_used: Option<CommandHistory> = conn
        .query_row(
            r#"
            SELECT
                id, command_id, command_label, command_category,
                execution_count, last_executed_at, created_at
            FROM command_history
            ORDER BY execution_count DESC
            LIMIT 1
            "#,
            [],
            |row| {
                Ok(CommandHistory {
                    id: row.get(0)?,
                    command_id: row.get(1)?,
                    command_label: row.get(2)?,
                    command_category: row.get(3)?,
                    execution_count: row.get(4)?,
                    last_executed_at: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .ok();

    Ok(CommandStatistics {
        total_commands,
        total_executions,
        most_used,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandStatistics {
    pub total_commands: i64,
    pub total_executions: i64,
    pub most_used: Option<CommandHistory>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE command_history (
                id TEXT PRIMARY KEY,
                command_id TEXT NOT NULL,
                command_label TEXT NOT NULL,
                command_category TEXT NOT NULL,
                execution_count INTEGER NOT NULL DEFAULT 1,
                last_executed_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );
            "#,
        )
        .unwrap();

        conn
    }

    #[test]
    fn test_record_command_execution() {
        let conn = setup_test_db();

        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();
        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();

        let commands = get_frequent_commands(&conn, 10).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_id, "cmd.open");
        assert_eq!(commands[0].execution_count, 2);
    }

    #[test]
    fn test_get_frequent_commands() {
        let conn = setup_test_db();

        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();
        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();
        record_command_execution(&conn, "cmd.save", "Save File", "file").unwrap();

        let commands = get_frequent_commands(&conn, 10).unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].command_id, "cmd.open");
        assert_eq!(commands[0].execution_count, 2);
    }

    #[test]
    fn test_get_recent_commands() {
        let conn = setup_test_db();

        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        record_command_execution(&conn, "cmd.save", "Save File", "file").unwrap();

        let commands = get_recent_commands(&conn, 10).unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].command_id, "cmd.save"); // Most recent first
    }

    #[test]
    fn test_get_commands_by_category() {
        let conn = setup_test_db();

        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();
        record_command_execution(&conn, "cmd.search", "Search", "search").unwrap();

        let file_commands = get_commands_by_category(&conn, "file", 10).unwrap();
        assert_eq!(file_commands.len(), 1);
        assert_eq!(file_commands[0].command_category, "file");
    }

    #[test]
    fn test_search_commands() {
        let conn = setup_test_db();

        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();
        record_command_execution(&conn, "cmd.open.recent", "Open Recent File", "file").unwrap();
        record_command_execution(&conn, "cmd.save", "Save File", "file").unwrap();

        let results = search_commands(&conn, "open", 10).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_clear_command_history() {
        let conn = setup_test_db();

        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();
        record_command_execution(&conn, "cmd.save", "Save File", "file").unwrap();

        let count = clear_command_history(&conn).unwrap();
        assert_eq!(count, 2);

        let commands = get_frequent_commands(&conn, 10).unwrap();
        assert_eq!(commands.len(), 0);
    }

    #[test]
    fn test_delete_command_history() {
        let conn = setup_test_db();

        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();
        record_command_execution(&conn, "cmd.save", "Save File", "file").unwrap();

        delete_command_history(&conn, "cmd.open").unwrap();

        let commands = get_frequent_commands(&conn, 10).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_id, "cmd.save");
    }

    #[test]
    fn test_get_command_statistics() {
        let conn = setup_test_db();

        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();
        record_command_execution(&conn, "cmd.open", "Open File", "file").unwrap();
        record_command_execution(&conn, "cmd.save", "Save File", "file").unwrap();

        let stats = get_command_statistics(&conn).unwrap();
        assert_eq!(stats.total_commands, 2);
        assert_eq!(stats.total_executions, 3);
        assert!(stats.most_used.is_some());
        assert_eq!(stats.most_used.unwrap().command_id, "cmd.open");
    }
}
