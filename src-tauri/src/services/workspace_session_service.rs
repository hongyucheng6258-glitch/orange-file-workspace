use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSession {
    pub id: String,
    pub project_id: String,
    pub open_files_json: String,
    pub active_file_id: Option<String>,
    pub terminal_tabs_json: String,
    pub active_terminal_index: Option<i64>,
    pub running_tasks_json: String,
    pub panel_layout_json: Option<String>,
    pub scroll_positions_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_restored_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSessionUpdate {
    pub open_files_json: Option<String>,
    pub active_file_id: Option<String>,
    pub terminal_tabs_json: Option<String>,
    pub active_terminal_index: Option<i64>,
    pub running_tasks_json: Option<String>,
    pub panel_layout_json: Option<String>,
    pub scroll_positions_json: Option<String>,
}

/// Save or update workspace session for a project
pub fn save_session(
    conn: &Connection,
    project_id: &str,
    update: &WorkspaceSessionUpdate,
) -> Result<WorkspaceSession, AppError> {
    let now = chrono::Utc::now().timestamp();

    // Check if session exists
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM workspace_sessions WHERE project_id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .ok();

    if let Some(session_id) = existing {
        // Update existing session
        let mut updates = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref open_files) = update.open_files_json {
            updates.push("open_files_json = ?");
            params_vec.push(Box::new(open_files.clone()));
        }
        if let Some(ref active_file) = update.active_file_id {
            updates.push("active_file_id = ?");
            params_vec.push(Box::new(active_file.clone()));
        }
        if let Some(ref terminal_tabs) = update.terminal_tabs_json {
            updates.push("terminal_tabs_json = ?");
            params_vec.push(Box::new(terminal_tabs.clone()));
        }
        if let Some(active_terminal) = update.active_terminal_index {
            updates.push("active_terminal_index = ?");
            params_vec.push(Box::new(active_terminal));
        }
        if let Some(ref running_tasks) = update.running_tasks_json {
            updates.push("running_tasks_json = ?");
            params_vec.push(Box::new(running_tasks.clone()));
        }
        if let Some(ref panel_layout) = update.panel_layout_json {
            updates.push("panel_layout_json = ?");
            params_vec.push(Box::new(panel_layout.clone()));
        }
        if let Some(ref scroll_positions) = update.scroll_positions_json {
            updates.push("scroll_positions_json = ?");
            params_vec.push(Box::new(scroll_positions.clone()));
        }

        updates.push("updated_at = ?");
        params_vec.push(Box::new(now));
        params_vec.push(Box::new(session_id.clone()));

        let sql = format!(
            "UPDATE workspace_sessions SET {} WHERE id = ?",
            updates.join(", ")
        );

        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, param_refs.as_slice())?;

        get_session(conn, project_id)
    } else {
        // Insert new session
        let session_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            r#"
            INSERT INTO workspace_sessions (
                id, project_id, open_files_json, active_file_id,
                terminal_tabs_json, active_terminal_index, running_tasks_json,
                panel_layout_json, scroll_positions_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
            "#,
            params![
                session_id,
                project_id,
                update.open_files_json.as_deref().unwrap_or("[]"),
                update.active_file_id,
                update.terminal_tabs_json.as_deref().unwrap_or("[]"),
                update.active_terminal_index,
                update.running_tasks_json.as_deref().unwrap_or("[]"),
                update.panel_layout_json,
                update.scroll_positions_json,
                now,
            ],
        )?;

        get_session(conn, project_id)
    }
}

/// Get workspace session for a project
pub fn get_session(conn: &Connection, project_id: &str) -> Result<WorkspaceSession, AppError> {
    let session = conn.query_row(
        r#"
        SELECT
            id, project_id, open_files_json, active_file_id,
            terminal_tabs_json, active_terminal_index, running_tasks_json,
            panel_layout_json, scroll_positions_json,
            created_at, updated_at, last_restored_at
        FROM workspace_sessions
        WHERE project_id = ?1
        "#,
        params![project_id],
        |row| {
            Ok(WorkspaceSession {
                id: row.get(0)?,
                project_id: row.get(1)?,
                open_files_json: row.get(2)?,
                active_file_id: row.get(3)?,
                terminal_tabs_json: row.get(4)?,
                active_terminal_index: row.get(5)?,
                running_tasks_json: row.get(6)?,
                panel_layout_json: row.get(7)?,
                scroll_positions_json: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                last_restored_at: row.get(11)?,
            })
        },
    )?;

    Ok(session)
}

/// Mark session as restored
pub fn mark_restored(conn: &Connection, project_id: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();
    let affected = conn.execute(
        "UPDATE workspace_sessions SET last_restored_at = ?1 WHERE project_id = ?2",
        params![now, project_id],
    )?;

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Session for project {} not found",
            project_id
        )));
    }

    Ok(())
}

/// Delete workspace session
pub fn delete_session(conn: &Connection, project_id: &str) -> Result<(), AppError> {
    let affected = conn.execute(
        "DELETE FROM workspace_sessions WHERE project_id = ?1",
        params![project_id],
    )?;

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Session for project {} not found",
            project_id
        )));
    }

    Ok(())
}

