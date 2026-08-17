use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRepository {
    pub id: String,
    pub project_id: String,
    pub repo_path: String,
    pub current_branch: Option<String>,
    pub remote_url: Option<String>,
    pub last_fetch_at: Option<i64>,
    pub has_uncommitted: bool,
    pub ahead_count: i64,
    pub behind_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFileStatus {
    pub id: String,
    pub repo_id: String,
    pub file_path: String,
    pub status: String,
    pub staged: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Register or update a Git repository
pub fn register_repository(
    conn: &Connection,
    project_id: &str,
    repo_path: &str,
) -> Result<GitRepository, AppError> {
    let now = chrono::Utc::now().timestamp();

    // Check if repository exists
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM git_repositories WHERE project_id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .ok();

    if let Some(repo_id) = existing {
        // Update existing
        conn.execute(
            "UPDATE git_repositories SET repo_path = ?1, updated_at = ?2 WHERE id = ?3",
            params![repo_path, now, repo_id],
        )?;
        get_repository(conn, project_id)
    } else {
        // Insert new
        let repo_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            r#"
            INSERT INTO git_repositories (
                id, project_id, repo_path, current_branch, remote_url,
                last_fetch_at, has_uncommitted, ahead_count, behind_count,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, NULL, NULL, NULL, 0, 0, 0, ?4, ?4)
            "#,
            params![repo_id, project_id, repo_path, now],
        )?;
        get_repository(conn, project_id)
    }
}

/// Get repository by project ID
pub fn get_repository(conn: &Connection, project_id: &str) -> Result<GitRepository, AppError> {
    let repo = conn.query_row(
        r#"
        SELECT
            id, project_id, repo_path, current_branch, remote_url,
            last_fetch_at, has_uncommitted, ahead_count, behind_count,
            created_at, updated_at
        FROM git_repositories
        WHERE project_id = ?1
        "#,
        params![project_id],
        |row| {
            Ok(GitRepository {
                id: row.get(0)?,
                project_id: row.get(1)?,
                repo_path: row.get(2)?,
                current_branch: row.get(3)?,
                remote_url: row.get(4)?,
                last_fetch_at: row.get(5)?,
                has_uncommitted: row.get::<_, i64>(6)? != 0,
                ahead_count: row.get(7)?,
                behind_count: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        },
    )?;

    Ok(repo)
}

/// Update repository status
pub fn update_repository_status(
    conn: &Connection,
    project_id: &str,
    current_branch: Option<&str>,
    remote_url: Option<&str>,
    has_uncommitted: bool,
    ahead_count: i64,
    behind_count: i64,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();

    let affected = conn.execute(
        r#"
        UPDATE git_repositories
        SET current_branch = ?1, remote_url = ?2, has_uncommitted = ?3,
            ahead_count = ?4, behind_count = ?5, updated_at = ?6
        WHERE project_id = ?7
        "#,
        params![
            current_branch,
            remote_url,
            if has_uncommitted { 1 } else { 0 },
            ahead_count,
            behind_count,
            now,
            project_id
        ],
    )?;

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Repository for project {} not found",
            project_id
        )));
    }

    Ok(())
}

/// Mark repository as fetched
pub fn mark_fetched(conn: &Connection, project_id: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();

    let affected = conn.execute(
        "UPDATE git_repositories SET last_fetch_at = ?1, updated_at = ?1 WHERE project_id = ?2",
        params![now, project_id],
    )?;

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Repository for project {} not found",
            project_id
        )));
    }

    Ok(())
}

/// Save file status
pub fn save_file_status(
    conn: &Connection,
    repo_id: &str,
    file_path: &str,
    status: &str,
    staged: bool,
) -> Result<GitFileStatus, AppError> {
    let now = chrono::Utc::now().timestamp();

    // Check if file status exists
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM git_file_status WHERE repo_id = ?1 AND file_path = ?2",
            params![repo_id, file_path],
            |row| row.get(0),
        )
        .ok();

    if let Some(status_id) = existing {
        // Update
        conn.execute(
            "UPDATE git_file_status SET status = ?1, staged = ?2, updated_at = ?3 WHERE id = ?4",
            params![status, if staged { 1 } else { 0 }, now, status_id],
        )?;
        get_file_status(conn, repo_id, file_path)
    } else {
        // Insert
        let status_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            r#"
            INSERT INTO git_file_status (id, repo_id, file_path, status, staged, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            "#,
            params![
                status_id,
                repo_id,
                file_path,
                status,
                if staged { 1 } else { 0 },
                now
            ],
        )?;
        get_file_status(conn, repo_id, file_path)
    }
}

