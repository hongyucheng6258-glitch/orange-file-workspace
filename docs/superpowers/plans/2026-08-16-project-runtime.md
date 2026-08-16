# 项目代码运行能力实施计划

> 本计划只覆盖设计文档中的第一阶段：Node.js、Python、Rust 识别，项目页快速运行，确认协议，启动/停止/重启，实时日志，单项目单实例和 Windows Job Object 清理。
>
> 不在本计划内：端口预览、独立运行中心、运行历史、Java/Makefile/Docker、依赖自动安装。

## 当前边界

当前工作区已有与全局搜索、扫描服务、系统工具相关的未提交改动。本计划不得回滚、重排或混入这些改动。新增运行能力独立于 `editorStore`、`project_service.rs` 的项目 CRUD 和数据库运行历史。

现有调用约定：前端通过 `call<T>(command, camelCaseArgs)` 调用 Tauri 命令；前端通过 `listen<T>(eventName, callback)` 订阅事件；Rust 命令返回 `CommandResult<T>`，错误使用 `AppError { code, message }`。

## 文件映射

### 后端新增

- `src-tauri/src/services/project_detector.rs`：只读项目根元数据，识别 Node.js、Python、Rust，并生成结构化候选命令。
- `src-tauri/src/services/run_confirmation.rs`：配置校验、路径规范化、敏感值脱敏、规范化 JSON、会话密钥、一次性确认票据和 HMAC。
- `src-tauri/src/services/process_api.rs`：`Win32ProcessApi` 抽象；Windows 生产实现封装 Job Object、挂起进程、管道、恢复、终止和等待；非 Windows 提供测试/不可用实现。
- `src-tauri/src/services/project_runtime.rs`：项目级单实例、运行状态机、日志缓存、事件顺序、启动/停止/重启和清理表。
- `src-tauri/src/commands/project_runtime.rs`：Tauri 命令薄封装，不直接调用 Win32 FFI。

### 后端修改

- `src-tauri/src/services/mod.rs`：注册四个服务模块。
- `src-tauri/src/commands/mod.rs`：注册运行命令模块。
- `src-tauri/src/events.rs`：增加运行状态、输出、退出和错误事件名。
- `src-tauri/src/lib.rs`：在 `AppState` 增加运行时状态；setup 后启动确认票据过期清理；正常退出时触发运行任务清理；将运行命令加入 `generate_handler!`。
- `src-tauri/Cargo.toml`：补充确认协议需要的 HMAC、随机会话密钥和常量时间比较依赖；确认 Windows crate features 覆盖 Job Object、进程、管道、环境和路径 API。

### 前端新增

- `src/features/projects/lib/projectRuntime.ts`：运行 DTO、命令封装、事件 payload、事件订阅和日志序号去重。
- `src/features/projects/stores/projectRuntimeStore.ts`：项目识别、配置、确认状态、运行快照、日志和错误状态；不扩展 `editorStore`。
- `src/features/projects/components/ProjectRuntimePanel.tsx`：项目识别、候选命令、配置编辑、运行控制和状态摘要。
- `src/features/projects/components/RunConfirmationDialog.tsx`：展示后端摘要和脱敏环境变量，提交确认票据。
- `src/features/projects/components/ProjectLogViewer.tsx`：按 stdout/stderr 展示当前保留日志，支持清空视图和复制。
- `src/features/projects/lib/projectRuntime.test.ts`：运行 DTO、候选命令和事件去重测试。
- `src/features/projects/stores/projectRuntimeStore.test.ts`：配置变化、确认失效、状态按钮和日志状态测试。

### 前端修改

- `src/features/projects/routes/ProjectPage.tsx`：修复树节点展开时传入空 `projectId` 的现有问题；选中项目后挂载 `ProjectRuntimePanel`。
- `src/styles/app.css`：增加运行面板、确认对话框和日志区域样式，保持现有项目页布局。

## 实施顺序

### 1. 建立识别器测试夹具

**目标**：先锁定 Node.js、Python、Rust 识别规则，不调用真实项目脚本。

**修改文件**：

- 新建 `src-tauri/src/services/project_detector.rs`
- 修改 `src-tauri/src/services/mod.rs`

**实现内容**：

