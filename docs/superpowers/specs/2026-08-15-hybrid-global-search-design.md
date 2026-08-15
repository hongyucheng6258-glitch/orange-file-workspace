# 全电脑混合搜索 设计文档

日期：2026-08-15
状态：已批准（用户确认方案）

## 1. 背景与目标

NexusFile 当前搜索只查询 SQLite `resources` 和 `resource_locations`，覆盖应用内文件、文件夹、页面和项目，无法发现未导入 NexusFile 的系统文件或已安装应用。

本功能将搜索扩展为全电脑统一入口。首版优先保证首屏体验：Windows 系统索引和应用来源先返回结果，后台扫描所有本地固定磁盘并持续补齐遗漏，NexusFile 自有资源继续参与聚合。

目标范围：

- 搜索所有可访问的本地文件和文件夹，匹配名称与路径。
- 搜索传统 Win32 桌面应用、开始菜单快捷方式和 Microsoft Store 应用。
- 融合 NexusFile 页面、项目、收藏和已有资源。
- 支持打开文件、打开文件夹、启动应用和打开所在位置。
- Windows Search 不可用或后台首轮索引未完成时，仍提供可理解的降级结果与覆盖状态。

## 2. 方案选择

| 方案 | 首屏速度 | 全盘覆盖 | 维护成本 | 结论 |
|---|---:|---:|---:|---|
| 仅 Windows Search | 最快 | 取决于系统索引范围 | 低 | 覆盖不稳定 |
| 仅自建索引 | 首轮较慢 | 高 | 高 | 首次体验差 |
| 混合模式 | 快 | 高 | 中 | 采用 |

混合模式包含两个检索层：

1. 即时层并行查询 Windows Search、应用来源和 NexusFile 资源，尽快返回首批结果。
2. 后台层低优先级扫描所有本地固定磁盘，写入独立索引并补充系统索引遗漏。

聚合层统一去重、排序和分批返回。每次查询生成唯一 `search_id`，新查询会取消旧查询，防止异步结果串线。

## 3. 搜索范围

### 3.1 文件与文件夹

- 默认扫描所有本地固定磁盘。
- 首版只索引名称和路径，不读取文件正文。
- 记录类型、扩展名、大小、修改时间、卷标识和扫描代次。
- 扫描隐藏文件，但不跟随符号链接和目录联接。
- 跳过回收站、系统卷信息、无权限目录和 NexusFile 自身索引目录。
- 不申请管理员权限绕过 Windows ACL；“全盘”指当前用户可访问范围。

### 3.2 应用

应用来源独立维护，覆盖：

- 用户和系统开始菜单快捷方式。
- App Paths 注册表项。
- 卸载注册表中的传统桌面应用信息。
- Shell AppsFolder 和 Microsoft Store 应用 AUMID。

传统应用按规范化启动目标去重，Store 应用按 AUMID 去重。磁盘中的普通 EXE、BAT 和 CMD 不自动归类为“应用”，仍作为文件结果返回，避免应用列表噪声。

### 3.3 NexusFile 资源

保留现有 `search_resources` 的数据语义。页面、项目、收藏和已导入资源参与统一排序；系统索引文件不会写入 `resources`，避免改变导入、收藏、回收站和项目层级行为。

## 4. 数据设计

新增独立的系统搜索表，不复用业务资源表：

```text
system_search_entries
- id
- canonical_path
- display_name
- entry_kind          file | folder
- extension
- file_size
- modified_at
- volume_id
- scan_generation
- is_offline
- indexed_at
```

```text
system_search_apps
- id
- app_kind            win32 | shortcut | store
- display_name
- launch_target
- canonical_target
- aumid
- icon_source
- install_location
- updated_at
```

```text
system_search_scan_state
- volume_id
- root_path
- status              pending | scanning | paused | completed | error | offline
- scan_generation
- checkpoint
- indexed_count
- skipped_count
- last_error
- started_at
- completed_at
```

名称精确和前缀匹配使用普通索引。三字符以上的包含匹配使用 SQLite FTS5 trigram；短关键词采用限量名称匹配。数据库迁移需先验证当前 bundled SQLite 是否启用 trigram tokenizer；若不可用，则降级为 FTS5 unicode61 前缀索引加转义后的 `LIKE`，不阻塞首版交付。

