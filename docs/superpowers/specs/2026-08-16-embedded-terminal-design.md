# 内置终端（基础版）设计

日期：2026-08-16
范围：基础终端（单会话、输入输出、resize、复制粘贴、启动目录、PowerShell/cmd 切换）

## 1. 目标

在 Orange 工作台内嵌一个真实 Shell 终端，替代用户切换到独立终端窗口。

首期（本次实现）：

- 前端 `/terminal` 路由，工作区样式与现有应用一致。
- 后端 ConPTY 会话：PowerShell（默认）/ cmd 可切换。
- 双向 IO：输出流（Rust → 前端）、输入流（前端 → Rust）。
- 窗口 resize 同步（cols/rows 自适应）。
- 复制粘贴、清屏、结束/重启会话、当前工作目录显示。
- 会话生命周期：页面关闭自动清理，应用退出兜底清理，无孤儿进程。

不在本次范围：多标签/分屏、Git Bash/WSL 探测、命令历史持久化、输出搜索、主题设置页、从文件页"在终端打开"入口（API 预留 `cwd` 参数）。

## 2. 技术选型

| 层 | 选型 | 理由 |
| --- | --- | --- |
| PTY 后端 | `portable-pty 0.8.1` | wezterm 出品，Windows 走 ConPTY，生产验证充分；**不用 0.9**（0.9 引入 `PSEUDOCONSOLE_INHERIT_CURSOR`，ConPTY 启动时发出 `\x1b[6n` 并阻塞 stdout 等待宿主响应，集成测试实测卡死） |
| 终端渲染 | `@xterm/xterm 6.0.0` + `@xterm/addon-fit 0.11.0` + `@xterm/addon-web-links 0.12.0` | VS Code 终端同款；6.0 内置 OSC 52、改进滚动与 IME |
| IPC 传输 | Tauri 2 `Channel<TerminalEvent>` | 双向流：spawn 时由前端传入 Channel，后端推送 `Output(bytes)` 与 `Exit`；输入经 invoke 写入 |

`portable-pty 0.8.1` 的 ConPTY 标志为 `RESIZE_QUIRK | WIN32_INPUT_MODE`，无 INHERIT_CURSOR 回归，与 xterm 交互可靠。

## 3. IPC 契约

### 命令（`commands/terminal.rs`）

```text
terminal_spawn({ shell: "powershell"|"cmd", cwd?: string, cols: u16, rows: u16, channel: Channel<TerminalEvent> })
  -> { session_id: u64, cwd: string, shell: string }

terminal_write({ session_id: u64, data: string })   // UTF-8 输入字节
terminal_resize({ session_id: u64, cols: u16, rows: u16 })
terminal_close({ session_id: u64 })                  // 结束进程并移除会话
terminal_list() -> Vec<TerminalSessionInfo>          // 存活会话（含 cwd/shell）
```

### 事件（Channel 内，不占全局事件名）

```rust
enum TerminalEvent {
    Output(Vec<u8>),          // tag="output"，原始字节，前端 new Uint8Array 写入 xterm
    Exit { code: Option<i32> } // tag="exit"
}
```

### 约束

- `shell` 仅允许白名单：`powershell`、`cmd`，避免任意命令注入；`cwd` 必须为已存在目录，否则回退用户主目录。
- 每个 `session_id` 由 `TerminalRuntime.next_id` 自增分配。
- 输入上限：单次 write 长度 ≤ 64KB，超限拒绝。

## 4. 后端结构

### `AppState` 增加

```rust
pub struct TerminalRuntime {
    pub sessions: Mutex<HashMap<u64, TerminalSession>>,
    pub next_id: AtomicU64,
}
```

### `services/terminal_service.rs`

```rust
pub struct TerminalSession {
    id: u64,
    cwd: PathBuf,
    shell: String,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Option<Box<dyn Child + Send + Sync>>>,
}

pub fn spawn_session(shell, cwd, cols, rows, channel) -> Result<u64, AppError>
pub fn write_session(id, data: &str) -> Result<(), AppError>
pub fn resize_session(id, cols, rows) -> Result<(), AppError>
pub fn close_session(state, id) -> Result<(), AppError>
pub fn shutdown_all(state) -> void   // 应用退出兜底
```

要点：

- 读取线程：`master.try_clone_reader()` 循环 `read`，将字节经 `channel.send(TerminalEvent::Output(...))` 推送；读到 EOF 后 `child.wait()` 拿退出码，发送 `Exit`，并从 sessions 移除自身。
- `spawn_command` 后立即 `drop(pair.slave)`；`CommandBuilder` 设置 `cwd` 与 `env("TERM","xterm-256color")`。
- 关闭顺序：先 `child.kill()`，再等读取线程结束，最后移除 session，避免并发写已关闭 channel。
- 兜底：`lib.rs` 的 `run()` 中处理 `RunEvent::Exit` 调用 `shutdown_all`。

### 依赖

```toml
portable-pty = "0.8.1"
```

## 5. 前端结构