- 定义 `RuntimeKind`：`Node`、`Python`、`Rust`。
- 定义 `RuntimeCandidate`：`label`、`executable`、`args`、`confidence`、`diagnostics`。
- 定义 `DetectionResult`：`runtime_kind`、`candidates`、`diagnostics`。
- 设计 `ProjectFs` 只读接口，生产实现访问项目根；测试实现使用临时目录并记录读取路径。
- 只允许读取 `package.json`、Cargo 元数据配置、`pyproject.toml`、`requirements.txt`、固定 Python 入口文件和锁文件存在性。
- Node：`dev`、`start`、其他 script；锁文件按 pnpm、yarn、bun、npm 顺序选择命令前缀；`main` 仅在项目内入口存在且没有 `dev`/`start` 时生成 `node <main>`。
- Rust：优先调用 `cargo metadata --no-deps --format-version 1`；单 package 生成 `cargo run`；多 package 或多 binary 只返回需选择 `--package`/`--bin` 的候选诊断。
- Python：优先读取 `[project.scripts]` 生成诊断，不生成不可验证的 `python -m module:function`；按 `main.py`、`app.py`、`manage.py` 检查入口；解释器按 `.venv\Scripts\python.exe`、PATH `python.exe`、`py.exe -3` 生成存在的候选。
- 不读取项目源码和可执行文件内容；所有入口路径先做项目根边界校验。

**测试先行**：在模块测试中先写并运行以下失败测试：

- Node scripts 顺序、锁文件选择、main fallback。
- Python 入口和 `[project.scripts]` 诊断。
- Rust 单 package、workspace 多目标和 cargo 不可用诊断。
- 路径越界被拒绝。
- 读取追踪器确认未访问白名单外文件和源码内容。

**验证命令**：

```powershell
cargo test --manifest-path 'src-tauri/Cargo.toml' project_detector
```

**完成标准**：识别器测试全部通过，失败路径返回结构化诊断，不执行项目命令。

### 2. 实现运行配置和确认协议

**目标**：在启动进程前完成确定性的配置校验和用户确认。

**修改文件**：

- 新建 `src-tauri/src/services/run_confirmation.rs`
- 修改 `src-tauri/Cargo.toml`
- 修改 `src-tauri/src/services/mod.rs`

**实现内容**：

- 定义 `RunConfig`、`NormalizedRunConfig`、`ConfirmationPreview`、`ConfirmationGrant`。
- 校验程序非空、参数允许为空、环境变量名不含 NUL 或 `=`、值不含 NUL、端口范围和预览协议枚举。
- 规范化项目 ID、真实项目根、cwd、PATH 解析后的 executable、`.cmd/.bat` 解释器路径和参数数组。
- 对路径统一 Windows 绝对规范形式；对环境变量名按大小写不敏感判重并按 invariant uppercase + UTF-16 序排序。
- 按固定字段顺序生成 UTF-8 canonical JSON，缺省字段写 `null`，数组保持顺序。
- 应用启动生成内存 `session_secret`；`prepare_run_confirmation` 生成 10 分钟有效、单次使用 `confirmation_id`；确认后使用 HMAC-SHA-256 生成当前会话 `confirmation_hash`。
- 哈希验证使用恒定时间比较；确认票据原子取出，防止并发兑换两次。
- 摘要、错误和持久化配置中对 `TOKEN`、`SECRET`、`PASSWORD`、`KEY` 等敏感变量脱敏，但原始值仍参与 canonical JSON。

**测试先行**：

- canonical JSON 对字段顺序稳定。
- Windows 路径等价形式生成同一规范配置。
- 环境变量大小写冲突被拒绝，排序稳定。
- confirmation ID 10 分钟后过期，且并发兑换最多成功一次。
- 配置任一字段、环境值、PATH 解析结果变化后旧 hash 失效。
- 新会话和应用重启后旧 hash 失效。
- 摘要和错误不包含敏感值。

**验证命令**：

```powershell
cargo test --manifest-path 'src-tauri/Cargo.toml' run_confirmation
```

**完成标准**：无需启动进程即可完整测试确认协议，测试覆盖所有失效边界。

### 3. 建立可故障注入的 Windows 进程抽象

