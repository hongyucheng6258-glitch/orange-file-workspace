# 本地文件工作台设计规格

## 1. 产品定位

本产品是一款 Windows 优先的本地单用户文件工作台，使用 Tauri 2、Rust、React、TypeScript 和 SQLite 构建。它以文件资源管理为核心，同时提供图片和常见文件预览、富文本页面、代码项目管理与本地代码编辑能力。

文件可以采用两种导入方式：

- 托管导入：复制到应用管理的文件仓库。
- 外部引用：保留原始路径，只在数据库中建立索引。

首版不包含账号体系、云同步、多人协作、GitHub 远程同步、Git 分支和提交历史。

## 2. 已确认范围

- 首版平台：Windows。
- 文件规模：按 10 万以上资源设计。
- UI：简约科技风、浅色主题、工作台式导航。
- 导入：优先支持拖拽导入，同时支持系统文件选择器。
- 信息层：富文本页面、页面树、附件、标签和资源关联。
- 代码：项目目录、文件树、README、语法高亮、代码编辑和手动保存。
- 备份：一键导出备份包。
- 安全模型：单用户本地使用，不做登录和多人权限。

## 3. UI 结构

### 3.1 应用壳层

固定工作台壳层由四部分组成：

- 左侧导航栏：品牌、首页、文件、页面、代码项目、收藏、回收站、任务中心、设置。
- 顶部工具栏：面包屑、全局搜索、前进后退、视图切换、排序、筛选和主操作。
- 中央工作区：文件列表、文件网格、页面编辑器或代码编辑器。
- 右侧详情栏：预览、元数据、标签、备注和关联资源，可折叠为抽屉。

窗口较窄时隐藏右侧详情栏，详情通过抽屉打开；文件列表切换为紧凑模式，代码编辑器保持最小可用宽度。

### 3.2 页面清单

| 页面 | 内容 | 关键操作 |
|---|---|---|
| 首页 | 最近使用、收藏、最近项目、任务和存储概览 | 快速导入、新建页面、打开项目 |
| 文件 | 目录树、列表/网格、预览和详情 | 导入、移动、重命名、收藏、标签、删除 |
| 页面 | 页面树、块编辑器、附件和关联 | 新建、编辑、嵌入文件、自动保存 |
| 代码项目 | 项目列表、项目文件树、README、编辑器 | 导入目录、编辑、手动保存、打开外部目录 |
| 搜索 | 查询框、筛选器、结果列表 | 名称、路径、类型、标签、时间筛选 |
| 收藏 | 文件、页面、项目统一列表 | 查看、取消收藏、批量操作 |
| 回收站 | 已删除资源、删除时间和恢复状态 | 恢复、永久删除、清空 |
| 任务中心 | 导入、扫描、缩略图、哈希和备份任务 | 暂停、取消、重试、查看错误 |
| 设置 | 仓库、导入、忽略规则、备份、外观 | 修改目录、导出、恢复、主题设置 |

### 3.3 文件页面

默认使用高密度列表，列包括名称、类型、大小、修改时间、来源和标签。图片目录可切换网格视图。单击资源只更新选择状态和详情，双击文件夹进入目录，双击文件使用系统默认应用打开。

右键菜单包括打开、在资源管理器中显示、复制路径、重命名、移动、收藏、标签和移入回收站。拖入资源后显示导入条，用户选择复制到仓库或保留原位置，应用记住上次选择。

### 3.4 富文本页面

页面采用结构化块编辑模型，支持标题、段落、列表、引用、分隔线、代码块、图片和文件附件。左侧页面树可折叠，中间是无边框编辑画布，顶部显示标题、收藏、更多操作和保存状态。

页面内容保存为结构化 JSON，同时保存纯文本摘要。附件只保存资源关系，不把大文件二进制写入页面正文。

### 3.5 代码项目页面

项目默认引用外部目录，不复制整个项目。项目文件树按需展开，默认忽略 `.git`、`node_modules`、`target`、`dist` 和缓存目录。中间为带行号和语法高亮的代码编辑器，支持复制、查找、跳转行和打开外部编辑器。

