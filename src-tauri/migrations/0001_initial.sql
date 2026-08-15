-- 0001_initial.sql
-- 本地文件工作台初始 schema

CREATE TABLE resources (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('file', 'folder', 'page', 'project')),
    name TEXT NOT NULL,
    parent_id TEXT REFERENCES resources(id) ON DELETE CASCADE,
    is_favorite INTEGER NOT NULL DEFAULT 0,
    is_deleted INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);

CREATE INDEX idx_resources_parent
ON resources(parent_id, is_deleted, name COLLATE NOCASE);

CREATE INDEX idx_resources_kind
ON resources(kind, is_deleted);

CREATE INDEX idx_resources_favorite
ON resources(is_favorite, is_deleted);

CREATE INDEX idx_resources_updated
ON resources(updated_at DESC);

CREATE TABLE resource_locations (
    id TEXT PRIMARY KEY,
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL CHECK(source_type IN ('managed', 'external')),
    path TEXT NOT NULL,
    canonical_path TEXT,
    file_size INTEGER,
    modified_at INTEGER,
    created_at INTEGER NOT NULL,
    last_verified_at INTEGER,
    content_hash TEXT,
    hash_algorithm TEXT,
    is_available INTEGER NOT NULL DEFAULT 1,
    UNIQUE(source_type, canonical_path)
);

CREATE INDEX idx_locations_resource ON resource_locations(resource_id);
CREATE INDEX idx_locations_path ON resource_locations(canonical_path);
CREATE INDEX idx_locations_hash ON resource_locations(content_hash);
CREATE INDEX idx_locations_available ON resource_locations(is_available, source_type);

CREATE TABLE file_metadata (
    resource_id TEXT PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
    extension TEXT,
    mime_type TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    width INTEGER,
    height INTEGER,
    duration_ms INTEGER,
    encoding TEXT,
    line_count INTEGER,
    is_binary INTEGER NOT NULL DEFAULT 0,
    preview_kind TEXT,
    metadata_json TEXT
);

CREATE INDEX idx_file_metadata_extension ON file_metadata(extension);
CREATE INDEX idx_file_metadata_mime ON file_metadata(mime_type);
CREATE INDEX idx_file_metadata_size ON file_metadata(mime_type, size_bytes);

CREATE TABLE pages (
    resource_id TEXT PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
    icon TEXT,
    cover_path TEXT,
    summary TEXT,
    content_version INTEGER NOT NULL DEFAULT 1,
    save_state TEXT NOT NULL DEFAULT 'saved'
        CHECK(save_state IN ('saved', 'dirty', 'conflict')),
    editor_mode TEXT NOT NULL DEFAULT 'blocks'
);

CREATE TABLE page_blocks (
    id TEXT PRIMARY KEY,
    page_id TEXT NOT NULL REFERENCES pages(resource_id) ON DELETE CASCADE,
    parent_block_id TEXT REFERENCES page_blocks(id) ON DELETE CASCADE,
    block_type TEXT NOT NULL,
    block_order INTEGER NOT NULL,
    content_json TEXT NOT NULL,
    plain_text TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_page_blocks_order
ON page_blocks(page_id, parent_block_id, block_order);

CREATE TABLE resource_relations (
    source_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(source_id, target_id, relation_type)
);

CREATE INDEX idx_relations_target
ON resource_relations(target_id, relation_type);

CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE resource_tags (
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(resource_id, tag_id)
);

CREATE INDEX idx_resource_tags_tag
ON resource_tags(tag_id, resource_id);

CREATE TABLE projects (
    resource_id TEXT PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
    project_type TEXT,
    language TEXT,
    entry_file TEXT,
    readme_resource_id TEXT REFERENCES resources(id),
    ignore_patterns_json TEXT NOT NULL DEFAULT '[]',
    save_mode TEXT NOT NULL DEFAULT 'manual'
        CHECK(save_mode IN ('manual')),
    last_opened_file_id TEXT REFERENCES resources(id)
);

CREATE TABLE editor_sessions (
    id TEXT PRIMARY KEY,
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    base_path TEXT NOT NULL,
    base_size INTEGER NOT NULL,
    base_modified_at INTEGER,
    base_hash TEXT,
    draft_content TEXT NOT NULL,
    language TEXT,
    is_dirty INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_editor_sessions_resource
ON editor_sessions(resource_id, is_dirty);

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    task_type TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN (
        'queued', 'running', 'paused', 'completed', 'failed', 'cancelled'
    )),
    title TEXT NOT NULL,
    total_count INTEGER,
    completed_count INTEGER NOT NULL DEFAULT 0,
    failed_count INTEGER NOT NULL DEFAULT 0,
    payload_json TEXT,
    error_json TEXT,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_tasks_status ON tasks(status, updated_at);

CREATE INDEX idx_tasks_active
ON tasks(status)
WHERE status IN ('queued', 'running', 'paused');

CREATE TABLE task_items (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    resource_id TEXT REFERENCES resources(id) ON DELETE SET NULL,
    source_path TEXT,
    status TEXT NOT NULL,
    error_message TEXT,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_task_items_task ON task_items(task_id, status);

CREATE TABLE thumbnails (
    resource_id TEXT PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
    cache_path TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    source_hash TEXT,
    generated_at INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'ready'
);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE backup_records (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    backup_type TEXT NOT NULL,
    database_version INTEGER NOT NULL,
    resource_count INTEGER,
    file_count INTEGER,
    created_at INTEGER NOT NULL,
    status TEXT NOT NULL,
    error_message TEXT
);
