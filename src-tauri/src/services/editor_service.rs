use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};

use crate::db::connection::now_unix;
use crate::db::models::{new_id, EditorSession};
use crate::error::AppError;
use crate::services::file_service as fsutil;
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

/// 打开文件：读取内容并建立编辑会话（记录基准指纹）。
/// 返回 (内容, 会话)。
pub fn open_session(
    conn: &Connection,
    resource_id: &str,
    path: &PathBuf,
) -> Result<(String, EditorSession), AppError> {
    if !path.exists() {
        return Err(AppError::new("path_missing", "文件路径不可用"));
    }
    let content = preview_service::read_text_preview(path)?;
    let (size, modified) = fsutil::stat_basic(path)?;
    let now = now_unix();

    let session = upsert_session(
        conn,
        resource_id,
        path,
        size,
        modified,
        &content,
        now,
    )?;
    Ok((content, session))
}

/// 保存文件。磁盘指纹与基准一致时写回；否则返回冲突。
pub fn save_session(
    conn: &Connection,
    resource_id: &str,
    path: &PathBuf,
    content: &str,
    force: bool,
) -> Result<SaveOutcome, AppError> {
    let existing = get_session(conn, resource_id)?;
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
    let now = now_unix();
    conn.execute(
        "UPDATE editor_sessions
         SET base_path = ?2, base_size = ?3, base_modified_at = ?4,
             draft_content = ?5, is_dirty = 0, updated_at = ?6
         WHERE resource_id = ?1",
        params![resource_id, path.to_string_lossy(), new_size, new_modified, content, now],
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

fn get_session(
    conn: &Connection,
    resource_id: &str,
) -> SqliteResult<Option<EditorSession>> {
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
    path: &PathBuf,
    size: i64,
    modified: Option<i64>,
    content: &str,
    now: i64,
) -> SqliteResult<EditorSession> {
    conn.execute(
        "INSERT INTO editor_sessions (
            id, resource_id, base_path, base_size, base_modified_at,
            base_hash, draft_content, is_dirty, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, 0, ?7, ?7)
         ON CONFLICT(resource_id) DO UPDATE SET
            base_path = excluded.base_path,
            base_size = excluded.base_size,
            base_modified_at = excluded.base_modified_at,
            draft_content = excluded.draft_content,
            is_dirty = 0,
            updated_at = excluded.updated_at",
        params![
            new_id(),
            resource_id,
            path.to_string_lossy(),
            size,
            modified,
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
        base_hash: None,
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
}
