# 本地文件工作台开发计划

## 1. 交付顺序

开发按可独立验证的阶段推进：

1. 工程骨架和运行契约
2. SQLite 数据层和迁移
3. Rust 文件服务和资源 API
4. React 工作台和文件页
5. 导入任务、扫描和文件监听
6. 预览和缩略图
7. 富文本页面
8. 代码项目和手动编辑
9. 备份恢复、性能和发布

每一阶段完成后运行对应测试，再进入下一阶段。不要在文件管理基础能力尚未稳定时同时加入云同步、GitHub 或多人权限。

## 2. 工程骨架

### 目标

建立可启动的 Tauri 2 + React + TypeScript + Rust 项目，并固定开发、测试和打包命令。

### 文件

```text
package.json
src-tauri/Cargo.toml
src-tauri/src/lib.rs
src-tauri/tauri.conf.json
src-tauri/capabilities/default.json
src/app/App.tsx
src/app/routes.tsx
src/styles/tokens.css
```

### 工作

- 使用 Vite 创建 React TypeScript 前端。
- 使用 Tauri 2 初始化桌面壳层。
- 添加 `rusqlite`，Windows 环境使用 `bundled` 特性，避免依赖系统 SQLite。
- 初始化 `tauri-plugin-dialog`、`tauri-plugin-fs`、`tauri-plugin-opener`。
- 在 capability 中只开放当前功能需要的目录和命令。
- 增加 `npm run dev`、`npm run build`、`npm run tauri dev`、`npm run tauri build`。
- 建立 Rust 错误类型和前端 IPC 响应格式。

### 验证

