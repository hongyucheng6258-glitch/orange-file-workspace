-- 系统搜索：文件/文件夹索引（与业务 resources 表隔离）
CREATE TABLE IF NOT EXISTS system_search_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    canonical_path TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name TEXT NOT NULL,
    entry_kind TEXT NOT NULL CHECK (entry_kind IN ('file', 'folder')),
    extension TEXT,
    file_size INTEGER,
    modified_at INTEGER,
    volume_id TEXT NOT NULL,
    scan_generation INTEGER NOT NULL,
    is_offline INTEGER NOT NULL DEFAULT 0,
    indexed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sse_name ON system_search_entries (display_name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_sse_volume_gen ON system_search_entries (volume_id, scan_generation);
CREATE INDEX IF NOT EXISTS idx_sse_kind ON system_search_entries (entry_kind);

-- 系统搜索：应用索引
CREATE TABLE IF NOT EXISTS system_search_apps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    app_kind TEXT NOT NULL CHECK (app_kind IN ('win32', 'shortcut', 'store')),
    display_name TEXT NOT NULL,
    launch_target TEXT,
    canonical_target TEXT NOT NULL UNIQUE COLLATE NOCASE,
    aumid TEXT,
    icon_source TEXT,
    install_location TEXT,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ssa_name ON system_search_apps (display_name);
CREATE INDEX IF NOT EXISTS idx_ssa_kind ON system_search_apps (app_kind);

-- 系统搜索：每个卷的扫描状态
CREATE TABLE IF NOT EXISTS system_search_scan_state (
    volume_id TEXT PRIMARY KEY,
    root_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'scanning', 'paused', 'completed', 'error', 'offline')),
    scan_generation INTEGER NOT NULL DEFAULT 1,
    checkpoint TEXT,
    indexed_count INTEGER NOT NULL DEFAULT 0,
    skipped_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    started_at INTEGER,
    completed_at INTEGER
);