**目标**：把 Win32 FFI 与运行状态机隔离，并可靠覆盖 Job Object 竞态和失败分支。

**修改文件**：

- 新建 `src-tauri/src/services/process_api.rs`
- 修改 `src-tauri/src/services/mod.rs`
- 修改 `src-tauri/Cargo.toml` 的 Windows features（如缺失）

**接口要求**：

```rust
trait Win32ProcessApi: Send + Sync {
    fn create_job(&self) -> Result<JobHandle, ProcessApiError>;
    fn set_job_kill_on_close(&self, job: &JobHandle) -> Result<(), ProcessApiError>;
    fn create_process_suspended(&self, spec: &ProcessSpec) -> Result<SuspendedProcess, ProcessApiError>;
    fn assign_process_to_job(&self, job: &JobHandle, process: &SuspendedProcess) -> Result<(), ProcessApiError>;
    fn resume_thread(&self, process: &SuspendedProcess) -> Result<(), ProcessApiError>;
    fn terminate_job(&self, job: &JobHandle) -> Result<(), ProcessApiError>;
    fn wait_process_exit(&self, process: &SuspendedProcess, timeout: Duration) -> WaitResult;
    fn query_job_process_count(&self, job: &JobHandle) -> Result<u32, ProcessApiError>;
}
```

