use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};

use crate::db::connection::now_unix;
use crate::db::models::{Task, TaskItem};

/// 创建任务记录（queued 状态）。
pub fn create_task(
    conn: &Connection,
    task_type: &str,
    title: &str,
    total_count: Option<i64>,
    payload_json: Option<&str>,
) -> SqliteResult<Task> {
    let now = now_unix();
    let id = crate::db::models::new_id();
    conn.execute(
        "INSERT INTO tasks (
            id, task_type, status, title, total_count,
            completed_count, failed_count, payload_json,
            created_at, updated_at
         ) VALUES (?1, ?2, 'queued', ?3, ?4, 0, 0, ?5, ?6, ?6)",
        params![id, task_type, title, total_count, payload_json, now],
    )?;
    Ok(Task {
        id,
        task_type: task_type.to_string(),
        status: "queued".to_string(),
        title: title.to_string(),
        total_count,
        completed_count: 0,
        failed_count: 0,
        payload_json: payload_json.map(|s| s.to_string()),
        error_json: None,
        created_at: now,
        started_at: None,
        finished_at: None,
        updated_at: now,
    })
}

/// 按 ID 更新任务状态为 running。
pub fn mark_running(conn: &Connection, id: &str) -> SqliteResult<()> {
    let now = now_unix();
    conn.execute(
        "UPDATE tasks SET status = 'running', started_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

/// 更新进度计数。
pub fn update_progress(
    conn: &Connection,
    id: &str,
    completed: i64,
    failed: i64,
) -> SqliteResult<()> {
    conn.execute(
        "UPDATE tasks SET completed_count = ?2, failed_count = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, completed, failed, now_unix()],
    )?;
    Ok(())
}

/// 标记完成。
pub fn mark_completed(conn: &Connection, id: &str) -> SqliteResult<()> {
    let now = now_unix();
    conn.execute(
        "UPDATE tasks SET status = 'completed', finished_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

/// 标记失败并记录错误。
pub fn mark_failed(conn: &Connection, id: &str, error: &str) -> SqliteResult<()> {
    let now = now_unix();
    conn.execute(
        "UPDATE tasks SET status = 'failed', error_json = ?2, finished_at = ?3, updated_at = ?3
         WHERE id = ?1",
        params![id, serde_json::json!({ "message": error }).to_string(), now],
    )?;
    Ok(())
}

/// 标记取消。
pub fn mark_cancelled(conn: &Connection, id: &str) -> SqliteResult<()> {
    let now = now_unix();
    conn.execute(
        "UPDATE tasks SET status = 'cancelled', finished_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

/// 请求取消：由用户操作调用，后台线程在下一个检查点停止。
pub fn request_cancel(conn: &Connection, id: &str) -> SqliteResult<()> {
    let now = now_unix();
    conn.execute(
        "UPDATE tasks SET status = 'cancelled', updated_at = ?2
         WHERE id = ?1 AND status IN ('queued', 'running', 'paused')",
        params![id, now],
    )?;
    Ok(())
}

/// 查询任务当前状态。
pub fn get_task(conn: &Connection, id: &str) -> SqliteResult<Option<Task>> {
    conn.query_row("SELECT * FROM tasks WHERE id = ?1", [id], task_from_row)
        .optional()
}

/// 查询最近任务列表。
pub fn list_tasks(conn: &Connection, limit: i64) -> SqliteResult<Vec<Task>> {
    let mut stmt = conn.prepare("SELECT * FROM tasks ORDER BY created_at DESC LIMIT ?1")?;
    let rows = stmt.query_map([limit], task_from_row)?;
    rows.collect()
}

/// 是否已请求取消（status = 'cancelled'）。用于扫描阶段周期性短路。
pub fn is_cancelled(conn: &Connection, id: &str) -> SqliteResult<bool> {
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM tasks WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(status.as_deref() == Some("cancelled"))
}

/// 查询任务的全部单项结果（含失败清单）。
pub fn list_task_items(
    conn: &Connection,
    task_id: &str,
    limit: i64,
) -> SqliteResult<Vec<TaskItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, resource_id, source_path, status, error_message, updated_at
         FROM task_items WHERE task_id = ?1
         ORDER BY CASE WHEN status = 'failed' THEN 0 ELSE 1 END, updated_at ASC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![task_id, limit], |row| {
        Ok(TaskItem {
            id: row.get(0)?,
            task_id: row.get(1)?,
            resource_id: row.get(2)?,
            source_path: row.get(3)?,
            status: row.get(4)?,
            error_message: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })?;
    rows.collect()
}

fn task_from_row(row: &rusqlite::Row) -> SqliteResult<Task> {
    Ok(Task {
        id: row.get("id")?,
        task_type: row.get("task_type")?,
        status: row.get("status")?,
        title: row.get("title")?,
        total_count: row.get("total_count")?,
        completed_count: row.get("completed_count")?,
        failed_count: row.get("failed_count")?,
        payload_json: row.get("payload_json")?,
        error_json: row.get("error_json")?,
        created_at: row.get("created_at")?,
        started_at: row.get("started_at")?,
        finished_at: row.get("finished_at")?,
        updated_at: row.get("updated_at")?,
    })
}