代码采用手动保存。编辑器打开文件时记录文件大小、修改时间和内容哈希。保存前再次读取磁盘指纹：未变化时写回；检测到外部变化时显示比较差异、覆盖、放弃本地修改、另存为和保留草稿等选项。

撤销和重做保存在当前编辑会话，不将每次按键写入数据库。

## 4. 架构设计

### 4.1 分层

```text
React + TypeScript
    |
    | Tauri IPC / Events
    v
Rust 应用核心
    |- 文件服务
    |- 导入服务
    |- 索引服务
    |- 监听服务
    |- 缩略图服务
    |- 预览服务
    |- 页面服务
    |- 项目服务
    |- 编辑服务
    |- 备份服务
    `- 任务服务
    |
    v
SQLite + rusqlite
    |
    v
本地文件仓库、缩略图缓存、页面附件和备份包
```

React 负责展示、交互、编辑器状态和用户操作反馈。Rust 负责文件系统访问、数据库写入、后台任务、文件监听、哈希、缩略图和备份。所有长任务通过事件向前端发送进度、状态和错误。

### 4.2 Rust 目录

```text
src-tauri/src/
|- commands/
|  |- resources.rs
|  |- import.rs
|  |- pages.rs
|  |- projects.rs
|  |- editor.rs
|  |- tasks.rs
|  `- backups.rs
|- db/
|  |- connection.rs
|  |- migrations.rs
|  |- models.rs
|  `- repositories.rs
|- domain/
|  |- resource.rs
|  |- page.rs
|  |- project.rs
|  `- task.rs
|- services/
|  |- file_service.rs
|  |- import_service.rs
|  |- index_service.rs
|  |- watcher_service.rs
|  |- thumbnail_service.rs
|  |- preview_service.rs
|  |- page_service.rs
|  |- project_service.rs
|  |- editor_service.rs
|  `- backup_service.rs
`- events.rs
```

### 4.3 React 目录

```text
src/
|- app/
|- routes/
|- layouts/
|- components/
|  |- AppShell
|  |- Sidebar
|  |- Topbar
|  |- FileTable
|  |- FileGrid
|  |- DetailPanel
|  |- ImportQueue
|  |- PageEditor
|  |- ProjectExplorer
|  `- CodeEditor
|- features/
|  |- files
|  |- pages
|  |- projects
|  |- tasks
|  `- settings
|- stores/
|  |- workspaceStore.ts
|  |- selectionStore.ts
|  |- taskStore.ts
|  `- editorStore.ts
`- lib/
   |- tauri.ts
   |- formatters.ts
   `- shortcuts.ts
```

## 5. 数据库设计

数据库文件为 `workspace.db`，启动时执行：

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;
PRAGMA temp_store = MEMORY;
```

### 5.1 核心资源

```sql
CREATE TABLE resources (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('file', 'folder', 'page', 'project')),
    name TEXT NOT NULL,
    parent_id TEXT REFERENCES resources(id) ON DELETE CASCADE,
    is_favorite INTEGER NOT NULL DEFAULT 0,
    is_deleted INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE INDEX idx_resources_parent
ON resources(parent_id, is_deleted, name COLLATE NOCASE);

CREATE INDEX idx_resources_kind
ON resources(kind, is_deleted);
```

### 5.2 文件位置和元数据

```sql
CREATE TABLE resource_locations (
    id TEXT PRIMARY KEY,
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL CHECK(source_type IN ('managed', 'external')),
    path TEXT NOT NULL,
    canonical_path TEXT,
    file_size INTEGER,
    modified_at TEXT,
    created_at TEXT NOT NULL,
    last_verified_at TEXT,
    content_hash TEXT,
    hash_algorithm TEXT,
    is_available INTEGER NOT NULL DEFAULT 1,
    UNIQUE(source_type, canonical_path)
);

CREATE INDEX idx_locations_resource ON resource_locations(resource_id);
CREATE INDEX idx_locations_path ON resource_locations(canonical_path);
CREATE INDEX idx_locations_hash ON resource_locations(content_hash);

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
```

