use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};

use crate::db::connection::now_unix;
use crate::db::models::{new_id, ProjectTask, ProjectTaskLink};

// ─── 任务 CRUD ───────────────────────────────────────────────

/// 创建项目任务。
pub fn create_task(
    conn: &Connection,
    project_id: &str,
    title: &str,
    description: Option<&str>,
    priority: &str,
) -> SqliteResult<ProjectTask> {
    let now = now_unix();
    let id = new_id();

    // 获取当前最大 sort_order
    let max_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) FROM project_tasks WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .unwrap_or(-1);

    conn.execute(
        "INSERT INTO project_tasks (
            id, project_id, title, description, status, priority,
            sort_order, due_date, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, 'todo', ?5, ?6, NULL, ?7, ?7)",
        params![id, project_id, title, description, priority, max_order + 1, now],
    )?;

    Ok(ProjectTask {
        id,
        project_id: project_id.to_string(),
        title: title.to_string(),
        description: description.map(|s| s.to_string()),
        status: "todo".to_string(),
        priority: priority.to_string(),
        sort_order: max_order + 1,
        due_date: None,
        created_at: now,
        updated_at: now,
        completed_at: None,
    })
}

/// 列出项目的所有任务（按 sort_order 排序）。
pub fn list_tasks(conn: &Connection, project_id: &str) -> SqliteResult<Vec<ProjectTask>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM project_tasks
         WHERE project_id = ?1
         ORDER BY sort_order ASC, created_at ASC",
    )?;
    let rows = stmt.query_map([project_id], task_from_row)?;
    rows.collect()
}

/// 更新任务字段（title/description/status/priority/due_date）。
pub fn update_task(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    description: Option<Option<&str>>,
    status: Option<&str>,
    priority: Option<&str>,
    due_date: Option<Option<i64>>,
) -> SqliteResult<Option<ProjectTask>> {
    let now = now_unix();

    // 动态构建 UPDATE
    let mut sets: Vec<String> = vec!["updated_at = ?".to_string()];
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];

    if let Some(t) = title {
        sets.push("title = ?".to_string());
        params_vec.push(Box::new(t.to_string()));
    }
    if let Some(d) = description {
        sets.push("description = ?".to_string());
        params_vec.push(Box::new(d.map(|s| s.to_string())));
    }
    if let Some(s) = status {
        sets.push("status = ?".to_string());
        params_vec.push(Box::new(s.to_string()));
        if s == "done" {
            sets.push("completed_at = ?".to_string());
            params_vec.push(Box::new(now));
        } else {
            sets.push("completed_at = NULL".to_string());
        }
    }
    if let Some(p) = priority {
        sets.push("priority = ?".to_string());
        params_vec.push(Box::new(p.to_string()));
    }
    if let Some(d) = due_date {
        sets.push("due_date = ?".to_string());
        params_vec.push(Box::new(d));
    }

    let sql = format!(
        "UPDATE project_tasks SET {} WHERE id = ?",
        sets.join(", ")
    );
    params_vec.push(Box::new(id.to_string()));

    let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let affected = conn.execute(&sql, param_refs.as_slice())?;

    if affected == 0 {
        return Ok(None);
    }

    conn.query_row("SELECT * FROM project_tasks WHERE id = ?1", [id], task_from_row)
        .optional()
}

/// 删除任务（级联删除关联）。
pub fn delete_task(conn: &Connection, id: &str) -> SqliteResult<()> {
    conn.execute("DELETE FROM project_tasks WHERE id = ?1", [id])?;
    Ok(())
}

/// 重排序任务。
pub fn reorder_tasks(conn: &Connection, task_ids: &[String]) -> SqliteResult<()> {
    let now = now_unix();
    for (i, tid) in task_ids.iter().enumerate() {
        conn.execute(
            "UPDATE project_tasks SET sort_order = ?2, updated_at = ?3 WHERE id = ?1",
            params![tid, i as i64, now],
        )?;
    }
    Ok(())
}

// ─── 任务-资源关联 ────────────────────────────────────────────

/// 关联任务与资源。
pub fn link_resource(
    conn: &Connection,
    task_id: &str,
    resource_id: &str,
    link_type: &str,
) -> SqliteResult<ProjectTaskLink> {
    let now = now_unix();
    conn.execute(
        "INSERT OR IGNORE INTO project_task_links (task_id, resource_id, link_type, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![task_id, resource_id, link_type, now],
    )?;
    Ok(ProjectTaskLink {
        task_id: task_id.to_string(),
        resource_id: resource_id.to_string(),
        link_type: link_type.to_string(),
        created_at: now,
    })
}

/// 取消关联。
pub fn unlink_resource(conn: &Connection, task_id: &str, resource_id: &str) -> SqliteResult<()> {
    conn.execute(
        "DELETE FROM project_task_links WHERE task_id = ?1 AND resource_id = ?2",
        params![task_id, resource_id],
    )?;
    Ok(())
}

/// 查询任务关联的资源 ID 列表。
pub fn list_task_links(conn: &Connection, task_id: &str) -> SqliteResult<Vec<ProjectTaskLink>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM project_task_links WHERE task_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([task_id], link_from_row)?;
    rows.collect()
}

/// 查询资源关联的任务 ID 列表（反向查询）。
pub fn list_links_by_resource(
    conn: &Connection,
    resource_id: &str,
) -> SqliteResult<Vec<ProjectTaskLink>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM project_task_links WHERE resource_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([resource_id], link_from_row)?;
    rows.collect()
}

// ─── 行映射 ──────────────────────────────────────────────────

fn task_from_row(row: &rusqlite::Row) -> SqliteResult<ProjectTask> {
    Ok(ProjectTask {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        title: row.get("title")?,
        description: row.get("description")?,
        status: row.get("status")?,
        priority: row.get("priority")?,
        sort_order: row.get("sort_order")?,
        due_date: row.get("due_date")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        completed_at: row.get("completed_at")?,
    })
}

fn link_from_row(row: &rusqlite::Row) -> SqliteResult<ProjectTaskLink> {
    Ok(ProjectTaskLink {
        task_id: row.get("task_id")?,
        resource_id: row.get("resource_id")?,
        link_type: row.get("link_type")?,
        created_at: row.get("created_at")?,
    })
}
