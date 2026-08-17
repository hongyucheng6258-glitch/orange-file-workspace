use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};

use crate::db::connection::now_unix;
use crate::db::models::{new_id, EditorSession};
use crate::error::AppError;
use crate::services::file_service as fsutil;
use crate::services::hash_service;
use crate::services::preview_service;

/// 保存结果。
#[derive(Debug, serde::Serialize)]
pub enum SaveOutcome {
    Saved,
    /// 磁盘文件在打开后被外部修改。
    Conflict {
        message: String,
        current_size: i64,
    },
}

/// 编辑器可处理的单文件大小上限（10 MiB）。
pub const EDITOR_MAX_BYTES: u64 = 10 * 1024 * 1024;

/// 打开文件：读取内容并建立编辑会话（记录基准指纹）。
/// 返回 (内容, 会话)。
pub fn open_session(
    conn: &Connection,
    resource_id: &str,
    path: &Path,
) -> Result<(String, EditorSession), AppError> {
    if !path.exists() {
        return Err(AppError::new("path_missing", "文件路径不可用"));
    }
    // 编辑必须完整读取；超过上限返回 file_too_large，绝不截断（预览读取与编辑读取分离）
    let content = preview_service::read_text_full(path, EDITOR_MAX_BYTES)?;
    let (size, modified) = fsutil::stat_basic(path)?;
    let now = now_unix();

    let session = upsert_session(conn, resource_id, path, size, modified, &content, now)?;
    Ok((content, session))
}

/// 保存文件。磁盘指纹与基准一致时写回；否则返回冲突。
pub fn save_session(
    conn: &Connection,
    resource_id: &str,
    path: &Path,
    content: &str,
    force: bool,
) -> Result<SaveOutcome, AppError> {
    let existing = get_session(conn, resource_id)?;
    // 防御：历史遗留的截断会话（基准大小超限）拒绝保存，避免覆盖造成数据丢失
    if existing
        .as_ref()
        .is_some_and(|s| s.base_size > EDITOR_MAX_BYTES as i64)
    {
        return Err(AppError::new(
            "file_too_large",
            "该文件超出编辑上限，请重新打开后操作",
        ));
    }
    let base = existing.unwrap_or(EditorSession {
        id: new_id(),
        resource_id: resource_id.to_string(),
        base_path: path.to_string_lossy().to_string(),
        base_size: 0,
        base_modified_at: None,
        base_hash: None,
        draft_content: String::new(),
        language: None,
        is_dirty: false,
        created_at: now_unix(),
        updated_at: now_unix(),
    });

    let (size, modified) = fsutil::stat_basic(path)?;
    let unchanged = size == base.base_size && modified == base.base_modified_at;

    if !unchanged && !force {
        return Ok(SaveOutcome::Conflict {
            message: "文件在编辑期间被外部修改".to_string(),
            current_size: size,
        });
    }

    // 写回文件（原子：先写临时文件再替换）
    let tmp = path.with_extension(format!("{}.tmp", new_id()));
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;

    // 更新会话基准
    let (new_size, new_modified) = fsutil::stat_basic(path)?;
    let new_hash = hash_service::sha256_bytes(content.as_bytes());
    let now = now_unix();
    conn.execute(
        "UPDATE editor_sessions
         SET base_path = ?2, base_size = ?3, base_modified_at = ?4,
             base_hash = ?5, draft_content = ?6, is_dirty = 0, updated_at = ?7
         WHERE resource_id = ?1",
        params![
            resource_id,
            path.to_string_lossy(),
            new_size,
            new_modified,
            new_hash,
            content,
            now
        ],
    )?;

    Ok(SaveOutcome::Saved)
}

/// 丢弃会话（不保存）。
pub fn discard_session(conn: &Connection, resource_id: &str) -> SqliteResult<()> {
    conn.execute(
        "DELETE FROM editor_sessions WHERE resource_id = ?1",
        [resource_id],
    )?;
    Ok(())
}