`managed` 表示文件已经复制到应用仓库，`external` 表示保留原始路径。文件本体始终位于文件系统，SQLite 只保存路径、索引和元数据。

### 5.3 页面和资源关系

```sql
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
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_page_blocks_order
ON page_blocks(page_id, parent_block_id, block_order);

CREATE TABLE resource_relations (
    source_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(source_id, target_id, relation_type)
);
```

### 5.4 标签和项目

```sql
CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE resource_tags (
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY(resource_id, tag_id)
);

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
```

### 5.5 编辑会话

```sql
CREATE TABLE editor_sessions (
    id TEXT PRIMARY KEY,
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    base_path TEXT NOT NULL,
    base_size INTEGER NOT NULL,
    base_modified_at TEXT,
    base_hash TEXT,
    draft_content TEXT NOT NULL,
    language TEXT,
    is_dirty INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_editor_sessions_resource
ON editor_sessions(resource_id, is_dirty);
```

该表只用于恢复未保存草稿和保存冲突，不承担代码版本控制。

### 5.6 任务、缩略图、设置和备份

```sql
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
    created_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE task_items (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    resource_id TEXT REFERENCES resources(id) ON DELETE SET NULL,
    source_path TEXT,
    status TEXT NOT NULL,
    error_message TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE thumbnails (
    resource_id TEXT PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
    cache_path TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    source_hash TEXT,
    generated_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'ready'
);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE backup_records (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    backup_type TEXT NOT NULL,
    database_version INTEGER NOT NULL,
    resource_count INTEGER,
    file_count INTEGER,
    created_at TEXT NOT NULL,
    status TEXT NOT NULL,
    error_message TEXT
);
```

## 6. 大规模文件策略

针对 10 万级资源，必须使用后台扫描、增量索引、批量事务、虚拟列表、按需缩略图和分页查询。

- 扫描任务在 Rust 后台线程执行。
- 每 500 到 2000 条资源提交一次事务。
- 先比较路径、大小和修改时间，再计算内容哈希。
- 目录树按需展开，不一次性加载全部后代。
- 文件列表使用游标分页或稳定的 `LIMIT/OFFSET` 替代全量查询。
- 文件监听事件需要去抖、合并和批量写入。
- 图片缩略图只在显示或预览时生成。
- 默认忽略 `.git`、`node_modules`、`target`、`dist` 和缓存目录。
- 所有长任务可暂停、取消、重试，并保留失败原因。

首版只做文件名、路径、类型、标签、收藏和时间搜索。全文搜索留到后续版本，可使用 SQLite FTS5 建立单独索引。

## 7. 备份和恢复

备份包建议采用以下结构：

```text
backup/
|- manifest.json
|- workspace.db
|- pages/
|- page-assets/
|- tags.json
`- file-index.json
```

备份类型分为：

- 元数据备份：数据库、页面、标签、关联和文件索引。
- 完整备份：元数据加托管文件和页面附件。

恢复时先读取 `manifest.json`，校验版本和文件清单，再迁移数据库、复制资源、恢复索引，最后重新验证路径和哈希。

## 8. 错误和一致性

文件系统操作成功后再提交数据库变更。数据库写入失败时，需要把操作记录为可重试任务，避免出现数据库指向不存在文件的状态。删除采用软删除，回收站清理任务负责真正删除文件。

外部引用失效时保留资源记录，并标记 `is_available = 0`。用户可以重新定位文件，系统更新路径和验证时间。

## 9. MVP 验收标准

- 可以拖拽文件和文件夹导入。
- 可以选择复制到仓库或引用原路径。
- 文件夹、文件、页面和项目可以统一收藏。
- 文件页支持列表、网格、排序、筛选和右侧详情。
- 图片和文本文件可以预览。
- 可以新建富文本页面并插入文件附件。
- 可以导入代码目录，展示文件树和 README。
- 可以编辑代码并通过 Ctrl+S 手动保存。
- 外部修改时不会直接覆盖本地草稿。
- 导入和扫描任务显示进度、错误和取消状态。
- 10 万级数据不会因为一次性渲染全部行而阻塞界面。
- 可以导出并恢复一键备份。
