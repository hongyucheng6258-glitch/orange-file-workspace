use rusqlite::{params, Connection, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentItem {
    pub id: String,
    pub resource_id: String,
    pub resource_type: String,
    pub access_count: i64,
    pub last_accessed_at: i64,
    pub created_at: i64,
    // Joined from resources table
    pub name: Option<String>,
    pub path: Option<String>,
}

/// Record access to a resource, incrementing count or creating new entry
pub fn record_access(
    conn: &Connection,
    resource_id: &str,
    resource_type: &str,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();

    // Verify resource exists
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM resources WHERE id = ?1)",
        params![resource_id],
        |row| row.get(0),
    )?;

    if !exists {
        return Err(AppError::NotFound(format!(
            "Resource {} not found",
            resource_id
        )));
    }

    // Upsert: increment if exists, insert if new
    conn.execute(
        r#"
        INSERT INTO recent_items (id, resource_id, resource_type, access_count, last_accessed_at, created_at)
        VALUES (?1, ?2, ?3, 1, ?4, ?4)
        ON CONFLICT(resource_id) DO UPDATE SET
            access_count = access_count + 1,
            last_accessed_at = ?4
        "#,
        params![uuid::Uuid::new_v4().to_string(), resource_id, resource_type, now],
    )?;

    Ok(())
}

/// Get recent items ordered by last access time
pub fn get_recent_items(
    conn: &Connection,
    resource_type_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<RecentItem>, AppError> {
    let mut sql = r#"
        SELECT
            ri.id, ri.resource_id, ri.resource_type, ri.access_count,
            ri.last_accessed_at, ri.created_at,
            r.name, r.path
        FROM recent_items ri
        LEFT JOIN resources r ON ri.resource_id = r.id
    "#
    .to_string();

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(type_filter) = resource_type_filter {
        sql.push_str(&format!(" WHERE ri.resource_type = ?{}", idx));
        params_vec.push(Box::new(type_filter));
        idx += 1;
    }

    sql.push_str(&format!(" ORDER BY ri.last_accessed_at DESC, ri.rowid DESC LIMIT ?{}", idx));
    let limit_i64 = limit as i64;
    params_vec.push(Box::new(limit_i64));

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
    let items = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(RecentItem {
                id: row.get(0)?,
                resource_id: row.get(1)?,
                resource_type: row.get(2)?,
                access_count: row.get(3)?,
                last_accessed_at: row.get(4)?,
                created_at: row.get(5)?,
                name: row.get(6)?,
                path: row.get(7)?,
            })
        })?
        .collect::<SqliteResult<Vec<_>>>()?;

    Ok(items)
}

/// Remove a specific recent item
pub fn remove_recent_item(conn: &Connection, resource_id: &str) -> Result<(), AppError> {
    let affected = conn.execute(
        "DELETE FROM recent_items WHERE resource_id = ?1",
        params![resource_id],
    )?;

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Recent item for resource {} not found",
            resource_id
        )));
    }

    Ok(())
}

/// Clear all recent items
pub fn clear_recent_items(conn: &Connection) -> Result<usize, AppError> {
    let affected = conn.execute("DELETE FROM recent_items", [])?;
    Ok(affected)
}

/// Remove items older than specified days
pub fn cleanup_old_items(conn: &Connection, older_than_days: i64) -> Result<usize, AppError> {
    let cutoff = chrono::Utc::now().timestamp() - (older_than_days * 24 * 60 * 60);
    let affected = conn.execute(
        "DELETE FROM recent_items WHERE last_accessed_at < ?1",
        params![cutoff],
    )?;
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();

        // Create tables
        conn.execute_batch(
            r#"
            CREATE TABLE resources (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE recent_items (
                id TEXT PRIMARY KEY,
                resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
                resource_type TEXT NOT NULL CHECK(resource_type IN ('file', 'folder', 'page', 'project')),
                access_count INTEGER NOT NULL DEFAULT 1,
                last_accessed_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                UNIQUE(resource_id)
            );
            "#,
        )
        .unwrap();

        // Insert test resources
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO resources (id, name, path, resource_type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["r1", "test.txt", "/path/test.txt", "file", now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO resources (id, name, path, resource_type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["r2", "project", "/path/project", "project", now],
        )
        .unwrap();

        conn
    }

    #[test]
    fn test_record_access() {
        let conn = setup_test_db();

        // First access
        record_access(&conn, "r1", "file").unwrap();

        let items = get_recent_items(&conn, None, 10).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].resource_id, "r1");
        assert_eq!(items[0].access_count, 1);

        // Second access
        std::thread::sleep(std::time::Duration::from_millis(10));
        record_access(&conn, "r1", "file").unwrap();

        let items = get_recent_items(&conn, None, 10).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].access_count, 2);
    }

    #[test]
    fn test_get_recent_items_with_filter() {
        let conn = setup_test_db();

        record_access(&conn, "r1", "file").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        record_access(&conn, "r2", "project").unwrap();

        // Get all
        let all = get_recent_items(&conn, None, 10).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].resource_id, "r2"); // Most recent first

        // Filter by type
        let files = get_recent_items(&conn, Some("file"), 10).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].resource_id, "r1");
    }

    #[test]
    fn test_remove_recent_item() {
        let conn = setup_test_db();
        record_access(&conn, "r1", "file").unwrap();

        remove_recent_item(&conn, "r1").unwrap();

        let items = get_recent_items(&conn, None, 10).unwrap();
        assert_eq!(items.len(), 0);
    }

    #[test]
    fn test_clear_recent_items() {
        let conn = setup_test_db();
        record_access(&conn, "r1", "file").unwrap();
        record_access(&conn, "r2", "project").unwrap();

        let count = clear_recent_items(&conn).unwrap();
        assert_eq!(count, 2);

        let items = get_recent_items(&conn, None, 10).unwrap();
        assert_eq!(items.len(), 0);
    }

    #[test]
    fn test_cleanup_old_items() {
        let conn = setup_test_db();

        // Insert old item manually
        let old_time = chrono::Utc::now().timestamp() - (31 * 24 * 60 * 60);
        conn.execute(
            "INSERT INTO recent_items (id, resource_id, resource_type, access_count, last_accessed_at, created_at) VALUES (?1, ?2, ?3, 1, ?4, ?4)",
            params![uuid::Uuid::new_v4().to_string(), "r1", "file", old_time],
        )
        .unwrap();

        record_access(&conn, "r2", "project").unwrap();

        let removed = cleanup_old_items(&conn, 30).unwrap();
        assert_eq!(removed, 1);

        let items = get_recent_items(&conn, None, 10).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].resource_id, "r2");
    }

    #[test]
    fn test_record_access_nonexistent_resource() {
        let conn = setup_test_db();
        let result = record_access(&conn, "nonexistent", "file");
        assert!(result.is_err());
    }
}