系统搜索索引与 NexusFile 主资源表逻辑隔离。索引损坏时可删除并重建，不影响业务资源数据。

## 5. 后台扫描

### 5.1 首轮扫描

- 应用启动后枚举本地固定磁盘，为每个卷创建持久化扫描任务。
- 默认一个扫描工作线程，按目录批次遍历，使用批量事务写入 SQLite。
- 扫描进度持久化；应用退出或崩溃后从检查点继续。
- 新一轮扫描写入新的 `scan_generation`，只有整卷完成后才清理旧代次记录，避免中断扫描误删可用索引。

### 5.2 资源调度

- 扫描线程使用低优先级策略，限制批次大小和写入频率。
- CPU 或磁盘繁忙时降速；电脑空闲时提高吞吐。
- 使用电池供电时默认进一步降速，但不完全停止。
- 用户可暂停、继续、重建索引，并管理排除目录。
- 搜索查询优先于扫描写入，避免数据库锁阻塞交互。

### 5.3 增量维护

- 首轮完成后监听文件创建、修改、重命名、移动和删除事件。
- 文件监听只负责快速更新，不作为唯一一致性来源。
- 事件丢失、监听异常或非正常退出时，将对应目录标记为待复查。
- 低频按卷校验修复遗漏和陈旧记录。
- 可移动磁盘不在默认首轮范围；已索引固定磁盘离线时保留记录并标记离线。

## 6. 即时搜索与聚合

一次查询并行执行：

- Windows Search 文件与文件夹查询。
- 应用索引查询。
- NexusFile 资源查询。
- 自建系统索引查询。

统一结果模型：

```text
GlobalSearchHit
- key                  稳定去重键
- kind                 file | folder | app | page | project
- name
- path
- source               windows | local_index | app_index | nexus
- matched_field        name | path
- modified_at
- icon_source
- is_offline
- actions
- score
```

去重规则：

- 文件和文件夹以大小写无关的规范化路径为主键。
- Win32 应用以规范化启动目标为主键。
- Store 应用以 AUMID 为主键。
- NexusFile 外部资源若路径与系统结果相同，只展示一次，并保留 NexusFile 收藏、资源 ID 等附加信息。

排序优先级：

1. 完整名称匹配。
2. 名称前缀匹配。
3. 名称包含匹配。
4. 路径包含匹配。
5. 同分时应用、页面和项目略优先。
6. 最近打开和修改时间只作为次级加分，不覆盖主要相关性。

每个来源独立超时，单一来源失败不会清空其他结果。首批结果返回后，后续批次按稳定键原位合并，不重置滚动位置。

## 7. 后端接口

建议新增独立 `global_search` 命令模块，保留现有 `search_resources` 供内部复用：

- `start_global_search(query, filters, limit) -> { search_id, hits, sources }`
- `cancel_global_search(search_id) -> ()`
- `get_search_index_status() -> SearchIndexStatus`
- `pause_search_index() -> ()`
- `resume_search_index() -> ()`
- `rebuild_search_index(volumes?) -> ()`
- `get_search_settings() -> SearchSettings`
- `update_search_settings(settings) -> SearchSettings`
- `open_search_result(key) -> ()`
- `reveal_search_result(key) -> ()`

事件：

- `global-search://batch`：后续结果批次、来源完成状态和错误摘要。
- `global-search://index-progress`：当前磁盘、扫描状态、已索引数和跳过数。

打开和定位操作由后端根据索引记录重新解析目标，不接受前端拼接的任意命令。文件存在性、结果类型和应用启动信息在执行前再次校验。

## 8. 前端交互

### 8.1 搜索入口

顶部栏继续导航到 `/search?q=...`，占位文案调整为“搜索文件、应用、页面、项目…”。搜索页保持 URL 与输入框同步。

输入采用 200ms 防抖，至少两个字符后自动查询；Enter 可立即触发。每次新查询取消旧 `search_id`，并继续使用前端请求序号作为第二层竞态保护。

### 8.2 结果页

结果类型包含“全部、应用、文件、文件夹、页面/项目”，并提供磁盘和扩展名筛选。首版不加入复杂查询语法。

结果行展示：

- 类型图标、名称和完整路径。
- 来源、修改时间或应用类型。
- 离线、索引补充中或来源失败状态。
- 双击/Enter 打开；右键提供打开所在位置和复制路径。

