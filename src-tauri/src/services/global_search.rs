use std::collections::HashMap;

use rusqlite::Connection;

/// 全局搜索结果条目（统一模型，供前端序列化）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct GlobalSearchHit {
    pub key: String,
    pub kind: String, // file | folder | app | page | project
    pub name: String,
    pub path: Option<String>,
    pub source: String, // windows | local_index | app_index | nexus
    pub matched_field: String,
    pub modified_at: Option<i64>,
    pub icon_source: Option<String>,
    pub is_offline: bool,
    pub actions: Vec<String>,
    pub score: i64,
}

/// 规范化搜索去重键：统一分隔符并小写，用于大小写无关去重。
pub fn canonical_key(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

/// 转义 LIKE 通配符（与 commands/search.rs 同规则）。
pub fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 同一 key 只保留一条：优先高分，其次更权威来源。
/// 保留各 key 首次出现的顺序（稳定），供后续排序使用。
/// 前置条件：调用方构造 hit 时必须已用 `canonical_key` 归一化 key，
/// 否则大小写/分隔符不同的重复项不会被合并。
pub fn dedup_hits(hits: &mut Vec<GlobalSearchHit>) {
    let mut best: HashMap<String, usize> = HashMap::new();
    let mut kept: Vec<GlobalSearchHit> = Vec::new();
    for h in hits.drain(..) {
        match best.get(&h.key) {
            Some(&idx) => {
                let prev = &mut kept[idx];
                let prev_authority = authority_rank(&prev.source);
                let cur_authority = authority_rank(&h.source);
                if h.score > prev.score
                    || (h.score == prev.score && cur_authority > prev_authority)
                {
                    *prev = h;
                }
            }
            None => {
                best.insert(h.key.clone(), kept.len());
                kept.push(h);
            }
        }
    }
    *hits = kept;
}

fn authority_rank(source: &str) -> i64 {
    match source {
        "windows" => 4,
        "local_index" => 3,
        "nexus" => 2,
        "app_index" => 1,
        _ => 0,
    }
}

/// 按评分降序排序；同分按名称字节序升序（tie-break）。
pub fn sort_hits(hits: &mut Vec<GlobalSearchHit>) {
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
}

/// 探测并确保 FTS5 trigram 索引可用；不可用时静默返回 false（查询走 LIKE）。
/// 运行时探测避免迁移阶段依赖具体 tokenizer。
pub fn ensure_fts(conn: &Connection) -> bool {
    let probe = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='system_search_entries_fts'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if probe > 0 {
        return true;
    }
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS system_search_entries_fts
         USING fts5(display_name, canonical_path, content='system_search_entries', content_rowid='id', tokenize='trigram')",
    )
    .is_ok()
}

/// 将一条索引记录写入 FTS（外部内容表需手动同步）。
/// FTS5 虚拟表不支持 upsert，用 INSERT OR REPLACE 实现幂等写入。
pub fn fts_sync_upsert(conn: &Connection, id: i64, display_name: &str, canonical_path: &str) {
    if ensure_fts(conn) {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO system_search_entries_fts(rowid, display_name, canonical_path)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![id, display_name, canonical_path],
        );
    }
}

/// 从 FTS 删除一条记录（记录删除/重命名时配套调用，防止残留行累积）。
pub fn fts_sync_delete(conn: &Connection, id: i64) {
    if ensure_fts(conn) {
        let _ = conn.execute(
            "DELETE FROM system_search_entries_fts WHERE rowid = ?1",
            [id],
        );
    }
}

/// 将 system_search_entries 一行映射为 GlobalSearchHit（FTS / LIKE 两分支共用）。
fn row_to_hit(row: &rusqlite::Row<'_>, q: &str) -> rusqlite::Result<GlobalSearchHit> {
    let name: String = row.get("display_name")?;
    let path: String = row.get("canonical_path")?;
    let kind: String = row.get("entry_kind")?;
    let ql = q.to_lowercase();
    let nl = name.to_lowercase();
    let score = if nl == ql {
        200
    } else if nl.starts_with(&ql) {
        160
    } else {
        120
    };
    Ok(GlobalSearchHit {
        key: canonical_key(&path),
        kind: kind.clone(),
        name,
        path: Some(path),
        source: "local_index".to_string(),
        matched_field: if nl.contains(&ql) { "name".into() } else { "path".into() },
        modified_at: row.get("modified_at")?,
        icon_source: None,
        is_offline: row.get::<_, i64>("is_offline")? != 0,
        actions: vec!["open".to_string(), "reveal".to_string()],
        score,
    })
}

