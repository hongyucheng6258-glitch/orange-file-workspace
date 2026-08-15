use tauri::State;

use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::AppState;

fn lock_db<'a>(state: &'a AppState) -> std::sync::MutexGuard<'a, rusqlite::Connection> {
    state.conn.lock().expect("db lock poisoned")
}

fn count(conn: &rusqlite::Connection, sql: &str) -> Result<i64, AppError> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

/// 首页仪表盘数据：资源统计、存储占用与最近文件。
#[tauri::command]
pub fn dashboard_stats(state: State<AppState>) -> CommandResult<serde_json::Value> {
    let conn = lock_db(&state);

    let total_files = count(
        &conn,
        "SELECT COUNT(*) FROM resources WHERE kind = 'file' AND is_deleted = 0",
    )?;
    let total_folders = count(
        &conn,
        "SELECT COUNT(*) FROM resources WHERE kind = 'folder' AND is_deleted = 0",
    )?;
    let total_pages = count(
        &conn,
        "SELECT COUNT(*) FROM resources WHERE kind = 'page' AND is_deleted = 0",
    )?;
    let total_projects = count(
        &conn,
        "SELECT COUNT(*) FROM resources WHERE kind = 'project' AND is_deleted = 0",
    )?;
    let favorites = count(
        &conn,
        "SELECT COUNT(*) FROM resources WHERE is_favorite = 1 AND is_deleted = 0",
    )?;
    let trash = count(&conn, "SELECT COUNT(*) FROM resources WHERE is_deleted = 1")?;

    let total_size: i64 = conn.query_row(
        "SELECT COALESCE(SUM(rl.file_size), 0)
         FROM resource_locations rl
         JOIN resources r ON r.id = rl.resource_id
         WHERE r.is_deleted = 0 AND rl.file_size IS NOT NULL",
        [],
        |row| row.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT r.id, r.kind, r.name, r.parent_id, r.is_favorite, r.updated_at,
                rl.path, rl.source_type, rl.file_size
         FROM resources r
         LEFT JOIN resource_locations rl ON rl.resource_id = r.id
         WHERE r.is_deleted = 0
         ORDER BY r.updated_at DESC
         LIMIT 12",
    )?;
    let recent = stmt
        .query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "kind": row.get::<_, String>(1)?,
                "name": row.get::<_, String>(2)?,
                "parent_id": row.get::<_, Option<String>>(3)?,
                "is_favorite": row.get::<_, bool>(4)?,
                "updated_at": row.get::<_, i64>(5)?,
                "path": row.get::<_, Option<String>>(6)?,
                "source_type": row.get::<_, Option<String>>(7)?,
                "file_size": row.get::<_, Option<i64>>(8)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(serde_json::json!({
        "totalFiles": total_files,
        "totalFolders": total_folders,
        "totalPages": total_pages,
        "totalProjects": total_projects,
        "favorites": favorites,
        "trash": trash,
        "totalSize": total_size,
        "recent": recent,
    }))
}