/// 保存编辑草稿：写入会话的 draft_content 并置 is_dirty=1。
/// 会话不存在时以当前磁盘状态为基准创建会话（用于崩溃恢复）。
pub fn save_draft(
    conn: &Connection,
    resource_id: &str,
    path: &Path,
    content: &str,
) -> Result<(), AppError> {
    let changed = conn.execute(
        "UPDATE editor_sessions
         SET draft_content = ?1, is_dirty = 1, updated_at = ?2
         WHERE resource_id = ?3",
        params![content, now_unix(), resource_id],
    )?;
    if changed == 0 {
        // 无会话：以磁盘状态为基准创建会话后再置 dirty
        let (size, modified) = fsutil::stat_basic(path)?;
        upsert_session(conn, resource_id, path, size, modified, content, now_unix())?;
        conn.execute(
            "UPDATE editor_sessions
             SET draft_content = ?1, is_dirty = 1, updated_at = ?2
             WHERE resource_id = ?3",
            params![content, now_unix(), resource_id],
        )?;
    }
    Ok(())
}

/// 读取未保存草稿（is_dirty=1 且非空）。无草稿时返回 None。
pub fn get_draft(conn: &Connection, resource_id: &str) -> Result<Option<String>, AppError> {
    let draft: Option<String> = conn
        .query_row(
            "SELECT draft_content FROM editor_sessions
             WHERE resource_id = ?1 AND is_dirty = 1",
            [resource_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(draft.filter(|d| !d.is_empty()))
}

/// 读取磁盘文件的当前完整内容（与编辑器同样上限），用于冲突对比。
pub fn read_disk_full(path: &Path) -> Result<String, AppError> {
    preview_service::read_text_full(path, EDITOR_MAX_BYTES)
}

/// 检测磁盘文件是否在会话基准之后被外部修改（size+mtime；文件缺失视为已更改）。
/// 供前端在窗口聚焦时主动刷新冲突状态，避免未保存草稿被静默覆盖。
pub fn check_external_change(
    conn: &Connection,
    resource_id: &str,
    path: &str,
) -> Result<bool, AppError> {
    let base = get_session(conn, resource_id)?;
    let Some(sess) = base else { return Ok(false) };
    if sess.base_size > EDITOR_MAX_BYTES as i64 {
        return Ok(false);
    }
    let p = Path::new(path);
    if !p.exists() {
        return Ok(true);
    }
    let (size, modified) = fsutil::stat_basic(p)?;
    Ok(size != sess.base_size || modified != sess.base_modified_at)
}

/// 最近打开的文件（按会话更新时间倒序，排除已删除资源）。
/// 返回 (resource_id, 文件名, 路径, 更新时间)。
pub fn list_recent_files(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<(String, String, String, i64)>, AppError> {
    let limit = limit.clamp(1, 100) as i64;
    let mut stmt = conn.prepare(
        "SELECT r.id, r.name, l.path, s.updated_at
         FROM editor_sessions s
         JOIN resources r ON r.id = s.resource_id
         LEFT JOIN resource_locations l ON l.resource_id = r.id
         WHERE r.is_deleted = 0 AND l.path IS NOT NULL
         ORDER BY s.updated_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn get_session(conn: &Connection, resource_id: &str) -> SqliteResult<Option<EditorSession>> {
    conn.query_row(
        "SELECT * FROM editor_sessions WHERE resource_id = ?1",
        [resource_id],
        |row| {
            Ok(EditorSession {
                id: row.get("id")?,
                resource_id: row.get("resource_id")?,
                base_path: row.get("base_path")?,
                base_size: row.get("base_size")?,
                base_modified_at: row.get("base_modified_at")?,
                base_hash: row.get("base_hash")?,
                draft_content: row.get("draft_content")?,
                language: row.get("language")?,
                is_dirty: row.get::<_, i64>("is_dirty")? != 0,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        },
    )
    .optional()
}

fn upsert_session(
    conn: &Connection,
    resource_id: &str,
    path: &Path,
    size: i64,
    modified: Option<i64>,
    content: &str,
    now: i64,
) -> SqliteResult<EditorSession> {
    let hash = hash_service::sha256_bytes(content.as_bytes());
    conn.execute(
        "INSERT INTO editor_sessions (
            id, resource_id, base_path, base_size, base_modified_at,
            base_hash, draft_content, is_dirty, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8)
         ON CONFLICT(resource_id) DO UPDATE SET
            base_path = excluded.base_path,
            base_size = excluded.base_size,
            base_modified_at = excluded.base_modified_at,
            base_hash = excluded.base_hash,
            draft_content = excluded.draft_content,
            is_dirty = 0,
            updated_at = excluded.updated_at",
        params![
            new_id(),
            resource_id,
            path.to_string_lossy(),
            size,
            modified,
            hash,
            content,
            now
        ],
    )?;
    Ok(EditorSession {
        id: new_id(),
        resource_id: resource_id.to_string(),
        base_path: path.to_string_lossy().to_string(),
        base_size: size,
        base_modified_at: modified,
        base_hash: Some(hash),
        draft_content: content.to_string(),
        language: None,
        is_dirty: false,
        created_at: now,
        updated_at: now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let mut c = Connection::open_in_memory().expect("db");
        c.pragma_update(None, "foreign_keys", "ON").expect("fk");
        crate::db::migrations::run_migrations(&mut c).expect("migrations");
        c
    }

    fn seed_resource(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO resources (id, kind, name, created_at, updated_at)
             VALUES (?1, 'file', 'test.txt', 1, 1)",
            [id],
        )
        .expect("seed resource");
    }

    #[test]
    fn save_without_external_change_succeeds() {
        let conn = conn();
        seed_resource(&conn, "r1");
        let path = std::env::temp_dir().join(format!("nexus-ed-{}", crate::db::models::new_id()));
        std::fs::write(&path, "v1").expect("write");

        open_session(&conn, "r1", &path).expect("open");
        let outcome = save_session(&conn, "r1", &path, "v2", false).expect("save");

        assert!(matches!(outcome, SaveOutcome::Saved));
        let content = std::fs::read_to_string(&path).expect("read");
        assert_eq!(content, "v2");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn external_change_triggers_conflict() {
        let conn = conn();
        seed_resource(&conn, "r1");
        let path = std::env::temp_dir().join(format!("nexus-ed2-{}", crate::db::models::new_id()));
        std::fs::write(&path, "v1").expect("write");

        open_session(&conn, "r1", &path).expect("open");

        // 外部程序修改文件（改变大小，确保指纹变化）
        std::fs::write(&path, "v1-external-changed").expect("external write");

        let outcome = save_session(&conn, "r1", &path, "v2", false).expect("save");
        assert!(matches!(outcome, SaveOutcome::Conflict { .. }));

        // 磁盘内容未被覆盖
        let content = std::fs::read_to_string(&path).expect("read");
        assert_eq!(content, "v1-external-changed");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn force_save_overwrites_external_change() {
        let conn = conn();
        seed_resource(&conn, "r1");
        let path = std::env::temp_dir().join(format!("nexus-ed3-{}", crate::db::models::new_id()));
        std::fs::write(&path, "v1").expect("write");

        open_session(&conn, "r1", &path).expect("open");
        std::fs::write(&path, "external").expect("external write");

        let outcome = save_session(&conn, "r1", &path, "v2", true).expect("force save");
        assert!(matches!(outcome, SaveOutcome::Saved));

        let content = std::fs::read_to_string(&path).expect("read");
        assert_eq!(content, "v2");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn discard_removes_session() {
        let conn = conn();
        seed_resource(&conn, "r1");
        let path = std::env::temp_dir().join(format!("nexus-ed4-{}", crate::db::models::new_id()));
        std::fs::write(&path, "v1").expect("write");

        open_session(&conn, "r1", &path).expect("open");
        discard_session(&conn, "r1").expect("discard");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM editor_sessions", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn open_reads_full_content_beyond_preview_limit() {
        let conn = conn();
        seed_resource(&conn, "r1");
        let path =
            std::env::temp_dir().join(format!("nexus-edbig-{}", crate::db::models::new_id()));
        let content = vec![b'x'; 300 * 1024]; // 超过 256KiB 预览上限
        std::fs::write(&path, &content).expect("write");

        let (text, _session) = open_session(&conn, "r1", &path).expect("open");
        assert_eq!(text.len(), 300 * 1024, "编辑会话必须包含完整内容");
        assert!(!text.contains("内容过长"), "编辑内容不应带截断提示");

        // 保存后磁盘内容完整
        let outcome = save_session(&conn, "r1", &path, &text, false).expect("save");
        assert!(matches!(outcome, SaveOutcome::Saved));
        let on_disk = std::fs::read_to_string(&path).expect("read");
        assert_eq!(on_disk.len(), 300 * 1024, "保存后原文件必须保持完整");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn open_rejects_file_over_edit_limit() {
        let conn = conn();
        seed_resource(&conn, "r1");
        let path =
            std::env::temp_dir().join(format!("nexus-edhuge-{}", crate::db::models::new_id()));
        std::fs::write(&path, vec![b'x'; (EDITOR_MAX_BYTES + 1) as usize]).expect("write");

        let err = open_session(&conn, "r1", &path).expect_err("should reject");
        assert_eq!(err.code, "file_too_large");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_draft_then_get_returns_content() {
        let conn = conn();
        seed_resource(&conn, "r1");
        let path =
            std::env::temp_dir().join(format!("nexus-eddraft-{}", crate::db::models::new_id()));
        std::fs::write(&path, "v1").expect("write");

        // 无会话时 save_draft 创建会话并置 dirty
        save_draft(&conn, "r1", &path, "draft-v2").expect("save draft");
        let draft = get_draft(&conn, "r1").expect("get draft");
        assert_eq!(draft.as_deref(), Some("draft-v2"), "应能读到未保存草稿");

        // 覆盖更新
        save_draft(&conn, "r1", &path, "draft-v3").expect("save draft again");
        let draft = get_draft(&conn, "r1").expect("get draft again");
        assert_eq!(draft.as_deref(), Some("draft-v3"));

        // 保存成功后草稿被清理（save_session 置 is_dirty=0）
        save_session(&conn, "r1", &path, "v3", false).expect("save");
        let draft = get_draft(&conn, "r1").expect("get draft after save");
        assert_eq!(draft, None, "保存后不应有草稿残留");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn list_recent_files_orders_by_session_update() {
        let conn = conn();
        seed_resource(&conn, "r1");
        seed_resource(&conn, "r2");
        let p1 = std::env::temp_dir().join(format!("nexus-edr1-{}", crate::db::models::new_id()));
        let p2 = std::env::temp_dir().join(format!("nexus-edr2-{}", crate::db::models::new_id()));
        std::fs::write(&p1, "a").expect("write");
        std::fs::write(&p2, "b").expect("write");
        // 补充物理位置（list_recent_files 需要关联 location）
        for (rid, p) in [("r1", &p1), ("r2", &p2)] {
            conn.execute(
                "INSERT INTO resource_locations
                 (id, resource_id, source_type, path, canonical_path, created_at)
                 VALUES (?1, ?2, 'managed', ?3, ?4, 1)",
                rusqlite::params![
                    crate::db::models::new_id(),
                    rid,
                    p.to_string_lossy().to_lowercase().replace('/', "\\"),
                    format!("managed://{rid}"),
                ],
            )
            .expect("seed location");
        }

        // 先打开 r2（更晚的会话更新时间应排前）
        open_session(&conn, "r2", &p2).expect("open r2");
        open_session(&conn, "r1", &p1).expect("open r1");

        // 手动调整 updated_at 以确定顺序：r1 设为旧时间戳（r2 保持最新）
        conn.execute(
            "UPDATE editor_sessions SET updated_at = 1000 WHERE resource_id = 'r1'",
            [],
        )
        .expect("touch r1");

        let recent = list_recent_files(&conn, 10).expect("recent");
        assert_eq!(recent.len(), 2, "两个文件都应出现在最近列表");
        assert_eq!(recent[0].0, "r2", "较新更新的会话排最前");
        assert_eq!(recent[0].1, "test.txt");

        // 软删除 r1 后不再出现在列表
        conn.execute(
            "UPDATE resources SET is_deleted = 1, deleted_at = ?1, updated_at = ?1 WHERE id = 'r1'",
            rusqlite::params![now_unix()],
        )
        .expect("trash r1");
        let recent = list_recent_files(&conn, 10).expect("recent after trash");
        assert_eq!(recent.len(), 1, "已删除资源不应出现在最近列表");
        assert_eq!(recent[0].0, "r2");

        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
    }
}
