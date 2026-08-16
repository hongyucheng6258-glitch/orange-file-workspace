# 项目运行第二阶段实施计划：Web 端口预览

> 本计划覆盖设计文档第二阶段：端口配置、日志地址识别和监听检测、打开系统浏览器预览、端口冲突和预览归属提示。不包含第三阶段运行中心与运行历史。

## 当前边界

第一阶段（项目页快速运行）已提交（`66e29a9`..`b414bab`）。工作区仍有全局搜索、扫描服务、系统工具、内置终端等未提交改动，本计划不得回滚、重排或混入这些改动。

现有能力复用：

- `RuntimeManager` 提供 `get_run(run_id)`、`get_logs(run_id, after_seq)`。
- `Win32ProcessApi` 已封装 Job Object 与进程操作；`process_api.rs` 的 `JobHandle` 为唯一所有权，需改为可共享引用以支持运行期间端口归属校验。
- `Cargo.toml` 已启用 `Win32_NetworkManagement_IpHelper`、`Win32_System_JobObjects`。
- 前端已有 `@tauri-apps/plugin-opener`，用于打开系统浏览器。
- 事件名规划：`project-preview-ready`。

## 设计要点（摘自设计文档）

1. 预览目标确定顺序：用户配置的 `expected_port` + `preview_scheme` → 结构化命令参数中的明确端口 → 日志中的本地 `http(s)://` 地址及路径。无法确认时不生成预览链接。
2. 打开前确认目标端口正在监听；端口监听 PID 属于当前 Job Object 时归属确认，无法确认归属时界面标记"端口归属未确认"并要求用户手动打开。
3. 日志中的协议、主机和路径必须保留；仅只有端口时使用 `http://127.0.0.1:<port>`。
4. 端口未监听时返回 `preview_unavailable`。
5. 命令：`open_project_preview(run_id) -> PreviewTarget`。
6. 首版只提供手动打开，不保存 `auto_open_preview`。

## 文件映射

### 后端新增

- `src-tauri/src/services/preview_service.rs`：`PreviewTarget`、`PreviewSource`、`PortOwnership`、目标解析纯函数、`PortProbe` trait、Windows 生产实现、`PreviewService`。
- `src-tauri/src/commands/project_preview.rs`：`open_project_preview` 命令。

### 后端修改

- `src-tauri/src/services/process_api.rs`：`JobHandle` 改为引用计数共享（`Arc`），支持 `Clone`；`Win32ProcessApi` 增加 `query_job_process_ids(&self, job) -> Result<Vec<u32>, ProcessApiError>`；Windows 实现用 `QueryInformationJobObject(JobObjectBasicProcessIdList)`。
- `src-tauri/src/services/project_runtime.rs`：`ActiveRun` 保存 Job 句柄共享引用；暴露 `preview_context(run_id) -> Option<PreviewContext>`（快照 + 拼接日志文本 + Job 句柄）；`PreviewService` 在事件发布时调用。
- `src-tauri/src/services/mod.rs`：注册 `preview_service`。
- `src-tauri/src/commands/mod.rs`：注册预览命令模块。
- `src-tauri/src/events.rs`：增加 `EVENT_PROJECT_PREVIEW_READY`。
- `src-tauri/src/lib.rs`：初始化 `PreviewService` 并加入 `AppState`；命令加入 `generate_handler!`。
- `src-tauri/Cargo.toml`：确认 `Win32_NetworkManagement_IpHelper` feature（已存在则不动）。

### 前端新增

- `src/features/projects/lib/projectPreview.ts`：`PreviewTarget` 等 DTO、`openProjectPreview` 命令封装、`subscribePreviewReady` 事件订阅、URL 格式化纯函数。
- `src/features/projects/stores/projectPreviewStore.ts`：预览状态（目标、归属、检查中、错误）、`openPreview()`、事件订阅。
- `src/features/projects/lib/projectPreview.test.ts`：URL 格式化、归属状态派生测试。
- `src/features/projects/stores/projectPreviewStore.test.ts`：事件更新、打开流程、归属提示测试。

### 前端修改

- `src/features/projects/components/ProjectRuntimePanel.tsx`：增加预览按钮、预览 URL 展示和归属未确认提示。
- `src/styles/app.css`：预览按钮和归属提示样式。

## 实施顺序

### 1. 目标解析与 URL 提取纯函数（测试先行）

**文件**：`src-tauri/src/services/preview_service.rs`（先写测试模块）。

- `port_from_args(args) -> Option<u16>`：识别 `--port=3000`、`--port 3000`、`-p 3000`；忽略其他参数。
- `local_url_from_log(text) -> Option<UrlCandidate>`：正则提取 `http(s)://host[:port][/path]`，仅接受本地 host（`localhost`、`127.0.0.1`、`0.0.0.0`、`[::1]`、`::1`）；端口缺省 http=80、https=443；保留 path；`0.0.0.0`/`::` 访问地址归一为 `127.0.0.1`。
- `resolve_target(expected_port, scheme, args, log_text) -> Option<UrlCandidate>`：按 config → args → log 优先级。
- `format_url(candidate) -> String`。
- `PreviewSource`（`config`/`args`/`log`）与 `UrlCandidate` 序列化。