/// List all sessions
pub fn list_sessions(conn: &Connection) -> Result<Vec<WorkspaceSession>, AppError> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            id, project_id, open_files_json, active_file_id,
            terminal_tabs_json, active_terminal_index, running_tasks_json,
            panel_layout_json, scroll_positions_json,
            created_at, updated_at, last_restored_at
        FROM workspace_sessions
        ORDER BY updated_at DESC
        "#,
    )?;

    let sessions = stmt
        .query_map([], |row| {
            Ok(WorkspaceSession {
                id: row.get(0)?,
                project_id: row.get(1)?,
                open_files_json: row.get(2)?,
                active_file_id: row.get(3)?,
                terminal_tabs_json: row.get(4)?,
                active_terminal_index: row.get(5)?,
                running_tasks_json: row.get(6)?,
                panel_layout_json: row.get(7)?,
                scroll_positions_json: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                last_restored_at: row.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE resources (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE workspace_sessions (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
                open_files_json TEXT NOT NULL DEFAULT '[]',
                active_file_id TEXT,
                terminal_tabs_json TEXT NOT NULL DEFAULT '[]',
                active_terminal_index INTEGER,
                running_tasks_json TEXT NOT NULL DEFAULT '[]',
                panel_layout_json TEXT,
                scroll_positions_json TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                last_restored_at INTEGER,
                UNIQUE(project_id)
            );
            "#,
        )
        .unwrap();

        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO resources (id, name, resource_type, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["proj1", "Test Project", "project", now],
        )
        .unwrap();

        conn
    }

    #[test]
    fn test_save_new_session() {
        let conn = setup_test_db();

        let update = WorkspaceSessionUpdate {
            open_files_json: Some(r#"[{"id":"f1","path":"/test.txt"}]"#.to_string()),
            active_file_id: Some("f1".to_string()),
            terminal_tabs_json: Some(r#"[{"id":"t1","cwd":"/"}]"#.to_string()),
            active_terminal_index: Some(0),
            running_tasks_json: Some("[]".to_string()),
            panel_layout_json: None,
            scroll_positions_json: None,
        };

        let session = save_session(&conn, "proj1", &update).unwrap();

        assert_eq!(session.project_id, "proj1");
        assert_eq!(session.active_file_id, Some("f1".to_string()));
        assert_eq!(session.active_terminal_index, Some(0));
    }

    #[test]
    fn test_update_existing_session() {
        let conn = setup_test_db();

        // Create initial session
        let initial = WorkspaceSessionUpdate {
            open_files_json: Some("[]".to_string()),
            active_file_id: None,
            terminal_tabs_json: Some("[]".to_string()),
            active_terminal_index: None,
            running_tasks_json: Some("[]".to_string()),
            panel_layout_json: None,
            scroll_positions_json: None,
        };
        save_session(&conn, "proj1", &initial).unwrap();

        // Update with new data
        let update = WorkspaceSessionUpdate {
            open_files_json: Some(r#"[{"id":"f2"}]"#.to_string()),
            active_file_id: Some("f2".to_string()),
            terminal_tabs_json: None,
            active_terminal_index: None,
            running_tasks_json: None,
            panel_layout_json: None,
            scroll_positions_json: None,
        };

        let session = save_session(&conn, "proj1", &update).unwrap();
        assert_eq!(session.active_file_id, Some("f2".to_string()));
        assert!(session.open_files_json.contains("f2"));
    }

    #[test]
    fn test_get_session() {
        let conn = setup_test_db();

        let update = WorkspaceSessionUpdate {
            open_files_json: Some("[]".to_string()),
            active_file_id: Some("f1".to_string()),
            terminal_tabs_json: Some("[]".to_string()),
            active_terminal_index: None,
            running_tasks_json: Some("[]".to_string()),
            panel_layout_json: None,
            scroll_positions_json: None,
        };
        save_session(&conn, "proj1", &update).unwrap();

        let session = get_session(&conn, "proj1").unwrap();
        assert_eq!(session.project_id, "proj1");
        assert_eq!(session.active_file_id, Some("f1".to_string()));
    }

    #[test]
    fn test_mark_restored() {
        let conn = setup_test_db();

        let update = WorkspaceSessionUpdate {
            open_files_json: Some("[]".to_string()),
            active_file_id: None,
            terminal_tabs_json: Some("[]".to_string()),
            active_terminal_index: None,
            running_tasks_json: Some("[]".to_string()),
            panel_layout_json: None,
            scroll_positions_json: None,
        };
        save_session(&conn, "proj1", &update).unwrap();

        mark_restored(&conn, "proj1").unwrap();

        let session = get_session(&conn, "proj1").unwrap();
        assert!(session.last_restored_at.is_some());
    }

    #[test]
    fn test_delete_session() {
        let conn = setup_test_db();

        let update = WorkspaceSessionUpdate {
            open_files_json: Some("[]".to_string()),
            active_file_id: None,
            terminal_tabs_json: Some("[]".to_string()),
            active_terminal_index: None,
            running_tasks_json: Some("[]".to_string()),
            panel_layout_json: None,
            scroll_positions_json: None,
        };
        save_session(&conn, "proj1", &update).unwrap();

        delete_session(&conn, "proj1").unwrap();

        let result = get_session(&conn, "proj1");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_sessions() {
        let conn = setup_test_db();

        let update = WorkspaceSessionUpdate {
            open_files_json: Some("[]".to_string()),
            active_file_id: None,
            terminal_tabs_json: Some("[]".to_string()),
            active_terminal_index: None,
            running_tasks_json: Some("[]".to_string()),
            panel_layout_json: None,
            scroll_positions_json: None,
        };
        save_session(&conn, "proj1", &update).unwrap();

        let sessions = list_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project_id, "proj1");
    }
}
