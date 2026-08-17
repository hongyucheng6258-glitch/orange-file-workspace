-- 0010_unified_workspace.sql
-- 第一阶段：统一工作台 - 命令面板、最近使用、工作区会话

-- 最近使用记录表
CREATE TABLE recent_items (
    id TEXT PRIMARY KEY,
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL CHECK(resource_type IN ('file', 'folder', 'page', 'project')),
    access_count INTEGER NOT NULL DEFAULT 1,
    last_accessed_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(resource_id)
);

CREATE INDEX idx_recent_items_accessed
ON recent_items(last_accessed_at DESC);

CREATE INDEX idx_recent_items_type
ON recent_items(resource_type, last_accessed_at DESC);

-- 工作区会话表（保存项目状态）
CREATE TABLE workspace_sessions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    open_files_json TEXT NOT NULL DEFAULT '[]',
    active_file_id TEXT,
    terminal_tabs_json TEXT NOT NULL DEFAULT '[]',
    active_terminal_index INTEGER,
    running_tasks_json TEXT NOT NULL DEFAULT '[]',
    panel_layout_json TEXT,
    scroll_positions_json TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_restored_at INTEGER,
    UNIQUE(project_id)
);

CREATE INDEX idx_workspace_sessions_project
ON workspace_sessions(project_id, updated_at DESC);

-- Git 仓库状态表
CREATE TABLE git_repositories (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    repo_path TEXT NOT NULL,
    current_branch TEXT,
    remote_url TEXT,
    last_fetch_at INTEGER,
    has_uncommitted INTEGER NOT NULL DEFAULT 0,
    ahead_count INTEGER NOT NULL DEFAULT 0,
    behind_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(project_id)
);

CREATE INDEX idx_git_repos_project
ON git_repositories(project_id);

-- Git 文件状态表（缓存）
CREATE TABLE git_file_status (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL REFERENCES git_repositories(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('untracked', 'modified', 'added', 'deleted', 'renamed', 'conflicted')),
    staged INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(repo_id, file_path)
);

CREATE INDEX idx_git_file_status_repo
ON git_file_status(repo_id, status);

-- 命令历史表（命令面板）
CREATE TABLE command_history (
    id TEXT PRIMARY KEY,
    command_id TEXT NOT NULL,
    command_label TEXT NOT NULL,
    command_category TEXT NOT NULL,
    execution_count INTEGER NOT NULL DEFAULT 1,
    last_executed_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_command_history_executed
ON command_history(last_executed_at DESC);

CREATE INDEX idx_command_history_count
ON command_history(execution_count DESC, last_executed_at DESC);