**验证**：`cargo test --manifest-path 'src-tauri/Cargo.toml' preview_service`

### 2. 端口探测与 Job 归属校验抽象

**文件**：`src-tauri/src/services/process_api.rs`、`src-tauri/src/services/preview_service.rs`。

- `PortProbe` trait：`listening_pid(&self, port: u16) -> Option<u32>`。
- Windows 生产实现：`GetExtendedTcpTable(TCP_TABLE_OWNER_PID_LISTENER)`，过滤 `MIB_TCP_STATE_LISTEN` 与端口（`dwLocalPort` 大端转主机序），返回 `dwOwningPid`。
- `Win32ProcessApi::query_job_process_ids`：两次 `QueryInformationJobObject(JobObjectBasicProcessIdList)` 获取 job 内 PID 集合；Windows 实现与测试替身（FakeApi 增加 `job_pids` 字段）。
- `JobHandle` 改为 `Arc` 内部共享：`Clone` 增加引用计数，引用归零才 `CloseHandle`；同步修改 `project_runtime.rs` 中所有构造/使用点。

**验证**：`cargo test --manifest-path 'src-tauri/Cargo.toml' process_api preview_service`

### 3. PreviewService 与命令接入

**文件**：`src-tauri/src/services/preview_service.rs`、`src-tauri/src/services/project_runtime.rs`、`src-tauri/src/commands/project_preview.rs`、`src-tauri/src/events.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/services/mod.rs`、`src-tauri/src/commands/mod.rs`。

- `RuntimeManager::preview_context(run_id)`：仅活动运行（`running`/`starting`）可预览；返回快照、日志文本（拼接当前保留条目）、Job 句柄 clone。
- `PreviewService::open_preview(run_id) -> Result<PreviewTarget>`：
  1. 取上下文，运行不存在返回 `run_not_found`，非活动运行返回 `preview_unavailable`。
  2. `resolve_target` 解析，无目标返回 `preview_unavailable`。
  3. `probe.listening_pid(port)`，未监听返回 `preview_unavailable`。
  4. 有 Job 句柄时 `query_job_process_ids` 校验归属；无法确认归属或运行已结束返回 `ownership = unconfirmed`。
  5. 发布 `project-preview-ready` 事件（payload = PreviewTarget）。
- 命令 `open_project_preview(run_id)` 薄封装。
- `lib.rs`：`AppState` 增加 `preview: Arc<PreviewService>`；setup 初始化（注入 `PortProbeImpl` 与 `runtime`）；命令注册。
- `project_runtime.rs` 的协调线程终态处理不受影响（Job 共享引用随 ActiveRun 移除而释放）。

**验证**：`cargo test --manifest-path 'src-tauri/Cargo.toml' preview_service project_runtime`；`cargo check`

### 4. 前端预览状态与打开

**文件**：`src/features/projects/lib/projectPreview.ts`、`src/features/projects/stores/projectPreviewStore.ts`、`src/features/projects/components/ProjectRuntimePanel.tsx`、`src/styles/app.css`、前端测试。

- `openProjectPreview(runId)` 命令封装；`subscribePreviewReady` 事件订阅。
- Store：`preview: PreviewTarget | null`、`previewOwnership`、`checking`、`error`；`openPreview()` 调用命令；事件更新 `preview`；`reset()` 随项目切换清空。
- 组件：运行面板操作区增加"打开预览"按钮（运行中且配置了 `expected_port` 或日志含 URL 时可用）；confirmed 时点击直接 `openUrl`；unconfirmed 时展示"端口归属未确认"提示与手动打开按钮；`preview_unavailable` 显示可读提示。
- 前端测试：URL 格式化、归属派生、store 打开流程与事件更新。

**验证**：`npm test`；`npm run build`

### 5. 集成验证与提交

- 全量：`npm test`、`npm run build`、`cargo fmt --check`（仅新文件）、`cargo test`、`cargo clippy`（新代码无警告）、`npm run tauri build`。
- 提交策略（选择性暂存，不混入其他未提交改动）：
  1. `docs(project-runtime): add phase 2 web preview plan`
  2. `feat(project-runtime): add preview target resolution and port probe abstraction`
  3. `feat(projects): add web preview button and ownership hint`
  4. `style(project-runtime): rustfmt and clippy on preview modules`

## 验收清单（对应设计文档第二阶段）

- 明确配置的监听端口通过验证后可打开预览。
- 日志中的 HTTPS 地址和路径被保留。
- 端口未监听时返回 `preview_unavailable`。
- 端口监听者无法确认属于当前运行时，不自动打开并展示归属提示。
- `git status` 确认没有生成临时文件，其他未提交改动未被混入提交。
