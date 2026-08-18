# C 盘清理 Rust 原生接入设计

日期：2026-08-18
状态：已批准

## 目标

在现有“电脑信息 > 工具”页加入 C 盘空间清理，提供安全与深度两种模式，默认安全模式。功能由 Rust 原生服务执行，不依赖外部 PowerShell。

## 交互

- 工具页顶部新增“C 盘空间清理”。
- 使用分段控件切换安全清理和深度清理，默认安全清理。
- 点击扫描后显示清理项、候选文件数、预计空间、风险和权限要求。
- 默认勾选低风险项；安全模式的回收站默认勾选。
- 点击清理后显示汇总确认；选中回收站时明确提示不可恢复。
- 完成后显示实际释放空间、删除数、跳过数及各项状态。

## 后端

新增 `services/c_drive_cleaner.rs`，对外提供：

- `scan(mode) -> Vec<CleanupScanItem>`
- `clean(mode, item_ids, confirm_recycle_bin) -> CleanupRunResult`

新增命令：

- `scan_c_drive_cleanup(mode)`
- `clean_c_drive_items(mode, item_ids, confirm_recycle_bin)`

调用方只能提交清理项 ID，不接受任意路径。扫描和清理共用同一白名单目录定义。

## 范围

安全模式包含用户临时文件、Windows 临时文件、图标缓存、缩略图缓存、Edge/Chrome/Firefox 普通缓存和 C 盘回收站。深度模式增加 Windows 错误报告、崩溃转储、Windows Update 下载缓存与 Delivery Optimization 缓存；需要管理员权限的项目默认不勾选。

浏览器只处理 Chromium profile 下的 `Cache`、`Code Cache`、`GPUCache`、`Service Worker/CacheStorage` 和 Firefox profile 下的 `cache2`。不处理 Cookie、密码、历史记录、Local Storage、IndexedDB、Sessions 或书签。

## 安全

- 固定 ID 到固定路径的白名单映射。
- 拒绝盘符根目录、用户目录、Windows 根目录和 profile 根目录。
- 不递归重解析点。
- 单次扫描限制候选文件数量。
- 被占用或无权限文件跳过并计数。
- 回收站必须传入独立确认标志。
- 高权限项在非管理员状态下返回“需要管理员权限”，不静默提权。

## 验收

- 安全模式返回 8 项，深度模式返回 12 项。
- 默认选择仅包含低风险项及已确认默认的回收站。
- 未确认回收站时拒绝执行该项。
- 未知清理项 ID 被拒绝。
- 前端可完成扫描、逐项选择、确认与结果展示。
- Rust 测试、前端测试、类型检查通过。