/// Get file status
pub fn get_file_status(
    conn: &Connection,
    repo_id: &str,
    file_path: &str,
) -> Result<GitFileStatus, AppError> {
    let status = conn.query_row(
        r#"
        SELECT id, repo_id, file_path, status, staged, created_at, updated_at
        FROM git_file_status
        WHERE repo_id = ?1 AND file_path = ?2
        "#,
        params![repo_id, file_path],
        |row| {
            Ok(GitFileStatus {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                file_path: row.get(2)?,
                status: row.get(3)?,
                staged: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )?;

    Ok(status)
}

/// List all file statuses for a repository
pub fn list_file_statuses(
    conn: &Connection,
    repo_id: &str,
) -> Result<Vec<GitFileStatus>, AppError> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, repo_id, file_path, status, staged, created_at, updated_at
        FROM git_file_status
        WHERE repo_id = ?1
        ORDER BY file_path
        "#,
    )?;

    let statuses = stmt
        .query_map(params![repo_id], |row| {
            Ok(GitFileStatus {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                file_path: row.get(2)?,
                status: row.get(3)?,
                staged: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(statuses)
}

/// Clear all file statuses for a repository
pub fn clear_file_statuses(conn: &Connection, repo_id: &str) -> Result<usize, AppError> {
    let affected = conn.execute(
        "DELETE FROM git_file_status WHERE repo_id = ?1",
        params![repo_id],
    )?;
    Ok(affected)
}

/// Delete repository
pub fn delete_repository(conn: &Connection, project_id: &str) -> Result<(), AppError> {
    let affected = conn.execute(
        "DELETE FROM git_repositories WHERE project_id = ?1",
        params![project_id],
    )?;

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Repository for project {} not found",
            project_id
        )));
    }

    Ok(())
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

            CREATE TABLE git_repositories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
                repo_path TEXT NOT NULL,
                current_branch TEXT,
                remote_url TEXT,
                last_fetch_at INTEGER,
                has_uncommitted INTEGER NOT NULL DEFAULT 0,
                ahead_count INTEGER NOT NULL DEFAULT 0,
                behind_count INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(project_id)
            );

            CREATE TABLE git_file_status (
                id TEXT PRIMARY KEY,
                repo_id TEXT NOT NULL REFERENCES git_repositories(id) ON DELETE CASCADE,
                file_path TEXT NOT NULL,
                status TEXT NOT NULL CHECK(status IN ('untracked', 'modified', 'added', 'deleted', 'renamed', 'conflicted')),
                staged INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(repo_id, file_path)
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
    fn test_register_repository() {
        let conn = setup_test_db();

        let repo = register_repository(&conn, "proj1", "/path/to/repo").unwrap();

        assert_eq!(repo.project_id, "proj1");
        assert_eq!(repo.repo_path, "/path/to/repo");
        assert!(!repo.has_uncommitted);
        assert_eq!(repo.ahead_count, 0);
    }

    #[test]
    fn test_update_repository_status() {
        let conn = setup_test_db();

        register_repository(&conn, "proj1", "/path/to/repo").unwrap();

        update_repository_status(
            &conn,
            "proj1",
            Some("main"),
            Some("https://github.com/user/repo.git"),
            true,
            2,
            1,
        )
        .unwrap();

        let repo = get_repository(&conn, "proj1").unwrap();
        assert_eq!(repo.current_branch, Some("main".to_string()));
        assert!(repo.has_uncommitted);
        assert_eq!(repo.ahead_count, 2);
        assert_eq!(repo.behind_count, 1);
    }

    #[test]
    fn test_mark_fetched() {
        let conn = setup_test_db();

        register_repository(&conn, "proj1", "/path/to/repo").unwrap();

        mark_fetched(&conn, "proj1").unwrap();

        let repo = get_repository(&conn, "proj1").unwrap();
        assert!(repo.last_fetch_at.is_some());
    }

    #[test]
    fn test_save_file_status() {
        let conn = setup_test_db();

        let repo = register_repository(&conn, "proj1", "/path/to/repo").unwrap();

        let status = save_file_status(&conn, &repo.id, "src/main.rs", "modified", false).unwrap();

        assert_eq!(status.file_path, "src/main.rs");
        assert_eq!(status.status, "modified");
        assert!(!status.staged);
    }

    #[test]
    fn test_update_file_status() {
        let conn = setup_test_db();

        let repo = register_repository(&conn, "proj1", "/path/to/repo").unwrap();

        save_file_status(&conn, &repo.id, "src/main.rs", "modified", false).unwrap();
        save_file_status(&conn, &repo.id, "src/main.rs", "modified", true).unwrap();

        let status = get_file_status(&conn, &repo.id, "src/main.rs").unwrap();
        assert!(status.staged);
    }

    #[test]
    fn test_list_file_statuses() {
        let conn = setup_test_db();

        let repo = register_repository(&conn, "proj1", "/path/to/repo").unwrap();

        save_file_status(&conn, &repo.id, "file1.txt", "modified", false).unwrap();
        save_file_status(&conn, &repo.id, "file2.txt", "added", true).unwrap();

        let statuses = list_file_statuses(&conn, &repo.id).unwrap();
        assert_eq!(statuses.len(), 2);
    }

    #[test]
    fn test_clear_file_statuses() {
        let conn = setup_test_db();

        let repo = register_repository(&conn, "proj1", "/path/to/repo").unwrap();

        save_file_status(&conn, &repo.id, "file1.txt", "modified", false).unwrap();
        save_file_status(&conn, &repo.id, "file2.txt", "added", true).unwrap();

        let count = clear_file_statuses(&conn, &repo.id).unwrap();
        assert_eq!(count, 2);

        let statuses = list_file_statuses(&conn, &repo.id).unwrap();
        assert_eq!(statuses.len(), 0);
    }

    #[test]
    fn test_delete_repository() {
        let conn = setup_test_db();

        register_repository(&conn, "proj1", "/path/to/repo").unwrap();

        delete_repository(&conn, "proj1").unwrap();

        let result = get_repository(&conn, "proj1");
        assert!(result.is_err());
    }
}