可执行文件仍按 Windows 默认关联启动。首次从搜索结果运行 EXE、BAT、CMD、COM 或 MSI 时显示确认，普通文档和已识别安装应用不额外确认。

### 8.3 索引状态

搜索页顶部提供紧凑状态入口，展示：

- 当前状态和扫描磁盘。
- 已索引数量、跳过数量和总体进度。
- 暂停、继续和重建索引。
- 排除目录管理。

Windows Search 关闭时显示“使用本地索引搜索”。首轮索引未完成时显示当前已覆盖卷和目录，不把暂时无结果表述为“全盘无匹配”。

## 9. 失败与降级

- Windows Search 不可用：应用索引、NexusFile 资源和自建索引继续工作。
- 首轮索引未完成：返回已覆盖范围结果，并显示扫描状态。
- 目录无权限：跳过并计数，不重复弹窗。
- 文件搜索后被删除：打开时提示目标不存在，并移除陈旧记录。
- 固定磁盘离线：保留记录并置灰；重新上线后校验更新。
- 索引库损坏：隔离系统搜索表并触发重建，不影响主资源库。
- 扫描中退出：保存检查点，下次启动继续。
- 结果过多：每批 50 条，默认最多展示 500 条，通过筛选缩小范围。

## 10. 性能目标

- Windows Search 可用时，常见查询首批结果目标不超过 300ms。
- Windows Search 不可用但本地索引可用时，首批结果目标不超过 500ms。
- 搜索过程中前端保持可交互，旧查询批次不能覆盖新查询。
- 后台扫描不得阻塞搜索、文件打开或电脑信息页面。
- 数据库批量写入和查询采用独立连接；必要时启用 WAL 和合理 busy timeout。

性能目标以典型本地 SSD、索引规模不超过 500 万条为首版验证基线。机械硬盘和超大文件量设备只要求功能正确、可暂停并提供真实进度，不承诺相同扫描完成时间。

## 11. 测试与验收

后端自动化测试：

- 路径规范化、大小写去重和同路径多来源合并。
- 完整、前缀、名称包含和路径包含的排序顺序。
- LIKE 特殊字符和 FTS 查询转义。
- 分批结果、来源超时、取消和旧 `search_id` 隔离。
- 扫描检查点恢复、扫描代次提交和中断不清理旧索引。
- 权限拒绝、目录联接循环、文件删除和磁盘离线处理。
- Win32、快捷方式和 Store 应用去重。

Windows 本机集成测试：

- Windows Search 可用和停用两种路径。
- 所有固定磁盘枚举、后台暂停/继续和退出恢复。
- 新增、重命名、移动和删除最终反映到结果。
- Win32 与 Store 应用可以搜索并启动。
- 打开文件、打开文件夹和打开所在位置行为正确。

前端验证：

- URL、输入框、筛选器和结果状态同步。
- 200ms 防抖、Enter 即时查询和旧请求取消。
- 即时结果先显示，后台批次原位合并且滚动位置稳定。
- 单一来源失败不会清空其他结果。
- 键盘选择、Enter 打开、右键动作和可执行文件确认有效。
- 索引未完成、暂停、离线和 Windows Search 不可用状态清晰可见。

验收条件：

- 同一文件不会因 Windows Search、本地索引和 NexusFile 来源重复展示。
- 首轮扫描期间应用其余功能可正常使用。
- 所有可访问固定磁盘最终完成索引或给出可诊断错误。
- 搜索结果不包含已确认删除的陈旧路径。
- 系统搜索索引可独立重建，不破坏 NexusFile 资源数据。

## 12. 分阶段交付

第一阶段建立统一结果模型、应用搜索、NexusFile 聚合、打开与定位动作，并接入 Windows Search 即时层。

第二阶段建立系统搜索索引表、固定磁盘后台扫描、进度状态和本地索引查询。

第三阶段加入文件变化监听、分卷校验、暂停/恢复、排除目录和故障降级。

第四阶段完善排序、键盘交互、性能调优、Windows 本机集成测试和全量回归。

首版不包含文件正文搜索、网络驱动器、可移动磁盘默认扫描、正则/高级查询语法、内容哈希去重或管理员权限扫描。