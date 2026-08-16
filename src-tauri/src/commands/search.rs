use std::sync::MutexGuard;

use rusqlite::Connection;
use tauri::State;

use crate::ipc::CommandResult;
use crate::AppState;

fn lock_db<'a>(state: &'a AppState) -> MutexGuard<'a, Connection> {
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

/// 转义 LIKE 通配符，避免 `%`、`_`、`\` 被当作模式字符。
fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 搜索资源：按名称/路径模糊匹配，支持类型与收藏筛选，分页返回。
/// 空关键词直接返回空结果（不列出全部资源）。
pub fn execute_search(
    conn: &Connection,
    query: &str,
    kinds: Option<&[String]>,
    favorite_only: bool,
    limit: i64,
    offset: i64,
) -> rusqlite::Result<Vec<SearchHit>> {
    let q = query.trim();
    let lim = limit.clamp(1, 200);
    let off = offset.max(0);
    let fav = favorite_only as i64;

    if q.is_empty() {
        return Ok(Vec::new());
    }

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

    let escaped = escape_like(q);
    sql.push_str(
        " AND (r.name LIKE ?1 ESCAPE '\\' COLLATE NOCASE \
               OR l.path LIKE ?1 ESCAPE '\\' COLLATE NOCASE)",
    );
    params.push(Box::new(format!("%{escaped}%")));

    if let Some(kinds) = kinds {
        if !kinds.is_empty() {
            let placeholders: Vec<String> = kinds
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + params.len() + 1))
                .collect();
            sql.push_str(&format!(" AND r.kind IN ({})", placeholders.join(",")));
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

/// 搜索资源命令入口。
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
    execute_search(
        &conn,
        &query.unwrap_or_default(),
        kinds.as_deref(),
        favorite_only.unwrap_or(false),
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn test_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations::run_migrations(&mut conn).unwrap();
        seed(&conn);
        conn
    }

    fn seed(conn: &Connection) {
        let now = 1_700_000_000i64;
        conn.execute(
            "INSERT INTO resources (id, kind, name, parent_id, created_at, updated_at)
             VALUES ('r1', 'file', '年度报告_2026.pdf', NULL, ?1, ?1),
                    ('r2', 'folder', '设计素材', NULL, ?1, ?1),
                    ('r3', 'page', '项目复盘', NULL, ?1, ?1),
                    ('r4', 'file', '100%完成率.png', NULL, ?1, ?1),
                    ('r5', 'file', '资料_备份.zip', NULL, ?1, ?1)",
            [now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO resource_locations (id, resource_id, source_type, path, created_at)
             VALUES ('p1', 'r1', 'managed', 'D:\\docs\\年度报告_2026.pdf', ?1),
                    ('p2', 'r2', 'managed', 'E:\\素材\\设计素材', ?1),
                    ('p3', 'r3', 'managed', '/pages/项目复盘', ?1),
                    ('p4', 'r4', 'managed', 'C:\\img\\100%完成率.png', ?1),
                    ('p5', 'r5', 'managed', 'C:\\backup\\资料_备份.zip', ?1)",
            [now],
        )
        .unwrap();
    }

    #[test]
    fn empty_query_returns_nothing() {
        let conn = test_conn();
        assert!(execute_search(&conn, "", None, false, 50, 0)
            .unwrap()
            .is_empty());
        assert!(execute_search(&conn, "   ", None, false, 50, 0)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn matches_by_name_case_insensitive() {
        let conn = test_conn();
        let hits = execute_search(&conn, "年度报告", None, false, 50, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "r1");
    }

    #[test]
    fn matches_by_path() {
        let conn = test_conn();
        let hits = execute_search(&conn, "backup", None, false, 50, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "r5");
    }

    #[test]
    fn wildcard_chars_are_literal() {
        let conn = test_conn();
        // `%` 应被当作普通字符，而不是通配符：不能匹配所有资源。
        let hits = execute_search(&conn, "100%", None, false, 50, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "r4");
        // `_` 同样按字面匹配。
        let hits = execute_search(&conn, "完成率", None, false, 50, 0).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn kind_filter_works() {
        let conn = test_conn();
        let hits = execute_search(&conn, "项", Some(&["page".to_string()]), false, 50, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "page");
    }

    #[test]
    fn limit_and_offset_apply() {
        let conn = test_conn();
        let all = execute_search(&conn, "", None, false, 50, 0).unwrap();
        assert!(all.is_empty(), "空关键词不应返回任何结果");
        let hits = execute_search(&conn, "a", None, false, 1, 0).unwrap();
        assert!(hits.len() <= 1);
    }
}
