# 电脑修复工具 设计文档

日期：2026-08-15
状态：已批准（用户确认方案）

## 1. 背景与目标

NexusFile「电脑信息」页已提供完整的系统信息查看能力（总览、性能、硬件、存储、传感器、网络、进程、服务、驱动、启动项、电源、安全、健康、报告）。

本功能在「电脑信息」页新增「工具」标签页，提供一组一键式系统修复/维护工具，覆盖日常遇到的图标异常、缓存膨胀、DNS 故障、文件管理器卡死等场景。全部本地执行，高风险项通过 UAC 提权并弹窗确认。

## 2. 范围

### 2.1 首批实现（低风险，普通权限可运行）

| 工具 | 实现方式 | 结果反馈 |
|---|---|---|
| 刷新桌面图标 | `SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, ...)` | 成功/失败 |
| 清理图标缓存 | 删除 `%LOCALAPPDATA%\IconCache.db` + `%LOCALAPPDATA%\Microsoft\Windows\Explorer\iconcache_*.db` | 删除文件数、释放字节；提示重启资源管理器完全生效 |
| 清理缩略图缓存 | 删除 `%LOCALAPPDATA%\Microsoft\Windows\Explorer\thumbcache_*.db` | 同上 |
| 清理临时文件 | 删除用户 `%TEMP%` 下可删文件（占用中跳过并计数） | 删除文件数、释放字节、跳过数 |
| 刷新 DNS 缓存 | `DnsFlushResolverCache`（DNSAPI.dll，extern 声明） | 成功/失败 |
| 重启资源管理器 | 定位 explorer 进程 → TerminateProcess → 重新启动 `explorer.exe` | 成功/失败（执行前强提示会关闭资源管理器窗口） |

### 2.2 高级工具（UAC 提权执行）

| 工具 | 执行方式 |
|---|---|
| 系统文件检查 | `ShellExecuteW(runas)` 打开提升 cmd 窗口执行 `cmd /k sfc /scannow` |
| 磁盘错误检查 | 提权执行 `cmd /k chkdsk C: /f`，提示下次重启生效 |
| 网络重置 | 提权执行 `cmd /k netsh winsock reset`，提示重启生效 |

- 非管理员点击时弹确认说明将弹出 UAC 授权框；拒绝授权则提示操作取消。
- 提权命令在独立提升窗口中运行（`cmd /k` 保持窗口），用户直接查看输出。

### 2.3 不在范围

- 注册表清理/修改启动项
- 系统还原点创建
- 磁盘分区操作

## 3. 技术方案

### 3.1 后端（`src-tauri/src/services/system_windows.rs` 新增）

```
ToolCleanResult { deleted_files: u64, freed_bytes: u64, skipped_files: u64 }
```

命令（`src-tauri/src/commands/system.rs`）：

- `refresh_desktop_icons() -> CommandResult<()>`：SHChangeNotify 刷新图标
- `clear_icon_cache() -> CommandResult<ToolCleanResult>`：清理图标缓存
- `clear_thumb_cache() -> CommandResult<ToolCleanResult>`：清理缩略图缓存
- `clear_temp_files() -> CommandResult<ToolCleanResult>`：清理用户临时文件
- `flush_dns_cache() -> CommandResult<()>`：DnsFlushResolverCache
- `restart_explorer() -> CommandResult<()>`：重启资源管理器
- `run_admin_tool(tool: String) -> CommandResult<()>`：UAC 提权执行（sfc / chkdsk / winsock），tool 白名单校验
- `get_admin_status() -> CommandResult<bool>`：IsUserAnAdmin（已有，新增命令封装）

实现要点：

- `SHChangeNotify`：windows crate 现成 API（Win32_UI_Shell）。
- `DnsFlushResolverCache`：windows 0.61 无此绑定，用 `unsafe extern "system"` 声明（与 DeviceIoControl 相同模式），返回 BOOL。
- 缓存清理：`std::fs` 递归/模式删除，逐文件 `remove_file`，失败（占用）记入 skipped；统计 freed_bytes。
- 临时文件清理：仅处理 `%TEMP%`（用户级），跳过当前进程正在使用的文件（remove 失败即跳过）。
- 重启资源管理器：`sysinfo` 枚举进程名 `explorer.exe` → `OpenProcess(PROCESS_TERMINATE)` → `TerminateProcess` → `CreateProcess`/`ShellExecuteW` 启动 `explorer.exe`。启动方式优先 `ShellExecuteW`（无需 CreateProcess 复杂参数）。
- UAC 提权：`ShellExecuteW(None, Some("runas"), "cmd.exe", Some("/k sfc /scannow"), None, SW_SHOWNORMAL)`。tool 参数白名单映射：
  - `sfc` → `cmd /k sfc /scannow`
  - `chkdsk` → `cmd /k chkdsk C: /f`（提示重启生效）
  - `winsock` → `cmd /k netsh winsock reset`（提示重启生效）

### 3.2 前端（`src/features/system/` 新增）

- `routes/SystemPage.tsx`：新增 TABS 项 `{ key: "tools", label: "工具", icon: Wrench }`
- `components/ToolsTab.tsx`：
  - 顶部身份提示条：管理员/普通用户（`get_admin_status`）
  - 工具卡片列表：图标、名称、描述、风险标签（低/中/高）、执行按钮
  - 每个工具：确认弹窗（`window.confirm`，中/高风险含额外警示文案）→ 执行 → 结果展示（toast/行内结果）
  - 页面底部「操作记录」列表：时间 + 工具 + 结果（成功/失败/取消），会话内保留
- `lib/types.ts`：新增 `ToolCleanResult` 类型
- `styles/app.css`：工具页样式（工具卡片、风险标签、结果区、操作记录）

### 3.3 注册

- `lib.rs` invoke_handler 注册 8 个新命令
- 前端 `types.ts` 命令调用封装（沿用 `call`）

## 4. 安全与确认

- 低风险工具：确认框文案为「确定要执行 X 吗？」
- 重启资源管理器：额外提示「将关闭所有资源管理器窗口，正在浏览的文件窗口会丢失」
- 高级工具：提示「将弹出 UAC 授权窗口，请确认以管理员身份运行」
- 所有操作不修改注册表启动项、不删除用户文档目录以外的用户数据
- 清理仅限系统缓存目录与用户临时目录

## 5. 测试

- 后端单元测试（`system_windows.rs` / `commands/system.rs`）：
  - `refresh_desktop_icons` 调用不崩溃
  - `clear_icon_cache` / `clear_thumb_cache` 返回合法结构（文件数 ≥ 0）
  - `clear_temp_files` 返回合法结构且不删除非临时目录
  - `flush_dns_cache` 返回成功（BOOLEAN 校验）
  - `run_admin_tool` 白名单外参数被拒绝
  - 清理函数对不存在目录返回空结果而非报错
- 前端构建通过；页面渲染验证（浏览器 DOM 检查）

## 6. 交付物

- `src-tauri`：8 个新命令 + 测试
- `src/features/system/components/ToolsTab.tsx` + 路由 + 样式
- 全量测试通过（现有 68 项 + 新增）