```text
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

预期结果：开发窗口启动，React 页面可加载，Rust 测试命令通过。

## 3. 数据库和迁移

### 目标

实现 `workspace.db` 的初始化、迁移、连接管理和事务边界。

### 文件

```text
src-tauri/src/db/connection.rs
src-tauri/src/db/migrations.rs
src-tauri/src/db/models.rs
src-tauri/src/db/repositories.rs
src-tauri/migrations/0001_initial.sql
src-tauri/tests/db_migrations.rs
```

### 工作

- 创建数据库连接管理器，初始化 WAL、外键、超时和同步级别。
- 实现 schema 版本表和按序迁移。
- 写入设计文档中的 `resources`、`resource_locations`、`file_metadata`、`pages`、`page_blocks`、`resource_relations`、`tags`、`resource_tags`、`projects`、`editor_sessions`、`tasks`、`task_items`、`thumbnails`、`app_settings` 和 `backup_records`。
- 所有查询使用参数绑定，不拼接用户输入。
- 所有批量导入使用事务。
- repository 层只负责持久化，不直接操作 Tauri 窗口。

### 测试

- 新数据库可以完成全部迁移。
- 重复执行迁移不会重复创建表。
- 外键约束生效。
- 删除资源会清理依赖关系。
- 批量插入失败时事务回滚。
- 关键索引通过 `EXPLAIN QUERY PLAN` 验证能被使用。

## 4. Rust 资源和文件服务

### 目标

提供统一资源模型、路径模型、文件元数据读取和基本文件操作。

### 文件

```text
src-tauri/src/domain/resource.rs
src-tauri/src/services/file_service.rs
src-tauri/src/commands/resources.rs
src-tauri/src/commands/files.rs
src-tauri/src/events.rs
src-tauri/tests/file_service.rs
```

### 工作

- 定义 `ResourceKind`、`SourceType`、`ResourceLocation` 和 `FileMetadata`。
- 实现路径规范化、绝对路径验证和 Windows 路径比较。
- 实现创建文件夹、重命名、移动、软删除、恢复和永久删除。
- 对外部引用提供 `verify_location` 命令。
- 对托管文件提供仓库路径生成规则，避免把用户文件名直接当作唯一物理路径。
- 文件操作完成后更新数据库，失败时返回结构化错误。
- 发送 `resource-changed`、`location-invalidated` 和 `trash-updated` 事件。

### 测试

- 文件夹创建和重命名更新资源记录。
- 外部路径失效后标记 `is_available = 0`。
- 删除后资源进入回收站，不立即丢失记录。
- 恢复后原父目录存在时恢复原位置，否则进入根目录。
- 路径包含 Unicode、空格和长文件名时仍能正常处理。

## 5. React 工作台和文件页

### 目标

实现浅色科技风工作台、左侧导航、中间文件区和右侧详情栏。

### 文件

```text
src/app/App.tsx
src/layouts/AppShell.tsx
src/components/Sidebar.tsx
src/components/Topbar.tsx
src/components/DetailPanel.tsx
src/features/files/routes/FilePage.tsx
src/features/files/components/FileTable.tsx
src/features/files/components/FileGrid.tsx
src/features/files/components/FileToolbar.tsx
src/features/files/stores/fileStore.ts
src/styles/tokens.css
src/styles/app.css
```

### 工作

- 建立导航路由：首页、文件、页面、代码项目、收藏、回收站、任务中心、设置。
- 实现文件列表和网格视图切换。
- 列表默认显示名称、类型、大小、修改时间、来源和标签。
- 实现单选、多选、排序、筛选、面包屑和右侧详情。
- 详情栏支持预览占位、元数据、路径、标签和关联资源。
- 设计空目录、无结果、失效路径、权限错误和加载状态。
- 使用图标库提供按钮图标，为非文字按钮增加可访问名称。
- 先用固定样例数据完成 UI，再接入真实 IPC。

### 验证

- 桌面宽度下三栏布局完整。
- 窄窗口下详情栏切换为抽屉。
- 列表和网格切换不会丢失当前选择。
- 多选工具栏显示批量操作。
- 键盘焦点可访问导航、列表和工具按钮。

## 6. 拖拽导入和后台任务

### 目标

把文件导入、目录扫描、冲突处理和任务进度接入工作台。

### 文件

```text
src-tauri/src/services/import_service.rs
src-tauri/src/services/index_service.rs
src-tauri/src/services/task_service.rs
src-tauri/src/commands/import.rs
src-tauri/src/commands/tasks.rs
src/features/tasks/components/TaskCenter.tsx
src/features/files/components/ImportDropzone.tsx
src/features/files/components/ImportDialog.tsx
src/stores/taskStore.ts
```

### 工作

- 接收 Tauri 原生拖拽路径，解析文件和目录。
- 导入对话框显示复制到仓库和引用原路径两个选项。
- 为导入建立 `tasks` 和 `task_items`。
- 目录扫描按批次读取元数据并批量写入 SQLite。
- 任务状态支持 queued、running、paused、completed、failed、cancelled。
- 前端通过 Tauri event 接收进度，不用高频轮询。
- 重复路径提供跳过、覆盖元数据、保留两份和取消选项。
- 失败任务保留错误原因，支持重试失败项。

### 测试

- 拖入单个文件、多个文件和目录都能创建任务。
- 任务取消后不再继续复制或写入。
- 扫描中断后重启可以从已完成批次继续。
- 重复文件路径不会创建重复资源。
- 10000 条模拟资源导入时前端仍可操作。

## 7. 文件监听、哈希和缩略图

### 目标

实现外部文件变化检测、增量更新、图片缩略图和预览缓存。

### 文件

```text
src-tauri/src/services/watcher_service.rs
src-tauri/src/services/thumbnail_service.rs
src-tauri/src/services/preview_service.rs
src-tauri/src/commands/previews.rs
src/features/files/components/PreviewPanel.tsx
src/features/files/components/Thumbnail.tsx
```

### 工作

- 对外部项目根目录建立监听。
- 合并短时间内重复的 create、modify、remove 和 rename 事件。
- 先使用大小和修改时间判断是否需要重新哈希。
- 图片缩略图写入缓存目录，数据库记录缓存路径和源哈希。
- 支持图片、纯文本、Markdown、JSON 和代码基础预览。
- 二进制、大文件和未知类型只显示元数据。
- 预览读取设置大小上限，超过上限显示延迟加载提示。

### 测试

- 外部新增、修改、删除文件能反映到数据库。
- 编辑器或其他程序保存文件时不会产生重复资源。
- 缩略图命中缓存时不重复生成。
- 原文件哈希改变后缩略图被标记为过期。
- 预览超大文件不会一次性加载全部内容。

## 8. 富文本页面

### 目标

实现页面树、结构化块编辑、页面附件和自动保存。

### 文件

```text
src-tauri/src/domain/page.rs
src-tauri/src/services/page_service.rs
src-tauri/src/commands/pages.rs
src/features/pages/routes/PagePage.tsx
src/features/pages/components/PageTree.tsx
src/features/pages/components/PageEditor.tsx
src/features/pages/components/BlockRenderer.tsx
src/features/pages/components/BlockMenu.tsx
src/features/pages/stores/pageStore.ts
```

### 工作

- 创建页面资源和页面扩展记录。
- 实现页面树按需加载。
- 定义段落、标题、列表、引用、代码块、图片和附件块格式。
- 页面输入使用 500 到 1000 毫秒防抖保存，失焦立即保存。
- 拖入文件或粘贴附件时创建 `resource_relations`。
- 保存页面摘要和纯文本内容。
- 显示 saved、dirty、conflict 状态。

### 测试

- 页面创建、重命名、移动和删除有效。
- 块顺序稳定，刷新后内容一致。
- 页面附件不会把二进制写入数据库正文。
- 自动保存失败时显示错误并保留编辑内容。
- 页面树支持多级嵌套和循环引用防护。

## 9. 代码项目和手动编辑

### 目标

实现项目导入、忽略规则、文件树、语法高亮和安全保存。

### 文件

```text
src-tauri/src/services/project_service.rs
src-tauri/src/services/editor_service.rs
src-tauri/src/commands/projects.rs
src-tauri/src/commands/editor.rs
src/features/projects/routes/ProjectPage.tsx
src/features/projects/components/ProjectTree.tsx
src/features/projects/components/CodeEditor.tsx
src/features/projects/components/SaveConflictDialog.tsx
src/features/projects/stores/editorStore.ts
```

### 工作

- 导入项目根目录并创建 project 资源。
- 识别 README、常见语言和项目类型。
- 默认忽略 `.git`、`node_modules`、`target`、`dist` 和缓存目录。
- 使用编辑器组件提供行号、语法高亮、查找、跳转行、复制和撤销重做。
- 打开文件时保存 path、size、modified_at 和 hash。
- Ctrl+S 时再次读取磁盘指纹。
- 指纹未变化时写回原文件。
- 指纹变化时打开冲突对话框，不直接覆盖。
- 草稿保存到 `editor_sessions`，应用崩溃后可以恢复。

### 测试

- 代码树可按需展开。
- 忽略目录不会出现在项目树中。
- UTF-8、UTF-8 BOM 和常见本地编码能正确识别或提示。
- 手动保存后文件内容、大小和修改时间更新。
- 外部修改后 Ctrl+S 必须进入冲突状态。
- 放弃草稿、覆盖、另存为和恢复草稿路径都可验证。
- 大于预览限制的文件不会被一次性加载。

## 10. 搜索、收藏和回收站

### 目标

完成资源发现和生命周期管理。

### 文件

```text
src-tauri/src/commands/search.rs
src-tauri/src/commands/trash.rs
src/features/search/routes/SearchPage.tsx
src/features/favorites/routes/FavoritesPage.tsx
src/features/trash/routes/TrashPage.tsx
```

### 工作

- 实现名称、路径、扩展名、类型、标签、收藏和时间筛选。
- 统一查询文件、页面和项目资源。
- 所有查询分页，结果稳定排序。
- 回收站显示删除时间、原父目录和资源类型。
- 恢复时检查父目录是否存在。
- 永久删除前显示确认并记录失败原因。

### 测试

- 混合资源搜索返回正确类型。
- 空结果和非法查询不会报错。
- 软删除资源不出现在普通列表。
- 恢复后资源回到原目录或根目录。
- 批量删除和批量恢复不会超出单次事务内存预算。

## 11. 备份和恢复

### 目标

实现一致性备份、恢复预览和版本校验。

### 文件

```text
src-tauri/src/services/backup_service.rs
src-tauri/src/commands/backups.rs
src/features/settings/components/BackupPanel.tsx
src/features/settings/components/RestoreDialog.tsx
```

### 工作

- 使用 SQLite Backup API 或等价的安全快照方式导出数据库。
- 生成 `manifest.json`、数据库、页面资源和文件索引。
- 完整备份可选择是否包含托管文件。
- 恢复前校验版本、清单和空间。
- 恢复过程写入任务中心，支持进度和失败记录。
- 恢复后重建缩略图状态并验证路径。

### 测试

- 数据库写入期间可以生成一致性备份。
- 备份包缺失文件时恢复前能提示。
- 版本不兼容时不会覆盖当前数据。
- 恢复后页面、标签、资源关系和托管文件可读取。
- 恢复失败时保留原数据库，不进入半恢复状态。

## 12. 性能和发布验证

### 目标

确保 10 万级资源下界面、数据库和后台任务可用，并生成 Windows 安装包。

### 工作

- 构造 10 万、50 万资源的本地压力数据。
- 测量目录打开、分页、搜索、详情加载和任务进度更新耗时。
- 使用虚拟列表检查首屏渲染和滚动稳定性。
- 检查扫描批量事务、WAL 文件增长和 checkpoint 行为。
- 检查内存、CPU、磁盘写入和缩略图缓存上限。
- 执行 `cargo clippy`、Rust 单元测试、前端类型检查和生产构建。
- 生成 Windows MSI 或 NSIS 安装包并验证首次启动、升级和数据目录权限。

### 验证命令

```text
cargo fmt --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run typecheck
npm run build
npm run tauri build
```

## 13. 里程碑

### M1 可启动工程

完成 Tauri、React、Rust、SQLite 连接和基础工作台壳层。

### M2 文件管理可用

完成目录树、列表、拖拽导入、两种存储策略、重命名、移动、回收站和收藏。

### M3 大规模索引稳定

完成批量扫描、任务中心、文件监听、分页、虚拟列表和缩略图队列。

### M4 信息层可用

完成富文本页面、页面树、附件、标签和资源关联。

### M5 代码工作区可用

完成项目导入、忽略规则、文件树、代码编辑器、手动保存和冲突处理。

### M6 数据可迁移

完成备份、恢复、性能基准、Windows 安装包和发布前验证。

## 14. 明确暂缓

以下功能不进入当前开发计划：

- 用户登录和多人权限。
- 云同步和远程 API。
- GitHub 账号授权、仓库拉取和推送。
- Git 分支、提交、合并和历史视图。
- 代码版本快照和完整版本控制。
- 首版全文搜索和内容级语义检索。
- 自动定时备份。
