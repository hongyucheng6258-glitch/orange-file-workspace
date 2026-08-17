use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use crate::commands::search::SearchHit;
use crate::db::connection::now_unix;
use crate::db::models::{new_id, SavedSearch};

/// 保存搜索的筛选条件（与前端共享 JSON 格式）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilters {
    /// 资源类型：file/folder/page/project
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
    /// 仅收藏
    #[serde(default)]
    pub favorite_only: bool,
    /// 标签 ID 列表
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// 扩展名列表
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// 日期范围（Unix 时间戳）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_from: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_to: Option<i64>,
}

// ─── CRUD ───────────────────────────────────────────────────

pub fn create_saved_search(
    conn: &Connection,
    name: &str,
    query: Option<&str>,
    filters_json: &str,
    color: Option<&str>,
    icon: Option<&str>,
) -> SqliteResult<SavedSearch> {
    let now = now_unix();
    let id = new_id();

    let max_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(display_order), -1) FROM saved_searches",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1);

    conn.execute(
        "INSERT INTO saved_searches (
            id, name, query, filters_json, color, icon,
            is_pinned, display_order, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?8)",
        params![id, name, query, filters_json, color, icon, max_order + 1, now],
    )?;

    Ok(SavedSearch {
        id,
        name: name.to_string(),
        query: query.map(|s| s.to_string()),
        filters_json: filters_json.to_string(),
        color: color.map(|s| s.to_string()),
        icon: icon.map(|s| s.to_string()),
        is_pinned: false,
        display_order: max_order + 1,
        created_at: now,
        updated_at: now,
        last_executed_at: None,
    })
}

pub fn list_saved_searches(conn: &Connection) -> SqliteResult<Vec<SavedSearch>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM saved_searches ORDER BY is_pinned DESC, display_order ASC",
    )?;
    let rows = stmt.query_map([], saved_search_from_row)?;
    rows.collect()
}

pub fn list_pinned(conn: &Connection) -> SqliteResult<Vec<SavedSearch>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM saved_searches WHERE is_pinned = 1 ORDER BY display_order ASC",
    )?;
    let rows = stmt.query_map([], saved_search_from_row)?;
    rows.collect()
}

pub fn update_saved_search(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    query: Option<Option<&str>>,
    filters_json: Option<&str>,
    color: Option<Option<&str>>,
    icon: Option<Option<&str>>,
) -> SqliteResult<Option<SavedSearch>> {
    let now = now_unix();
    let mut sets: Vec<String> = vec!["updated_at = ?".to_string()];
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];

    if let Some(n) = name {
        sets.push("name = ?".to_string());
        params_vec.push(Box::new(n.to_string()));
    }
    if let Some(q) = query {
        sets.push("query = ?".to_string());
        params_vec.push(Box::new(q.map(|s| s.to_string())));
    }
    if let Some(f) = filters_json {
        sets.push("filters_json = ?".to_string());
        params_vec.push(Box::new(f.to_string()));
    }
    if let Some(c) = color {
        sets.push("color = ?".to_string());
        params_vec.push(Box::new(c.map(|s| s.to_string())));
    }
    if let Some(i) = icon {
        sets.push("icon = ?".to_string());
        params_vec.push(Box::new(i.map(|s| s.to_string())));
    }

    let sql = format!("UPDATE saved_searches SET {} WHERE id = ?", sets.join(", "));
    params_vec.push(Box::new(id.to_string()));

    let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let affected = conn.execute(&sql, param_refs.as_slice())?;

    if affected == 0 {
        return Ok(None);
    }

    conn.query_row("SELECT * FROM saved_searches WHERE id = ?1", [id], saved_search_from_row)
        .optional()
}

pub fn delete_saved_search(conn: &Connection, id: &str) -> SqliteResult<()> {
    conn.execute("DELETE FROM saved_searches WHERE id = ?1", [id])?;
    Ok(())
}

