use rusqlite::{params, Connection, Result as SqliteResult};
use serde::{Deserialize, Serialize};

/// 生成新的 UUID v4 字符串 ID。
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 资源类型。页面与项目也统一为资源，支持统一收藏、回收站和关联。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    File,
    Folder,
    Page,
    Project,
}

impl ResourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceKind::File => "file",
            ResourceKind::Folder => "folder",
            ResourceKind::Page => "page",
            ResourceKind::Project => "project",
        }
    }
}

/// 文件来源类型：托管复制到应用仓库，或仅引用外部原路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Managed,
    External,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceType::Managed => "managed",
            SourceType::External => "external",
        }
    }
}

/// 统一资源实体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub kind: ResourceKind,
    pub name: String,
    pub parent_id: Option<String>,
    pub is_favorite: bool,
    pub is_deleted: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

/// 文件物理位置。一个资源可以同时有托管位置和外部引用位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLocation {
    pub id: String,
    pub resource_id: String,
    pub source_type: SourceType,
    pub path: String,
    pub canonical_path: Option<String>,
    pub file_size: Option<i64>,
    pub modified_at: Option<i64>,
    pub created_at: i64,
    pub last_verified_at: Option<i64>,
    pub content_hash: Option<String>,
    pub hash_algorithm: Option<String>,
    pub is_available: bool,
}

/// 文件级元数据，与资源一对一。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub resource_id: String,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub encoding: Option<String>,
    pub line_count: Option<i64>,
    pub is_binary: bool,
    pub preview_kind: Option<String>,
    pub metadata_json: Option<String>,
}

/// 富文本页面扩展。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub resource_id: String,
    pub icon: Option<String>,
    pub cover_path: Option<String>,
    pub summary: Option<String>,
    pub content_version: i64,
    pub save_state: String,
    pub editor_mode: String,
}

/// 页面块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageBlock {
    pub id: String,
    pub page_id: String,
    pub parent_block_id: Option<String>,
    pub block_type: String,
    pub block_order: i64,
    pub content_json: String,
    pub plain_text: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 标签。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub created_at: i64,
}

/// 代码项目扩展。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub resource_id: String,
    pub project_type: Option<String>,
    pub language: Option<String>,
    pub entry_file: Option<String>,
    pub readme_resource_id: Option<String>,
    pub ignore_patterns_json: String,
    pub save_mode: String,
    pub last_opened_file_id: Option<String>,
}

/// 编辑会话草稿，用于保存冲突和崩溃恢复。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSession {
    pub id: String,
    pub resource_id: String,
    pub base_path: String,
    pub base_size: i64,
    pub base_modified_at: Option<i64>,
    pub base_hash: Option<String>,
    pub draft_content: String,
    pub language: Option<String>,
    pub is_dirty: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 后台任务。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: String,
    pub status: String,
    pub title: String,
    pub total_count: Option<i64>,
    pub completed_count: i64,
    pub failed_count: i64,
    pub payload_json: Option<String>,
    pub error_json: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub updated_at: i64,
}

/// 任务项（单个文件的处理结果）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskItem {
    pub id: String,
    pub task_id: String,
    pub resource_id: Option<String>,
    pub source_path: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub updated_at: i64,
}

/// 缩略图缓存记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thumbnail {
    pub resource_id: String,
    pub cache_path: String,
    pub width: i64,
    pub height: i64,
    pub source_hash: Option<String>,
    pub generated_at: i64,
    pub status: String,
}

/// 键值设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSetting {
    pub key: String,
    pub value_json: String,
    pub updated_at: i64,
}

/// 备份记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRecord {
    pub id: String,
    pub path: String,
    pub backup_type: String,
    pub database_version: i64,
    pub resource_count: Option<i64>,
    pub file_count: Option<i64>,
    pub created_at: i64,
    pub status: String,
    pub error_message: Option<String>,
}

/// 插入资源并返回实体。由文件、页面、项目服务统一调用。
pub fn insert_resource(
    conn: &Connection,
    id: &str,
    kind: ResourceKind,
    name: &str,
    parent_id: Option<&str>,
    now: i64,
) -> SqliteResult<Resource> {
    conn.execute(
        "INSERT INTO resources (id, kind, name, parent_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, kind.as_str(), name, parent_id, now],
    )?;
    Ok(Resource {
        id: id.to_string(),
        kind,
        name: name.to_string(),
        parent_id: parent_id.map(|s| s.to_string()),
        is_favorite: false,
        is_deleted: false,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    })
}

/// 从行读取 Resource。列顺序与 SELECT 保持一致。
pub fn resource_from_row(row: &rusqlite::Row) -> SqliteResult<Resource> {
    let kind_str: String = row.get("kind")?;
    let kind = match kind_str.as_str() {
        "folder" => ResourceKind::Folder,
        "page" => ResourceKind::Page,
        "project" => ResourceKind::Project,
        _ => ResourceKind::File,
    };
    Ok(Resource {
        id: row.get("id")?,
        kind,
        name: row.get("name")?,
        parent_id: row.get("parent_id")?,
        is_favorite: row.get::<_, i64>("is_favorite")? != 0,
        is_deleted: row.get::<_, i64>("is_deleted")? != 0,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        deleted_at: row.get("deleted_at")?,
    })
}
