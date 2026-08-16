use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};

use crate::db::connection::now_unix;
use crate::db::models::{new_id, Page, PageBlock, Resource, ResourceKind};

/// 创建页面资源与页面扩展记录。
pub fn create_page(
    conn: &Connection,
    name: &str,
    parent_id: Option<&str>,
) -> SqliteResult<(Resource, Page)> {
    let now = now_unix();
    let resource = crate::db::models::insert_resource(
        conn,
        &new_id(),
        ResourceKind::Page,
        name,
        parent_id,
        now,
    )?;
    conn.execute(
        "INSERT INTO pages (resource_id, content_version, save_state, editor_mode, content_json)
         VALUES (?1, 1, 'saved', 'blocks', ?2)",
        [&resource.id, EMPTY_DOC],
    )?;
    let page = Page {
        resource_id: resource.id.clone(),
        icon: None,
        cover_path: None,
        summary: None,
        content_version: 1,
        save_state: "saved".to_string(),
        editor_mode: "blocks".to_string(),
        content_json: Some(EMPTY_DOC.to_string()),
    };
    Ok((resource, page))
}

/// 空文档 JSON（TipTap doc 结构）。
pub const EMPTY_DOC: &str = r#"{"type":"doc","content":[]}"#;

/// 获取页面扩展记录。
pub fn get_page(conn: &Connection, resource_id: &str) -> SqliteResult<Option<Page>> {
    conn.query_row(
        "SELECT * FROM pages WHERE resource_id = ?1",
        [resource_id],
        |row| {
            Ok(Page {
                resource_id: row.get("resource_id")?,
                icon: row.get("icon")?,
                cover_path: row.get("cover_path")?,
                summary: row.get("summary")?,
                content_version: row.get("content_version")?,
                save_state: row.get("save_state")?,
                editor_mode: row.get("editor_mode")?,
                content_json: row.get("content_json")?,
            })
        },
    )
    .optional()
}

/// 保存页面富文本文档（全量替换 content_json）。
pub fn save_page_document(
    conn: &Connection,
    page_id: &str,
    content_json: &str,
    plain_text: &str,
) -> SqliteResult<()> {
    let now = now_unix();
    let summary = summarize_plain_text(plain_text);
    conn.execute(
        "UPDATE pages SET content_json = ?2, content_version = content_version + 1,
         save_state = 'saved', summary = ?3
         WHERE resource_id = ?1",
        params![page_id, content_json, summary],
    )?;
    conn.execute(
        "UPDATE resources SET updated_at = ?2 WHERE id = ?1",
        params![page_id, now],
    )?;
    Ok(())
}

/// 由纯文本生成摘要（最多 200 字）。
fn summarize_plain_text(plain_text: &str) -> Option<String> {
    let joined: String = plain_text
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(200)
        .collect();
    if joined.trim().is_empty() {
        None
    } else {
        Some(plain_text.chars().take(200).collect())
    }
}

/// 列出某父目录下的页面资源（页面树节点）。
pub fn list_pages(conn: &Connection, parent_id: Option<&str>) -> SqliteResult<Vec<Resource>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM resources
         WHERE kind = 'page' AND is_deleted = 0
           AND ((?1 IS NULL AND parent_id IS NULL) OR parent_id = ?1)
         ORDER BY name COLLATE NOCASE ASC",
    )?;
    let rows = stmt.query_map([parent_id], |row| crate::db::models::resource_from_row(row))?;
    rows.collect()
}

/// 读取页面的全部块（含子块）。
pub fn list_all_blocks(conn: &Connection, page_id: &str) -> SqliteResult<Vec<PageBlock>> {
    let mut stmt =
        conn.prepare("SELECT * FROM page_blocks WHERE page_id = ?1 ORDER BY block_order ASC")?;
    let rows = stmt.query_map([page_id], page_block_from_row)?;
    rows.collect()
}