pub fn toggle_pinned(conn: &Connection, id: &str, pinned: bool) -> SqliteResult<Option<SavedSearch>> {
    let now = now_unix();
    let affected = conn.execute(
        "UPDATE saved_searches SET is_pinned = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, pinned as i32, now],
    )?;

    if affected == 0 {
        return Ok(None);
    }

    conn.query_row("SELECT * FROM saved_searches WHERE id = ?1", [id], saved_search_from_row)
        .optional()
}

pub fn reorder_pinned(conn: &Connection, ids: &[String]) -> SqliteResult<()> {
    let now = now_unix();
    for (i, id) in ids.iter().enumerate() {
        conn.execute(
            "UPDATE saved_searches SET display_order = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, i as i64, now],
        )?;
    }
    Ok(())
}

pub fn touch_last_executed(conn: &Connection, id: &str) -> SqliteResult<()> {
    let now = now_unix();
    conn.execute(
        "UPDATE saved_searches SET last_executed_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

// ─── 查询执行 ───────────────────────────────────────────────

/// 执行保存的搜索，返回匹配的资源。
/// 复用 `search.rs::execute_search` 的逻辑 + 扩展筛选条件。
pub fn execute_saved_search(
    conn: &Connection,
    id: &str,
    limit: i64,
) -> SqliteResult<Vec<SearchHit>> {
    let search = conn
        .query_row("SELECT * FROM saved_searches WHERE id = ?1", [id], saved_search_from_row)
        .optional()?
        .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

    let filters: SearchFilters = serde_json::from_str(&search.filters_json)
        .unwrap_or_default();

    // 构建基础查询 — 复用 execute_search 的 LIKE 逻辑
    let query = search.query.as_deref().unwrap_or("");
    let kinds_ref = if filters.kinds.is_empty() {
        None
    } else {
        Some(filters.kinds.as_slice())
    };

    // 调用现有 execute_search
    let mut hits = crate::commands::search::execute_search(
        conn,
        query,
        kinds_ref,
        filters.favorite_only,
        limit,
        0,
    )?;

    // 扩展筛选：标签（取交集）
    if !filters.tags.is_empty() {
        hits.retain(|hit| {
            for tag_id in &filters.tags {
                let ok: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM resource_tags WHERE resource_id = ?1 AND tag_id = ?2)",
                        params![hit.id, tag_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(false);
                if ok {
                    return true;
                }
            }
            false
        });
    }

    // 扩展筛选：扩展名
    if !filters.extensions.is_empty() {
        hits.retain(|hit| {
            let ext: Option<String> = conn
                .query_row(
                    "SELECT extension FROM file_metadata WHERE resource_id = ?1",
                    [hit.id.clone()],
                    |row| row.get(0),
                )
                .optional()
                .ok()
                .flatten();
            match &ext {
                Some(e) => filters.extensions.iter().any(|f| {
                    f.eq_ignore_ascii_case(e) || e.eq_ignore_ascii_case(f)
                }),
                None => false,
            }
        });
    }

    // 扩展筛选：日期范围
    if let Some(from) = filters.date_from {
        hits.retain(|hit| hit.updated_at >= from);
    }
    if let Some(to) = filters.date_to {
        hits.retain(|hit| hit.updated_at <= to);
    }

    // 更新最后执行时间
    let _ = touch_last_executed(conn, id);

    Ok(hits)
}

// ─── 行映射 ──────────────────────────────────────────────────

fn saved_search_from_row(row: &rusqlite::Row) -> SqliteResult<SavedSearch> {
    Ok(SavedSearch {
        id: row.get("id")?,
        name: row.get("name")?,
        query: row.get("query")?,
        filters_json: row.get("filters_json")?,
        color: row.get("color")?,
        icon: row.get("icon")?,
        is_pinned: row.get::<_, i32>("is_pinned")? != 0,
        display_order: row.get("display_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        last_executed_at: row.get("last_executed_at")?,
    })
}