/// 查询本地系统搜索索引（FTS 优先，LIKE 兜底），返回可直接合并的命中。
/// FTS 分支无结果时降级 LIKE：外部内容表需手动同步，扫描未完成时 FTS 索引可能为空。
pub fn query_local_index(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> rusqlite::Result<Vec<GlobalSearchHit>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let lim = limit.clamp(1, 500);
    let fts = ensure_fts(conn);
    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if fts && q.chars().count() >= 3 {
        (
            "SELECT e.id, e.canonical_path, e.display_name, e.entry_kind, e.modified_at, e.is_offline
             FROM system_search_entries e
             JOIN system_search_entries_fts f ON f.rowid = e.id
             WHERE system_search_entries_fts MATCH ?1
             ORDER BY f.rank LIMIT ?2".to_string(),
            vec![Box::new(q.to_string()), Box::new(lim)],
        )
    } else {
        let like = format!("%{}%", escape_like(q));
        (
            "SELECT id, canonical_path, display_name, entry_kind, modified_at, is_offline
             FROM system_search_entries
             WHERE display_name LIKE ?1 ESCAPE '\\' COLLATE NOCASE
                OR canonical_path LIKE ?1 ESCAPE '\\' COLLATE NOCASE
             ORDER BY
               CASE WHEN display_name LIKE ?2 ESCAPE '\\' COLLATE NOCASE THEN 100
                    WHEN display_name LIKE ?3 ESCAPE '\\' COLLATE NOCASE THEN 60
                    ELSE 30 END DESC
             LIMIT ?4".to_string(),
            vec![
                Box::new(like.clone()),
                Box::new(format!("{}%", escape_like(q))),
                Box::new(like),
                Box::new(lim),
            ],
        )
    };
    let mut hits = if fts && q.chars().count() >= 3 {
        // FTS 查询报错（trigram 不支持的特殊字符语法）时降级，不向上抛
        run_local_query(conn, &sql, &params, q).unwrap_or_default()
    } else {
        run_local_query(conn, &sql, &params, q)?
    };
    // FTS 分支（可触发但索引未同步）空结果 → 降级 LIKE，保证基础可用
    if hits.is_empty() && fts && q.chars().count() >= 3 {
        let like = format!("%{}%", escape_like(q));
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = (
            "SELECT id, canonical_path, display_name, entry_kind, modified_at, is_offline
             FROM system_search_entries
             WHERE display_name LIKE ?1 ESCAPE '\\' COLLATE NOCASE
                OR canonical_path LIKE ?1 ESCAPE '\\' COLLATE NOCASE
             ORDER BY
               CASE WHEN display_name LIKE ?2 ESCAPE '\\' COLLATE NOCASE THEN 100
                    WHEN display_name LIKE ?3 ESCAPE '\\' COLLATE NOCASE THEN 60
                    ELSE 30 END DESC
             LIMIT ?4".to_string(),
            vec![
                Box::new(like.clone()),
                Box::new(format!("{}%", escape_like(q))),
                Box::new(like),
                Box::new(lim),
            ],
        );
        hits = run_local_query(conn, &sql, &params, q)?;
    }
    Ok(hits)
}

/// 执行本地索引查询并解析命中（供 query_local_index 复用）。
fn run_local_query(
    conn: &Connection,
    sql: &str,
    params: &[Box<dyn rusqlite::types::ToSql>],
    q: &str,
) -> rusqlite::Result<Vec<GlobalSearchHit>> {
    let mut stmt = conn.prepare(sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|b| b.as_ref() as &dyn rusqlite::types::ToSql).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| row_to_hit(row, q))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 合并 NexusFile 业务资源结果（复用现有 search_resources 逻辑）。