/// 全量替换页面块（编辑器整体保存模型）。同一事务内先删后插。
pub fn replace_blocks(
    conn: &mut Connection,
    page_id: &str,
    blocks: &[BlockInput],
) -> SqliteResult<()> {
    let now = now_unix();
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM page_blocks WHERE page_id = ?1", [page_id])?;
    for (idx, b) in blocks.iter().enumerate() {
        tx.execute(
            "INSERT INTO page_blocks (
                id, page_id, parent_block_id, block_type, block_order,
                content_json, plain_text, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                new_id(),
                page_id,
                b.parent_block_id,
                b.block_type,
                (idx as i64) * 10,
                b.content_json,
                b.plain_text,
                now
            ],
        )?;
    }
    tx.execute(
        "UPDATE pages SET content_version = content_version + 1, save_state = 'saved',
         summary = ?2
         WHERE resource_id = ?1",
        params![page_id, blocks_summary(blocks)],
    )?;
    tx.execute(
        "UPDATE resources SET updated_at = ?2 WHERE id = ?1",
        params![page_id, now],
    )?;
    tx.commit()?;
    Ok(())
}

/// 由前端提交的块输入。
pub struct BlockInput {
    pub parent_block_id: Option<String>,
    pub block_type: String,
    pub content_json: String,
    pub plain_text: Option<String>,
}

fn blocks_summary(blocks: &[BlockInput]) -> Option<String> {
    let joined: String = blocks
        .iter()
        .filter_map(|b| b.plain_text.as_deref())
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect();
    if joined.trim().is_empty() {
        None
    } else {
        Some(joined)
    }
}

fn page_block_from_row(row: &rusqlite::Row) -> SqliteResult<PageBlock> {
    Ok(PageBlock {
        id: row.get("id")?,
        page_id: row.get("page_id")?,
        parent_block_id: row.get("parent_block_id")?,
        block_type: row.get("block_type")?,
        block_order: row.get("block_order")?,
        content_json: row.get("content_json")?,
        plain_text: row.get("plain_text")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;

    fn conn() -> Connection {
        let mut c = Connection::open_in_memory().expect("db");
        c.pragma_update(None, "foreign_keys", "ON").expect("fk");
        run_migrations(&mut c).expect("migrations");
        c
    }

    #[test]
    fn page_lifecycle() {
        let conn = conn();
        let (resource, page) = create_page(&conn, "工作笔记", None).expect("create");
        assert_eq!(page.resource_id, resource.id);
        assert_eq!(resource.kind, ResourceKind::Page);

        // 页面出现在树中
        let pages = list_pages(&conn, None).expect("list");
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].id, resource.id);
    }

    #[test]
    fn replace_blocks_is_atomic_and_summarizes() {
        let mut conn = conn();
        let (resource, _) = create_page(&conn, "文档", None).expect("create");

        let blocks = vec![
            BlockInput {
                parent_block_id: None,
                block_type: "heading".into(),
                content_json: "{\"text\":\"第一章\"}".into(),
                plain_text: Some("第一章".into()),
            },
            BlockInput {
                parent_block_id: None,
                block_type: "paragraph".into(),
                content_json: "{\"text\":\"正文内容\"}".into(),
                plain_text: Some("正文内容".into()),
            },
        ];
        replace_blocks(&mut conn, &resource.id, &blocks).expect("replace");

        let all = list_all_blocks(&conn, &resource.id).expect("list");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].block_order, 0);
        assert_eq!(all[1].block_order, 10);

        let page = get_page(&conn, &resource.id).expect("get").expect("page");
        assert_eq!(page.save_state, "saved");
        assert_eq!(page.content_version, 2);
        assert!(page.summary.is_some());
        assert!(page.summary.unwrap().contains("第一章"));
    }

    #[test]
    fn second_replace_clears_old_blocks() {
        let mut conn = conn();
        let (resource, _) = create_page(&conn, "文档", None).expect("create");

        let first = vec![BlockInput {
            parent_block_id: None,
            block_type: "paragraph".into(),
            content_json: "{}".into(),
            plain_text: Some("a".into()),
        }];
        let second = vec![BlockInput {
            parent_block_id: None,
            block_type: "paragraph".into(),
            content_json: "{}".into(),
            plain_text: Some("b".into()),
        }];

        replace_blocks(&mut conn, &resource.id, &first).expect("first");
        replace_blocks(&mut conn, &resource.id, &second).expect("second");

        let all = list_all_blocks(&conn, &resource.id).expect("list");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].plain_text.as_deref(), Some("b"));
    }
}