- Windows 生产实现：Job Object 先创建并设置 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`；进程使用 `CREATE_SUSPENDED`；加入 Job 成功后才 `ResumeThread`。
- 加入 Job、恢复线程或终止失败时，生产实现返回 Win32 错误码；不得恢复未受控进程。
- 句柄包装类型表达所有权，保证每个句柄最多关闭一次。
- 测试实现按调用点注入创建失败、Assign 失败、Resume 失败、Terminate 失败、等待超时和屏障竞态。

**测试先行**：

- Job 创建/配置失败不会创建进程。
- 挂起进程加入 Job 失败时不恢复线程并完成清理。
- Resume 失败时终止 Job 并等待清理。
- 终止超时保留可重试的清理句柄。
- 每个分支最多发布一次终态，句柄不会重复关闭。

**验证命令**：

```powershell
cargo test --manifest-path 'src-tauri/Cargo.toml' process_api
```

**完成标准**：生产 Windows 编译通过，测试实现可稳定触发所有关键失败和竞态，不依赖真实系统故障。

### 4. 实现项目运行状态机和日志管线

**目标**：在不依赖 Tauri 命令的情况下完成项目级单实例、生命周期、事件顺序和清理表。

**修改文件**：

- 新建 `src-tauri/src/services/project_runtime.rs`
- 修改 `src-tauri/src/events.rs`
- 修改 `src-tauri/src/services/mod.rs`

**实现内容**：

- 定义 `RunState`：`Starting`、`Running`、`Stopping`、`Exited`、`Failed`。
- 定义 `RunSnapshot`：`run_id`、`project_id`、规范化 cwd、状态、PID、启动时间、退出码、错误码、最近错误和配置摘要。
- 维护 `RuntimeManager`：按规范化项目根加锁；活动表、项目占位、清理表、确认票据和日志缓存独立于 SQLite 连接锁。
- 启动流程：确认配置、原子登记 `Starting`、创建 Job、挂起创建进程、加入 Job、恢复、登记管道读取线程、发布 `Running`。
- `Starting` 期间收到停止请求时，不恢复主线程，直接终止并清理。
- 停止流程：原子转 `Stopping`，终止 Job，5 秒等待；超时进入 `Failed` 但清理表和项目占位继续保留，新的启动/重启返回 `project_already_running`。
- 重启只复用后端保存的规范化快照；旧运行确认清理完成后再生成新 `run_id`。
- stdout/stderr 单事件最多 32 KiB；每个运行保留最近 2 MiB；`seq` 每个 run 从 1 开始；读取线程不得因前端慢而阻塞管道。
- 发布 `project-process-status`、`project-process-output`、`project-process-exited`、`project-process-error`，所有事件带 `run_id`、`project_id`、时间；输出带 stream、seq、text、truncated。
- `stderr` 不直接判错；退出码 0 进入 `Exited`，非零退出返回 `process_non_zero_exit`。
- 提供正常应用关闭清理入口，5 秒总上限，保留清理失败记录。

**测试先行**：

- 状态机合法转换和非法转换。
- 两个并发启动只有一个创建进程。
- Starting/Stopping/自然退出竞态只发布一个终态。
- stdout/stderr 输出序号单调，日志查询补发不重复。
- 2 MiB 淘汰最旧日志并产生截断标记。
- 停止超时后项目仍被占位，新启动被拒绝。
- 重启等待清理完成并生成新 run_id。
- Win32ProcessApi 故障注入覆盖每个清理分支。

**验证命令**：

```powershell
cargo test --manifest-path 'src-tauri/Cargo.toml' project_runtime
```

**完成标准**：运行状态机可通过测试替身验证，不要求前端或真实项目目录。

### 5. 接入 Tauri 命令、事件和应用生命周期

**目标**：将前四步的纯服务能力暴露给前端，并接入 AppState。

**修改文件**：

- 新建 `src-tauri/src/commands/project_runtime.rs`
- 修改 `src-tauri/src/commands/mod.rs`
- 修改 `src-tauri/src/lib.rs`
- 修改 `src-tauri/src/events.rs`

**命令**：

- `detect_project_runtime(project_id)`：读取项目根路径并调用识别器。
- `prepare_run_confirmation(project_id, config)`：返回脱敏预览和 confirmation ID。
- `confirm_run_config(confirmation_id)`：兑换一次性票据并返回 hash。
- `start_project_process(project_id, config, confirmation_hash)`：后端重新规范化并启动。
- `stop_project_process(run_id)`：幂等停止。
- `restart_project_process(run_id)`：复用旧快照重启。
- `get_project_run(project_id)`：返回活动或最近终态。
- `get_process_logs(run_id, after_seq)`：返回当前保留日志区间和最新序号。

命令层只做项目资源查询、参数映射和 `AppError` 转换；不得直接持有 SQLite 锁等待进程或日志。`AppState` 增加 `Arc<RuntimeManager>`，setup 中初始化；正常窗口关闭回调调用清理入口；所有命令加入 `generate_handler!`。

**测试**：

- 命令参数 camelCase 到 Rust 字段映射。
- 未知项目、确认无效、重复启动、未知 run_id 返回规定错误码。
- 命令不会阻塞数据库连接锁。

**验证命令**：

```powershell
cargo test --manifest-path 'src-tauri/Cargo.toml' commands::project_runtime
cargo check --manifest-path 'src-tauri/Cargo.toml'
```

**完成标准**：Tauri 命令可被前端调用，事件 payload 与设计文档一致。

### 6. 建立前端运行 API 与 Zustand store

**目标**：不改变编辑器状态管理，增加项目运行的独立前端状态层。

**修改文件**：

- 新建 `src/features/projects/lib/projectRuntime.ts`
- 新建 `src/features/projects/stores/projectRuntimeStore.ts`
- 新建对应测试文件

**实现内容**：

- 镜像 Rust DTO：`DetectionResult`、`RuntimeCandidate`、`RunConfig`、`ConfirmationPreview`、`RunSnapshot`、`LogPage`。
- 封装所有命令调用，统一把 `Error.code` 转换为 store 可读错误。
- 订阅项目进程事件，按 `runId` 过滤，按 `seq` 去重排序；事件断档时调用 `get_process_logs` 补发。
- Store 操作：`detect(projectId)`、`setConfig`、`prepareConfirmation`、`confirm`、`start`、`stop`、`restart`、`clearVisibleLogs`。
- 由 store 派生按钮状态：确认中禁用启动；Starting/Running 禁用启动；Stopping 禁用停止和重启；Failed 且清理未完成仍禁用启动。
- 不在前端计算 hash，不在前端保存敏感环境变量明文到日志或错误摘要。

**测试先行**：

- 配置编辑使 confirmation 状态失效。
- 事件按 runId 过滤、按 seq 去重和排序。
- Starting/Running/Stopping/Exited/Failed 对按钮状态的映射。
- 后端错误码映射为可读状态。

**验证命令**：

```powershell
$env:PATH = 'D:\;' + $env:PATH; & 'D:\npm.cmd' test -- --run src/features/projects/lib/projectRuntime.test.ts src/features/projects/stores/projectRuntimeStore.test.ts
```

**完成标准**：store 测试通过，组件无需了解 Tauri 事件细节。

### 7. 实现项目页运行面板和日志查看器

**目标**：让用户在现有项目页内完成识别、确认、启动、停止、重启和日志查看。

**修改文件**：

- 新建 `src/features/projects/components/ProjectRuntimePanel.tsx`
- 新建 `src/features/projects/components/RunConfirmationDialog.tsx`
- 新建 `src/features/projects/components/ProjectLogViewer.tsx`
- 修改 `src/features/projects/routes/ProjectPage.tsx`
- 修改 `src/styles/app.css`

**实现内容**：

- `ProjectPage` 选中项目后调用识别，并在编辑器右侧或底部挂载运行面板。
- 展示项目类型、候选命令、程序、参数、cwd 和环境变量编辑入口。
- 运行前展示最终命令、规范化工作目录和脱敏环境变量；确认后调用后端票据兑换。
- 提供启动、停止、重启按钮、状态、PID、运行时长、退出码和错误码。
- 日志按 stdout/stderr 区分，展示截断提示，支持清空视图和复制当前保留日志。
- 对不存在运行时、工作目录越界、确认过期、项目已运行等错误显示可读提示。
- 修复 `TreeNode` 调用 `list_project_files` 时的空 `projectId`，由 `ProjectPage` 传入当前项目 ID。

**测试**：

- 首次运行打开确认对话框。
- 修改命令后再次运行要求重新确认。
- 状态和按钮禁用正确。
- 日志追加、去重、清空和复制正确。
- 错误摘要不显示敏感环境变量。

**验证命令**：

```powershell
$env:PATH = 'D:\;' + $env:PATH; & 'D:\npm.cmd' test
$env:PATH = 'D:\;' + $env:PATH; & 'D:\npm.cmd' run build
```

**完成标准**：项目页可以不离开当前页面完成第一阶段运行闭环。

### 8. 集成验证与发布构建

**目标**：验证完整第一阶段，不将第二、三阶段内容混入。

**执行顺序**：

```powershell
$env:PATH = 'D:\;' + $env:PATH; & 'D:\npm.cmd' test
$env:PATH = 'D:\;' + $env:PATH; & 'D:\npm.cmd' run build
$env:PATH = 'D:\;C:\Users\asus\.cargo\bin;' + $env:PATH; & 'C:\Users\asus\.cargo\bin\cargo.exe' fmt --manifest-path 'E:\work\新建文件夹\src-tauri\Cargo.toml' -- --check
$env:PATH = 'D:\;C:\Users\asus\.cargo\bin;' + $env:PATH; & 'C:\Users\asus\.cargo\bin\cargo.exe' test --manifest-path 'E:\work\新建文件夹\src-tauri\Cargo.toml'
$env:PATH = 'D:\;C:\Users\asus\.cargo\bin;' + $env:PATH; & 'C:\Users\asus\.cargo\bin\cargo.exe' clippy --manifest-path 'E:\work\新建文件夹\src-tauri\Cargo.toml' --all-targets -- -D warnings
$env:PATH = 'D:\;C:\Users\asus\.cargo\bin;' + $env:PATH; & 'D:\npm.cmd' run tauri build
```

**验收清单**：

- 前端测试和 TypeScript/Vite 构建通过。
- Rust 单元测试、确认协议、识别器、状态机和进程抽象测试通过。
- Windows 专属 Job Object 测试通过，或明确报告受系统环境限制而跳过。
- Tauri release 构建生成可执行文件和安装包。
- `git status` 确认没有生成测试 fixture、日志、构建中间文件或未预期源码改动。
- 不提交当前其他未提交改动，运行能力按逻辑拆分为独立提交。

## 提交策略

按以下逻辑提交，避免大提交混合：

1. `test(project-runtime): add detector and confirmation contracts`
2. `feat(project-runtime): add process containment and runtime state machine`
3. `feat(projects): add project runtime panel`
4. `test(project-runtime): verify end-to-end run lifecycle`

每次提交前执行对应模块测试和 `git diff --check`。所有提交完成后再执行一次全量验证和 Tauri 构建。
