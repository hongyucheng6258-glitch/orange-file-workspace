use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};

use crate::db::models::{
    insert_resource, resource_from_row, FileMetadata, Resource, ResourceKind, ResourceLocation,
    SourceType,
};

/// 列出某父目录下的资源；parent_id 为 None 时列出根目录。
/// include_deleted=false 时排除已删除资源。
pub fn list_children(
    conn: &Connection,
    parent_id: Option<&str>,
    include_deleted: bool,
) -> SqliteResult<Vec<Resource>> {
    let sql = match (parent_id, include_deleted) {
        (Some(_), false) => {
            "SELECT * FROM resources
             WHERE parent_id = ?1 AND is_deleted = 0
             ORDER BY (kind = 'folder') DESC, name COLLATE NOCASE ASC"
        }
        (Some(_), true) => {
            "SELECT * FROM resources
             WHERE parent_id = ?1
             ORDER BY name COLLATE NOCASE ASC"
        }
        (None, false) => {
            "SELECT * FROM resources
             WHERE parent_id IS NULL AND is_deleted = 0
             ORDER BY (kind = 'folder') DESC, name COLLATE NOCASE ASC"
        }
        (None, true) => {
            "SELECT * FROM resources
             WHERE parent_id IS NULL
             ORDER BY name COLLATE NOCASE ASC"
        }
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = if parent_id.is_some() {
        stmt.query_map([parent_id], resource_from_row)?
    } else {
        stmt.query_map([], resource_from_row)?
    };
    rows.collect()
}

/// 按 ID 获取资源。
pub fn get_resource(conn: &Connection, id: &str) -> SqliteResult<Option<Resource>> {
    conn.query_row(
        "SELECT * FROM resources WHERE id = ?1",
        [id],
        resource_from_row,
    )
    .optional()
}

/// 创建文件夹资源。目录已存在于文件系统时才调用。
pub fn create_folder(
    conn: &Connection,
    id: &str,
    name: &str,
    parent_id: Option<&str>,
    now: i64,
) -> SqliteResult<Resource> {
    insert_resource(conn, id, ResourceKind::Folder, name, parent_id, now)
}

/// 重命名资源并更新时间戳。
pub fn rename_resource(conn: &Connection, id: &str, name: &str, now: i64) -> SqliteResult<()> {
    conn.execute(
        "UPDATE resources SET name = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, name, now],
    )?;
    Ok(())
}

/// 移动资源到新父目录。
pub fn move_resource(
    conn: &Connection,
    id: &str,
    new_parent_id: Option<&str>,
    now: i64,
) -> SqliteResult<()> {
    conn.execute(
        "UPDATE resources SET parent_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, new_parent_id, now],
    )?;
    Ok(())
}

/// 软删除：标记删除并记录删除时间。
pub fn soft_delete(conn: &Connection, id: &str, now: i64) -> SqliteResult<()> {
    conn.execute(
        "UPDATE resources SET is_deleted = 1, deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

/// 从回收站恢复：清除删除标记。原父目录存在性由调用方检查。
pub fn restore(conn: &Connection, id: &str, now: i64) -> SqliteResult<()> {
    conn.execute(
        "UPDATE resources SET is_deleted = 0, deleted_at = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

/// 永久删除（连同级联依赖）。调用方需先删除磁盘文件。
#[allow(dead_code)] // 备用 API，回收站永久删除目前在命令层直接实现
pub fn delete_permanently(conn: &Connection, id: &str) -> SqliteResult<()> {
    conn.execute("DELETE FROM resources WHERE id = ?1", [id])?;
    Ok(())
}

/// 列出回收站中的资源。
pub fn list_trash(conn: &Connection) -> SqliteResult<Vec<Resource>> {
    let mut stmt =
        conn.prepare("SELECT * FROM resources WHERE is_deleted = 1 ORDER BY deleted_at DESC")?;
    let rows = stmt.query_map([], resource_from_row)?;
    rows.collect()
}

/// 插入或更新文件位置记录。
pub fn upsert_location(conn: &Connection, loc: &ResourceLocation) -> SqliteResult<()> {
    conn.execute(
        "INSERT INTO resource_locations (
            id, resource_id, source_type, path, canonical_path,
            file_size, modified_at, created_at, last_verified_at,
            content_hash, hash_algorithm, is_available
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12
         )
         ON CONFLICT(source_type, canonical_path) DO UPDATE SET
            path = excluded.path,
            file_size = excluded.file_size,
            modified_at = excluded.modified_at,
            last_verified_at = excluded.last_verified_at,
            content_hash = excluded.content_hash,
            hash_algorithm = excluded.hash_algorithm,
            is_available = excluded.is_available",
        params![
            loc.id,
            loc.resource_id,
            loc.source_type.as_str(),
            loc.path,
            loc.canonical_path,
            loc.file_size,
            loc.modified_at,
            loc.created_at,
            loc.last_verified_at,
            loc.content_hash,
            loc.hash_algorithm,
            loc.is_available as i64,
        ],
    )?;
    Ok(())
}

/// 获取资源的全部位置记录。
pub fn list_locations(conn: &Connection, resource_id: &str) -> SqliteResult<Vec<ResourceLocation>> {
    let mut stmt = conn
        .prepare("SELECT * FROM resource_locations WHERE resource_id = ?1 ORDER BY created_at")?;
    let rows = stmt.query_map([resource_id], |row| {
        Ok(ResourceLocation {
            id: row.get("id")?,
            resource_id: row.get("resource_id")?,
            source_type: match row.get::<_, String>("source_type")?.as_str() {
                "managed" => SourceType::Managed,
                _ => SourceType::External,
            },
            path: row.get("path")?,
            canonical_path: row.get("canonical_path")?,
            file_size: row.get("file_size")?,
            modified_at: row.get("modified_at")?,
            created_at: row.get("created_at")?,
            last_verified_at: row.get("last_verified_at")?,
            content_hash: row.get("content_hash")?,
            hash_algorithm: row.get("hash_algorithm")?,
            is_available: row.get::<_, i64>("is_available")? != 0,
        })
    })?;
    rows.collect()
}

/// 按规范路径查找资源位置。
pub fn find_location_by_path(
    conn: &Connection,
    canonical_path: &str,
) -> SqliteResult<Option<ResourceLocation>> {
    conn.query_row(
        "SELECT * FROM resource_locations WHERE canonical_path = ?1 LIMIT 1",
        [canonical_path],
        |row| {
            Ok(ResourceLocation {
                id: row.get("id")?,
                resource_id: row.get("resource_id")?,
                source_type: match row.get::<_, String>("source_type")?.as_str() {
                    "managed" => SourceType::Managed,
                    _ => SourceType::External,
                },
                path: row.get("path")?,
                canonical_path: row.get("canonical_path")?,
                file_size: row.get("file_size")?,
                modified_at: row.get("modified_at")?,
                created_at: row.get("created_at")?,
                last_verified_at: row.get("last_verified_at")?,
                content_hash: row.get("content_hash")?,
                hash_algorithm: row.get("hash_algorithm")?,
                is_available: row.get::<_, i64>("is_available")? != 0,
            })
        },
    )
    .optional()
}

/// 更新位置可用性状态。
pub fn set_location_availability(
    conn: &Connection,
    location_id: &str,
    is_available: bool,
    now: i64,
) -> SqliteResult<()> {
    conn.execute(
        "UPDATE resource_locations SET is_available = ?2, last_verified_at = ?3 WHERE id = ?1",
        params![location_id, is_available as i64, now],
    )?;
    Ok(())
}

/// 插入或更新文件元数据。
#[allow(dead_code)] // 备用 API：批量导入使用手写 SQL 以支持批量事务
pub fn upsert_file_metadata(conn: &Connection, meta: &FileMetadata) -> SqliteResult<()> {
    conn.execute(
        "INSERT INTO file_metadata (
            resource_id, extension, mime_type, size_bytes, width, height,
            duration_ms, encoding, line_count, is_binary, preview_kind, metadata_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(resource_id) DO UPDATE SET
            extension = excluded.extension,
            mime_type = excluded.mime_type,
            size_bytes = excluded.size_bytes,
            width = excluded.width,
            height = excluded.height,
            duration_ms = excluded.duration_ms,
            encoding = excluded.encoding,
            line_count = excluded.line_count,
            is_binary = excluded.is_binary,
            preview_kind = excluded.preview_kind,
            metadata_json = excluded.metadata_json",
        params![
            meta.resource_id,
            meta.extension,
            meta.mime_type,
            meta.size_bytes,
            meta.width,
            meta.height,
            meta.duration_ms,
            meta.encoding,
            meta.line_count,
            meta.is_binary as i64,
            meta.preview_kind,
            meta.metadata_json,
        ],
    )?;
    Ok(())
}

/// 获取文件元数据。
#[allow(dead_code)] // 备用 API：详情面板后续将展示文件元数据
pub fn get_file_metadata(
    conn: &Connection,
    resource_id: &str,
) -> SqliteResult<Option<FileMetadata>> {
    conn.query_row(
        "SELECT * FROM file_metadata WHERE resource_id = ?1",
        [resource_id],
        |row| {
            Ok(FileMetadata {
                resource_id: row.get("resource_id")?,
                extension: row.get("extension")?,
                mime_type: row.get("mime_type")?,
                size_bytes: row.get("size_bytes")?,
                width: row.get("width")?,
                height: row.get("height")?,
                duration_ms: row.get("duration_ms")?,
                encoding: row.get("encoding")?,
                line_count: row.get("line_count")?,
                is_binary: row.get::<_, i64>("is_binary")? != 0,
                preview_kind: row.get("preview_kind")?,
                metadata_json: row.get("metadata_json")?,
            })
        },
    )
    .optional()
}

/// 查询缩略图缓存路径。
pub fn get_thumbnail_path(conn: &Connection, resource_id: &str) -> SqliteResult<Option<String>> {
    conn.query_row(
        "SELECT cache_path FROM thumbnails WHERE resource_id = ?1",
        [resource_id],
        |row| row.get(0),
    )
    .optional()
}

/// 插入或更新缩略图记录。
pub fn upsert_thumbnail(
    conn: &Connection,
    resource_id: &str,
    cache_path: &str,
    width: i64,
    height: i64,
    source_hash: Option<&str>,
    now: i64,
) -> SqliteResult<()> {
    conn.execute(
        "INSERT INTO thumbnails (resource_id, cache_path, width, height, source_hash, generated_at, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ready')
         ON CONFLICT(resource_id) DO UPDATE SET
            cache_path = excluded.cache_path,
            width = excluded.width,
            height = excluded.height,
            source_hash = excluded.source_hash,
            generated_at = excluded.generated_at,
            status = 'ready'",
        params![resource_id, cache_path, width, height, source_hash, now],
    )?;
    Ok(())
}

/// 更新文件位置的内容哈希。
pub fn update_location_hash(
    conn: &Connection,
    location_id: &str,
    content_hash: &str,
    algorithm: &str,
) -> SqliteResult<()> {
    conn.execute(
        "UPDATE resource_locations SET content_hash = ?2, hash_algorithm = ?3 WHERE id = ?1",
        params![location_id, content_hash, algorithm],
    )?;
    Ok(())
}

/// 读取应用设置（value_json 原样返回）。
pub fn get_setting(conn: &Connection, key: &str) -> SqliteResult<Option<String>> {
    conn.query_row(
        "SELECT value_json FROM app_settings WHERE key = ?1",
        [key],
        |row| row.get(0),
    )
    .optional()
}

/// 写入应用设置（value_json 需调用方自行 JSON 序列化）。
pub fn set_setting(conn: &Connection, key: &str, value_json: &str) -> SqliteResult<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![key, value_json, crate::db::connection::now_unix()],
    )?;
    Ok(())
}

/// 更新文件位置的尺寸/时间信息（外部变化后同步）。
pub fn update_location_stat(
    conn: &Connection,
    location_id: &str,
    file_size: i64,
    modified_at: Option<i64>,
) -> SqliteResult<()> {
    conn.execute(
        "UPDATE resource_locations SET file_size = ?2, modified_at = ?3 WHERE id = ?1",
        params![location_id, file_size, modified_at],
    )?;
    Ok(())
}

/// 切换收藏状态，返回切换后的状态。
pub fn toggle_favorite(conn: &Connection, id: &str, now: i64) -> SqliteResult<bool> {
    let current: bool = conn
        .query_row(
            "SELECT is_favorite = 1 FROM resources WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(false);
    let next = !current;
    conn.execute(
        "UPDATE resources SET is_favorite = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, next as i64, now],
    )?;
    Ok(next)
}

/// 列出全部收藏资源。
pub fn list_favorites(conn: &Connection) -> SqliteResult<Vec<Resource>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM resources
         WHERE is_favorite = 1 AND is_deleted = 0
         ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], resource_from_row)?;
    rows.collect()
}