pub fn merge_nexus_hits(
    hits: &mut Vec<GlobalSearchHit>,
    nexus: Vec<crate::commands::search::SearchHit>,
) {
    for h in nexus {
        let path = h.path.clone().unwrap_or_default();
        let key = if path.is_empty() {
            format!("nexus:{}", h.id)
        } else {
            canonical_key(&path)
        };
        hits.push(GlobalSearchHit {
            key,
            kind: h.kind,
            name: h.name,
            path: h.path,
            source: "nexus".to_string(),
            matched_field: "name".to_string(),
            modified_at: Some(h.updated_at),
            icon_source: None,
            is_offline: false,
            actions: vec!["open".to_string()],
            score: 90,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(key: &str, kind: &str, name: &str, score: i64, source: &str) -> GlobalSearchHit {
        GlobalSearchHit {
            key: key.to_string(),
            kind: kind.to_string(),
            name: name.to_string(),
            path: Some(format!("C:\\tmp\\{name}")),
            source: source.to_string(),
            matched_field: "name".to_string(),
            modified_at: Some(1_700_000_000),
            icon_source: None,
            is_offline: false,
            actions: vec!["open".to_string()],
            score,
        }
    }

    #[test]
    fn canonical_path_is_case_insensitive() {
        assert_eq!(canonical_key("C:\\Users\\A\\File.Txt"), canonical_key("c:\\users\\a\\file.txt"));
        assert_eq!(canonical_key("D:/Docs/报告.pdf"), canonical_key("d:\\docs\\报告.pdf"));
    }

    #[test]
    fn dedup_keeps_highest_score() {
        let mut hits = vec![
            hit("C:\\a\\b.txt", "file", "b.txt", 50, "windows"),
            hit("C:\\a\\b.txt", "file", "b.txt", 80, "local_index"),
        ];
        dedup_hits(&mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "local_index");
    }

    #[test]
    fn sort_orders_name_prefix_over_contains() {
        let mut hits = vec![
            hit("k1", "file", "report.pdf", 2, "local_index"),
            hit("k2", "file", "my_report.pdf", 1, "local_index"),
        ];
        sort_hits(&mut hits);
        assert_eq!(hits[0].key, "k1");
    }

    #[test]
    fn sort_same_score_orders_by_name() {
        // 同分时按名称升序（tie-break 分支）
        let mut hits = vec![
            hit("k2", "file", "my_report.pdf", 2, "local_index"),
            hit("k1", "file", "report.pdf", 2, "local_index"),
        ];
        sort_hits(&mut hits);
        assert_eq!(hits[0].key, "k2", "同分时 my_report.pdf 应排在 report.pdf 前");
    }

    #[test]
    fn dedup_same_score_prefers_more_authoritative() {
        // 同分时低权威先出现、高权威后出现 → 高权威胜出
        let mut hits = vec![
            hit("C:\\a\\b.txt", "file", "b.txt", 60, "app_index"),
            hit("C:\\a\\b.txt", "file", "b.txt", 60, "windows"),
        ];
        dedup_hits(&mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "windows");
    }

    #[test]
    fn dedup_replacement_then_reappearance_keeps_best() {
        // 替换后再现同 key：应与当前保留项比较，而非与首次出现项比较
        let mut hits = vec![
            hit("C:\\a\\b.txt", "file", "b.txt", 50, "windows"),
            hit("C:\\a\\b.txt", "file", "b.txt", 80, "local_index"),
            hit("C:\\a\\b.txt", "file", "b.txt", 80, "windows"),
        ];
        dedup_hits(&mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "windows", "同分时更权威的 windows 应替换 local_index");
    }

    #[test]
    fn escape_like_handles_specials() {
        assert_eq!(escape_like("100%_\\"), "100\\%\\_\\\\");
    }

    #[test]
    fn fts_probe_reports_available_or_degrades() {
        let conn = Connection::open_in_memory().unwrap();
        // bundled SQLite 应支持 trigram；若未来降级也应返回确定值而非 panic
        let ok = ensure_fts(&conn);
        assert!(
            ok || conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='system_search_entries_fts'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0)
                == 0,
            "ensure_fts 返回 false 时不应留下半成品 FTS 表"
        );
    }

    #[test]
    fn fts_sync_and_match_returns_hits() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        // 先插基础表，再同步 FTS，验证 FTS 分支真正命中
        conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\docs\\季度报告.docx', '季度报告.docx', 'file', 'v1', 1, 1)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM system_search_entries LIMIT 1", [], |r| r.get(0))
            .unwrap();
        fts_sync_upsert(&conn, id, "季度报告.docx", "c:\\docs\\季度报告.docx");
        // 3 字符以上查询应走 FTS 分支且命中
        let hits = query_local_index(&conn, "季度报告", 10).unwrap();
        assert_eq!(hits.len(), 1, "FTS 同步后 MATCH 应命中");
        assert_eq!(hits[0].name, "季度报告.docx");
    }

    #[test]
    fn query_local_index_degrades_on_fts_syntax_error() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\x\\100%完成率.png', '100%完成率.png', 'file', 'v1', 1, 1)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM system_search_entries LIMIT 1", [], |r| r.get(0))
            .unwrap();
        fts_sync_upsert(&conn, id, "100%完成率.png", "c:\\x\\100%完成率.png");
        // 含 % 的查询在 FTS MATCH 中会报语法错误，应降级 LIKE 而非整体 Err
        let hits = query_local_index(&conn, "100%", 10).unwrap();
        assert_eq!(hits.len(), 1, "FTS 语法错误应降级 LIKE 命中");
    }

    #[test]
    fn fts_sync_delete_removes_row_and_join_filters_naturally() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\docs\\已删除.txt', '已删除.txt', 'file', 'v1', 1, 1)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM system_search_entries LIMIT 1", [], |r| r.get(0))
            .unwrap();
        fts_sync_upsert(&conn, id, "已删除.txt", "c:\\docs\\已删除.txt");
        assert_eq!(query_local_index(&conn, "已删除", 10).unwrap().len(), 1);
        // 删除基础记录后，即使 FTS 残留行存在，JOIN 也应自然过滤
        conn.execute("DELETE FROM system_search_entries WHERE id=?1", [id]).unwrap();
        assert_eq!(query_local_index(&conn, "已删除", 10).unwrap().len(), 0);
        // 显式调用 fts_sync_delete 后 FTS 表不再有残留
        fts_sync_delete(&conn, id);
        let leftover: i64 = conn
            .query_row(
                "SELECT count(*) FROM system_search_entries_fts WHERE rowid=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(leftover, 0);
    }

    #[test]
    fn query_local_index_searches_name_and_path() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\docs\\年度报告.pdf', '年度报告.pdf', 'file', 'v1', 1, 1)",
            [],
        )
        .unwrap();
        let hits = query_local_index(&conn, "年度", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "file");
        // 路径匹配同样命中
        let hits = query_local_index(&conn, "docs", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }
}