```
src/features/terminal/
  lib/terminal.ts                 # 类型 + call 封装（spawn/write/resize/close/list）
  components/TerminalView.tsx     # xterm 实例封装：open/fit/onData/onResize/清理
  routes/TerminalPage.tsx         # 页面：工具栏 + TerminalView
```

- 路由：`src/app/App.tsx` 增加 `<Route path="/terminal" element={<TerminalPage />} />`。
- 导航：`Sidebar.tsx` 增加 `终端` 项（`SquareTerminal` 图标），置于"任务中心"之后。
- 页面挂载：创建 `Channel<TerminalEvent>`，`spawn` 成功后：
  - `term.onData(d => invoke terminal_write)`
  - `term.onResize` + ResizeObserver → `fit()` → `invoke terminal_resize`
  - `channel.onmessage` → `Output` 写 xterm / `Exit` 显示"进程已退出 (code)"并禁用输入
- 页面卸载：`terminal_close(session_id)`。
- 工具栏：Shell 下拉（PowerShell/cmd）、工作目录显示、清屏、结束会话、重启会话按钮。
- 样式追加到 `src/styles/app.css`（深色终端面板，与现有设计令牌一致）。

## 6. 安全边界

- Shell 白名单 + cwd 存在性校验。
- 不新增 capability（自定义 command 不受权限插件约束，仍走 `AppState` 校验）。
- 进程回收：kill → 读取线程 EOF → 会话移除；应用退出 `shutdown_all` 兜底，杜绝 conhost 孤儿。
- 输入长度限制，防止超大 payload。

## 7. 文件变更清单

| 文件 | 变更 |
| --- | --- |
| `src-tauri/Cargo.toml` | + `portable-pty = "0.8.1"` |
| `src-tauri/src/services/terminal_service.rs` | 新建 |
| `src-tauri/src/services/mod.rs` | + `pub mod terminal_service;` |
| `src-tauri/src/commands/terminal.rs` | 新建 |
| `src-tauri/src/commands/mod.rs` | + `pub mod terminal;` |
| `src-tauri/src/lib.rs` | `TerminalRuntime`、注册命令、Exit 兜底 |
| `package.json` | + xterm 三个包 |
| `src/features/terminal/**` | 新建（3 个文件） |
| `src/app/App.tsx` | + `/terminal` 路由 |
| `src/components/Sidebar.tsx` | + 终端导航 |
| `src/styles/app.css` | + 终端样式 |

## 8. 里程碑与验证

1. M1 依赖与骨架：Cargo/npm 依赖、`terminal_service` 编译、单测（spawn→echo→kill）。
2. M2 命令层：4 个 command 注册，`terminal_list`/`terminal_close` 单测。
3. M3 前端：路由、导航、TerminalView、页面交互。
4. M4 集成验证（本机 Windows）：
   - `dir` / `Get-ChildItem` 输出与回显；
   - 中文输入输出；
   - 窗口缩放 resize 无撕裂；
   - `exit` 命令后退出状态提示；
   - 关闭页面/退出应用无残留进程（任务管理器核对 conhost/powershell）。

## 9. 备选方案（已否决）

- **portable-pty 0.9.x**：INHERIT_CURSOR 阻塞 stdout 回归，否决。
- **直接 windows crate 实现 ConPTY**：零新依赖但需自行处理 PseudoConsole/pipe/attribute list，易错且收益低，否决。
- **Tauri 全局事件传输出**：需常量命名与全局序列化，高频输出下与 Channel 无优势，且通道语义更贴合流，选用 Channel。

## 10. 扩展（2026-08-16 已实现）："在终端打开"入口

设计文档第 1 节"不在本次范围"中的"从文件页在终端打开"入口已补实现：

- **URL 契约**：`/terminal?cwd=<encodeURIComponent(绝对路径)>`。终端页 `useSearchParams` 读取并解码，spawn 时作为 `cwd` 传入；路径非法/不存在时后端回退用户主目录（既有白名单校验不变）。
- **文件页入口**：
  - 文件夹行内"在终端打开"图标按钮（`FileTable`，仅 `kind === "folder"` 显示）；
  - 文件夹右键菜单新增"在终端打开"项（`打开`/`重命名` 之后）；
  - 工具栏"在当前文件夹打开终端"按钮（面包屑当前目录，根目录时禁用）。
- **项目页入口**：项目行新增"在终端打开"图标按钮（悬停显示，与"删除项目"并列），以项目根目录为 cwd。
- **路径解析**：`src/lib/openResource.ts` 新增 `getResourcePath(id)`，取 `get_resource` 返回 locations 中首个可用物理路径。

实测（真实应用）：文件页行内按钮/工具栏/右键菜单 → 终端 cwd 显示并验证 shell `$PWD` 为 `E:\managed-files\游戏`；项目页入口 → `E:\work\毕业设计`；URL 中文路径编码解码正确；离开终端页自动回收会话，应用退出无残留进程。

## 11. 扩展（2026-08-16 已实现）：多标签

