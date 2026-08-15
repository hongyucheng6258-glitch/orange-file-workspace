use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};

use crate::db::connection::now_unix;
use crate::db::models::{new_id, insert_resource, Project, Resource, ResourceKind, ResourceLocation, SourceType};
use crate::db::repositories as repo;
use crate::error::AppError;
use crate::services::file_service as fsutil;

/// 创建项目资源：根目录引用 + 项目扩展记录。
pub fn create_project(
    conn: &Connection,
    name: &str,
    root_path: &str,
    ignore_patterns: &[String],
) -> Result<(Resource, Project), AppError> {
    let now = now_unix();
    let id = new_id();

    let root = std::path::PathBuf::from(root_path);
    if !root.is_dir() {
        return Err(AppError::new(
            "not_directory",
            format!("{root_path} 不是有效的目录"),
        ));
    }

    // 项目资源
    insert_resource(conn, &id, ResourceKind::Project, name, None, now)?;

    // 位置记录（external 引用根目录）
    let normalized = fsutil::normalize_path(&root)?;
    repo::upsert_location(
        conn,
        &ResourceLocation {
            id: new_id(),
            resource_id: id.clone(),
            source_type: SourceType::External,
            path: normalized.clone(),
            canonical_path: Some(fsutil::canonical_path_key(&normalized)),
            file_size: None,
            modified_at: None,
            created_at: now,
            last_verified_at: Some(now),
            content_hash: None,
            hash_algorithm: None,
            is_available: true,
        },
    )?;

    // 项目扩展
    conn.execute(
        "INSERT INTO projects (
            resource_id, project_type, language, entry_file,
            readme_resource_id, ignore_patterns_json, save_mode
         ) VALUES (?1, NULL, NULL, NULL, NULL, ?2, 'manual')",
        params![id, serde_json::to_string(ignore_patterns).unwrap_or_else(|_| "[]".into())],
    )?;

    let project = Project {
        resource_id: id.clone(),
        project_type: None,
        language: None,
        entry_file: None,
        readme_resource_id: None,
        ignore_patterns_json: serde_json::to_string(ignore_patterns)
            .unwrap_or_else(|_| "[]".into()),
        save_mode: "manual".to_string(),
        last_opened_file_id: None,
    };

    let resource = repo::get_resource(conn, &id)?.expect("project exists");
    Ok((resource, project))
}

/// 获取项目扩展记录。
pub fn get_project(conn: &Connection, resource_id: &str) -> SqliteResult<Option<Project>> {
    conn.query_row(
        "SELECT * FROM projects WHERE resource_id = ?1",
        [resource_id],
        |row| {
            Ok(Project {
                resource_id: row.get("resource_id")?,
                project_type: row.get("project_type")?,
                language: row.get("language")?,
                entry_file: row.get("entry_file")?,
                readme_resource_id: row.get("readme_resource_id")?,
                ignore_patterns_json: row.get("ignore_patterns_json")?,
                save_mode: row.get("save_mode")?,
                last_opened_file_id: row.get("last_opened_file_id")?,
            })
        },
    )
    .optional()
}

/// 列出全部项目资源。
pub fn list_projects(conn: &Connection) -> SqliteResult<Vec<Resource>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM resources WHERE kind = 'project' AND is_deleted = 0
         ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], crate::db::models::resource_from_row)?;
    rows.collect()
}
