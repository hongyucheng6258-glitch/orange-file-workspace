# 内置终端（基础版）实现计划

日期：2026-08-16
依据：`docs/superpowers/specs/2026-08-16-embedded-terminal-design.md`
环境：Windows 11（本机）、Tauri 2、React 19、Rust（edition 2021）

## 依赖决策

- `portable-pty = "0.8.1"`：wezterm 出品，Windows 走 ConPTY；避开 0.9 的 `PSEUDOCONSOLE_INHERIT_CURSOR` 阻塞 stdout 回归。
- `@xterm/xterm@^6.0.0`、`@xterm/addon-fit@^0.11.0`、`@xterm/addon-web-links@^0.12.0`：VS Code 终端同款（6.0 为 2025-12 稳定版）。
- IPC：Tauri 2 `Channel<TerminalEvent>`（前端 spawn 时传入，后端推送字节流与退出事件）。

## 任务分解

### M1 后端服务

**Task 1.1** `src-tauri/Cargo.toml` 增加 `portable-pty = "0.8.1"`（依赖块，dependencies 内）。

**Task 1.2** 新建 `src-tauri/src/services/terminal_service.rs`，实现 `TerminalSession`、`TerminalRuntime`（sessions map + next_id）、`spawn_session / write_session / resize_session / close_session / shutdown_all`、读取线程。

**Task 1.3** `src-tauri/src/services/mod.rs` 增加 `pub mod terminal_service;`。

**Task 1.4** 单元测试（spawn → 写 echo → 读输出 → kill），`cargo test` 通过。

### M2 命令层与状态接入

**Task 2.1** 新建 `src-tauri/src/commands/terminal.rs`：`terminal_spawn / terminal_write / terminal_resize / terminal_close / terminal_list`。

**Task 2.2** `src-tauri/src/commands/mod.rs` 增加 `pub mod terminal;`。

**Task 2.3** `src-tauri/src/lib.rs`：`AppState` 增加 `pub terminal: TerminalRuntime`；setup 初始化；`invoke_handler` 注册 5 个命令；`run()` 改为 `build() + run(callback)`，`RunEvent::Exit` 调 `shutdown_all`。

**Task 2.4** `cargo test` 全量通过。

### M3 前端

**Task 3.1** `npm install @xterm/xterm @xterm/addon-fit @xterm/addon-web-links`。

**Task 3.2** 新建 `src/features/terminal/lib/terminal.ts`：类型 + 5 个 invoke 封装。

**Task 3.3** 新建 `src/features/terminal/components/TerminalView.tsx`：xterm 实例、fit、onData、onResize、生命周期。

**Task 3.4** 新建 `src/features/terminal/routes/TerminalPage.tsx`：工具栏（shell 切换、cwd 显示、清屏、结束、重启）+ TerminalView + Channel 装配。

**Task 3.5** `src/app/App.tsx` 增加 `/terminal` 路由；`src/components/Sidebar.tsx` 增加"终端"导航。

**Task 3.6** `src/styles/app.css` 增加终端样式。

**Task 3.7** `npm run build` 通过。

### M4 集成验证（本机 Windows）

**Task 4.1** `npm run tauri dev`，验证：目录/回显、中文 I/O、resize、exit 状态、关闭清理（任务管理器核对无残留 powershell/conhost）。

## 关键代码

### terminal_service.rs（核心，完整实现见 Task 1.2）

```rust
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, SlavePty};
use tauri::ipc::Channel;

use crate::error::AppError;

#[derive(serde::Serialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum TerminalEvent {
    Output(Vec<u8>),
    Exit(Option<i32>),
}

pub struct TerminalSession {
    pub id: u64,
    pub cwd: PathBuf,
    pub shell: String,
    pub master: Mutex<Box<dyn MasterPty + Send>>,
    pub writer: Mutex<Box<dyn Write + Send>>,
    pub child: Mutex<Option<Box<dyn Child + Send + Sync>>>,
}

pub struct TerminalRuntime {
    pub sessions: Mutex<HashMap<u64, TerminalSession>>,
    pub next_id: AtomicU64,
}

impl Default for TerminalRuntime { ... }
```

（spawn/write/resize/close/shutdown_all 与读取线程的完整实现见代码提交，关键点：spawn 后 drop slave；读取线程 EOF 后从 map 移除、wait 退出码、发送 Exit；close 时 kill + wait；shutdown_all 遍历 kill。）

### lib.rs run() 改造

```rust
tauri::Builder::default()
    // ... 原样 ...
    .build(tauri::generate_context!())
    .expect("error while building tauri application")
    .run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            let state = app.state::<AppState>();
            crate::services::terminal_service::shutdown_all(&state);
        }
    });
```

## 验证命令

- 后端：`cd src-tauri && cargo test`（期望 terminal_service 相关用例通过，既有用例不回归）
- 前端：`npm run build`（tsc + vite 通过）
- 集成：`npm run tauri dev`（人工清单）

## 实现记录（2026-08-16，全部已落地并实测）

| 问题 | 根因 | 修复 |
| --- | --- | --- |
| 页面报 `Cannot read properties of null (reading 'cols')` | `useImperativeHandle` 工厂函数在 mount 时求值，xterm 实例尚未创建，句柄被固定为 null | 句柄改为 getter，每次访问实时取值 |
| `exit` 后 conhost.exe 残留为孤儿 | ConPTY 的 conhost 在 shell 退出后仍保持输出管道打开，读取线程阻塞在 `read()`，会话永不回收（ClosePseudoConsole 未被触发） | 新增监视线程：200ms 轮询 `child.try_wait()`，检测到退出即回收会话（drop master 触发 ClosePseudoConsole）并发送 Exit 事件 |
| StrictMode 双挂载导致每次进入终端页泄漏 1 个会话 | 两次 `start()` 并发，`mountedRef` 检查竞态使两个会话都存活 | 引入代际序号 `startSeqRef`，过期会话（序号不匹配）立即 `terminalClose` 回收 |

实测验证（真实 Tauri 应用 + Windows 进程核对）：

- 会话创建：Orange 派生 `powershell.exe` + headless `conhost.exe`，尺寸随窗口 fit（170×47 → 69×24）
- 输入链路：`dir`/echo 探针/中文（"中文终端测试"）均正确写入并执行
- resize：窗口缩放后 ConPTY 内 `$Host.UI.RawUI.WindowSize` 同步更新
- 重启/结束按钮：均正确终止子进程
- `exit`：监视线程回收会话，powershell + conhost 全部消失
- 应用退出：`RunEvent::Exit` → `shutdown_all`，无任何残留终端进程

构建与测试：`cargo test terminal_service::tests` 3/3 通过；前端 `build` + 4 个单测通过。
注：全量 `cargo test` 中有 12 个失败来自 2026-08-16 10:04 外部并行进程新增的 `run_confirmation.rs`（非本次终端任务代码）。