设计文档第 1 节"不在本次范围"中的"多标签"已补实现（后端无需改动，天然支持多会话）：

- **架构**：`TerminalPage` 维护 `tabs: TabMeta[]`（每标签独立 `sessionId/shell/cwd/seq`），只渲染激活标签的 pane（其余 `display:none` 保持 xterm 实例存活与滚动历史），标签切换即切换激活 pane。
- **标签栏**：标签标题（`PowerShell N`/`CMD N`）+ busy 旋转指示 + 关闭按钮；`+` 新建标签（使用工具栏下拉选择的 Shell，已有标签 Shell 固定）；关闭全部后显示空状态与"新建终端"。
- **工具栏语义**：Shell 下拉改为"新标签使用的 Shell"；清屏/结束/重启作用于激活标签；cwd 显示激活标签的工作目录。
- **竞态防护**：`spawnTab` 增加同步防重入集合（`spawningRef`）——StrictMode 下 TerminalView effect 同步双跑时 `busy` 状态尚未生效会重复 spawn，导致同一标签泄漏两个会话，实测修复前进入终端页产生 2 个 powershell、修复后 1 个。
- **URL cwd**：`/terminal?cwd=...` 仍作用于首标签，`+` 新建标签用默认目录。

实测（真实应用 + Windows 进程核对）：进入终端页 1 个会话；`+` 后 2 个独立 powershell（`$PID` 探针分别返回各自 PID）；标签切换后输入正确路由到对应会话；关闭标签回收对应会话；导航离开回收全部；应用退出无残留。

## 12. 扩展（2026-08-16 已实现）：Git Bash / WSL 探测

设计文档第 1 节"不在本次范围"中的"Git Bash/WSL 探测"已补实现：

- **后端**：`terminal_service` 新增 `ShellSpec { program, args }`，白名单扩展为 `powershell / cmd / gitbash / wsl`：
  - `gitbash`：探测 PATH 中的 `bash.exe`，其次 `Program Files\Git\bin|usr\bin\bash.exe`，spawn 参数 `--login -i`；
  - `wsl`：校验 `System32\wsl.exe` 且 `wsl --list --quiet` 有发行版，spawn 时追加 `--cd <cwd>` 让 Windows 侧工作目录生效；
  - 新增 `terminal_list_shells` 命令返回 `ShellInfo { id, label, available }`（内置两个恒可用）。
- **前端**：Shell 下拉改为动态渲染后端探测列表（不可用的不显示），默认仍为 PowerShell；失败时回退内置两项。
- **兼容处理**：外部并行进程将 `preview_service.rs` 重构为端口预览并删除了仍被引用的 `read_text_preview`，临时补充兼容实现恢复编译（标注说明）。

实测（本机已装 Git for Windows + WSL Ubuntu）：

- 下拉显示全部 4 项（PowerShell/CMD/Git Bash/WSL）；
- 选择 Git Bash 新建标签：`bash.exe` 会话启动；
- PTY 回环测试（临时用例，验证后移除）：Git Bash `echo GITBASH_OK` 与 WSL `echo WSL_OK` 均收到回显，输入输出链路正常；
- `cargo test` 全量 206 passed / 0 failed；前端构建通过。

## 13. 扩展（2026-08-16 已实现）：输出搜索

设计文档第 1 节"不在本次范围"中的"输出搜索"已补实现：

- **依赖**：`@xterm/addon-search`（0.16.0）。
- **TerminalView**：加载 `SearchAddon`，handle 暴露 `search`；新增 `onSearchResults` 回调把 `onDidChangeResults`（resultIndex/resultCount）转发给父组件。
- **TerminalPage**：
  - 工具栏新增"搜索输出（Ctrl+F）"按钮，点击展开搜索栏（输入框自动聚焦）；
  - 输入增量搜索（`incremental: true`），Enter/Shift+Enter 或按钮上一个/下一个导航，Esc 或关闭按钮退出并清理所有标签的高亮；
  - 计数显示 `当前/总数`（无匹配显示 `0/0`）；
  - 快捷键在捕获阶段拦截 Ctrl+F（避免传给 shell）与 Esc；切换标签时对新的激活标签重新搜索。
- **重要 bug 修复**：初次实现时计数恒为 `0/0`。排查 `addon-search` 源码发现 `_fireResults(!!e?.decorations)` 在未传 `decorations` 选项时 `fireResultsChanged(false)` 直接返回、不触发 `onDidChangeResults`，计数因此无法更新。修复：所有 `findNext/findPrevious` 调用统一传入 `decorations`（含匹配/当前匹配背景与 overview ruler 配色），同时获得高亮效果。
- 样式：搜索栏内联于工具栏（`terminal-search`），深色主题适配。

实测：搜索栏打开/聚焦/输入/导航按钮/关闭全流程正常；计数恒 `0/0` 的现象与源码中"缺 decorations 不触发结果事件"完全吻合，修复（源码级确认）后事件必触发。测试期间宿主窗口焦点被外部进程反复抢占，未能完成计数 >0 的端到端目视确认。
