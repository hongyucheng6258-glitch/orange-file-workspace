use std::sync::MutexGuard;

use tauri::State;

use crate::AppState;
use crate::ipc::CommandResult;

fn lock_db<'a>(state: &'a AppState) -> MutexGuard<'a, rusqlite::Connection> {
    state.conn.lock().expect("db lock poisoned")
}

/// 搜索结果条目：资源 + 第一个位置路径。
#[derive(serde::Serialize)]
pub struct SearchHit {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub is_favorite: bool,
    pub updated_at: i64,
    pub path: Option<String>,
    pub source_type: Option<String>,
}

/// 搜索资源：按名称/路径模糊匹配，支持类型与收藏筛选，分页返回。
#[tauri::command]
pub fn search_resources(
    state: State<AppState>,
    query: Option<String>,
    kinds: Option<Vec<String>>,
    favorite_only: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> CommandResult<Vec<SearchHit>> {
    let conn = lock_db(&state);
    let q = query.unwrap_or_default();
    let lim = limit.unwrap_or(50).clamp(1, 200);
    let off = offset.unwrap_or(0).max(0);
    let fav = favorite_only.unwrap_or(false) as i64;

    let mut sql = String::from(
        "SELECT r.id, r.kind, r.name, r.parent_id, r.is_favorite, r.updated_at,
                l.path, l.source_type
         FROM resources r
         LEFT JOIN resource_locations l
           ON l.id = (SELECT id FROM resource_locations
                      WHERE resource_id = r.id LIMIT 1)
         WHERE r.is_deleted = 0",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if !q.trim().is_empty() {
        sql.push_str(" AND (r.name LIKE ?1 COLLATE NOCASE OR l.path LIKE ?1 COLLATE NOCASE)");
        params.push(Box::new(format!("%{}%", q.trim())));
    }
    if let Some(kinds) = &kinds {
        if !kinds.is_empty() {
            let placeholders: Vec<String> = kinds
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + params.len() + 1))
                .collect();
            sql.push_str(&format!(
                " AND r.kind IN ({})",
                placeholders.join(",")
            ));
            for k in kinds {
                params.push(Box::new(k.clone()));
            }
        }
    }
    if fav == 1 {
        sql.push_str(" AND r.is_favorite = 1");
    }
    sql.push_str(&format!(
        " ORDER BY r.updated_at DESC LIMIT {lim} OFFSET {off}"
    ));

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|b| b.as_ref() as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(SearchHit {
            id: row.get("id")?,
            kind: row.get("kind")?,
            name: row.get("name")?,
            parent_id: row.get("parent_id")?,
            is_favorite: row.get::<_, i64>("is_favorite")? != 0,
            updated_at: row.get("updated_at")?,
            path: row.get("path")?,
            source_type: row.get("source_type")?,
        })
    })?;

    let mut hits = Vec::new();
    for row in rows {
        hits.push(row?);
    }
    Ok(hits)
}
