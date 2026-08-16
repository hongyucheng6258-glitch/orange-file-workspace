# 项目运行第三阶段实施计划：独立运行中心

> 本计划覆盖设计文档第三阶段：独立运行中心、多项目并行管理、活动运行列表、
> 已退出记录与日志保留策略、Java / Makefile / Docker 候选识别。
> 不包含完整终端仿真、远程运行与容器隔离。

## 当前边界

第一、二阶段已提交（`66e29a9`..`b781b9f`）。工作区仍有全局搜索、扫描服务、
内置终端等未提交改动，本计划不得回滚、重排或混入这些改动。

现有能力复用：

- `RuntimeManager`：`runs`（活动）、`cleanup`（清理中）、`recent`（每项目最近一条终态）。
- 迁移机制：`db/migrations.rs` 的 `MIGRATIONS` 数组 + `include_str!` SQL，下一版本号为 8。
- `db::connection::open` 提供 WAL + busy_timeout 的连接。
- 前端已有项目页运行面板（`ProjectRuntimePanel`）、日志查看器（`ProjectLogViewer`）、预览 store。
- `commands/projects.rs::list_projects` 返回 `Resource[]`，可映射 `project_id -> name`。
- 事件：`project-process-status/output/exited/error`、`project-preview-ready`（全局，不按项目过滤）。

## 设计要点（摘自设计文档）

1. 运行中心复用统一后端命令和事件，不实现第二套进程逻辑。
2. 多项目并行：`runs` 为 HashMap，天然支持；单实例键仍为规范化项目根（同一项目默认只有一个活动实例）。
3. 命令接口：`list_project_runs(include_exited) -> RunSnapshot[]`。
4. 每个项目默认保留最近 20 条已退出记录，最多保留 7 天；运行历史落库，日志默认不跨应用重启持久化。
5. 应用重启后能看到运行历史，但不把旧运行误报为仍在运行（历史记录均为终态）。

## 文件映射

### 后端新增

- `src-tauri/migrations/0008_project_run_history.sql`：运行历史表 + 索引。
- `src-tauri/src/services/run_history.rs`：`RunHistoryStore` trait、`HistoryRun`、
  `SqliteRunHistoryStore`（注入 Connection）、`InMemoryRunHistoryStore`（测试）、
  每项目 20 条 + 7 天保留策略。
- `src-tauri/src/commands/run_center.rs`：`list_project_runs` 命令。

### 后端修改

- `src-tauri/src/db/migrations.rs`：注册 version 8 迁移。
- `src-tauri/src/services/project_runtime.rs`：
  - `RuntimeManager::new(api, sink, history)`；终态写入历史；启动时加载历史到 recent（终态，不误报运行中）。
  - `list_runs(include_exited)`：活动 runs + cleanup + recent + history（按 run_id 去重，倒序）。
  - `get_project_name` 由前端映射，后端不新增字段。
- `src-tauri/src/services/project_detector.rs`：`RuntimeKind` 增加 `Java`、`Makefile`、`Docker`；
  marker 检查与保守候选/诊断规则。
- `src-tauri/src/services/mod.rs`：注册 `run_history`。
- `src-tauri/src/commands/mod.rs`：注册 `run_center`。
- `src-tauri/src/lib.rs`：初始化 `SqliteRunHistoryStore` 并注入；注册命令。
- `src-tauri/src/services/backup_service.rs`、`src-tauri/src/services/test_support.rs`：
  `make_manager`/AppState 测试构造适配新签名。

### 前端新增

- `src/features/runs/lib/runCenter.ts`：`listProjectRuns` 封装、项目名映射。
- `src/features/runs/stores/runCenterStore.ts`：列表、全局事件订阅刷新、停止/重启/预览操作、日志加载。
- `src/features/runs/routes/RunCenterPage.tsx`：活动实例区 + 已退出记录区 + 行日志展开。
- `src/features/runs/stores/runCenterStore.test.ts`、`src/features/runs/lib/runCenter.test.ts`。

### 前端修改

- `src/app/App.tsx`：注册 `/runs` 路由。
- `src/components/Sidebar.tsx`：加入"运行中心"入口。
- `src/styles/app.css`：运行中心列表与状态徽标样式。

## 实施顺序

### 1. 运行历史存储（测试先行）

**文件**：`src-tauri/src/services/run_history.rs`、`src-tauri/migrations/0008_project_run_history.sql`、`src-tauri/src/db/migrations.rs`。

- `HistoryRun`：run_id、project_id、project_key、executable、args、cwd、env（脱敏摘要）、
  expected_port、preview_scheme、state、exit_code、error_code、error_message、stop_reason、
  started_at、exited_at；`to_snapshot()` 转 `RunSnapshot`（state 强制终态）。
- `RunHistoryStore` trait：`record(snap, project_key)`、`list(limit)`、`by_project(project_key, limit)`、`prune()`。
- 保留策略在 store 内实现：按 `project_key` 保留最近 20 条、全局 `exited_at` 7 天内。
- `SqliteRunHistoryStore`：注入 `Connection`（`Mutex` 包裹）；`InMemoryRunHistoryStore` 测试实现。
- 迁移 8：`project_run_history` 表 + `(project_key, started_at DESC)` 索引 + `exited_at` 索引。
- 测试：record/list/去重/prune（20 条与 7 天）、SQLite 迁移可应用。

**验证**：`cargo test --manifest-path 'src-tauri/Cargo.toml' run_history migrations`

### 2. RuntimeManager 集成与列表命令

**文件**：`src-tauri/src/services/project_runtime.rs`、`src-tauri/src/services/test_support.rs`、`src-tauri/src/services/backup_service.rs`、`src-tauri/src/commands/run_center.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/services/mod.rs`。

- `RuntimeManager::new(api, sink, history)`；`load_history()` 启动时把历史记录并入 recent（仅终态）。
- 终态路径（`finalize_exited`、`finalize_stop_timeout`、占位释放）写入历史。
- `list_runs(include_exited)`：合并 runs + cleanup + recent + history，按 `started_at` 倒序、run_id 去重。
- 命令 `list_project_runs(include_exited)` 薄封装。
- `lib.rs`：构造 `SqliteRunHistoryStore`（复用 `state.db` 连接或独立连接；采用独立 `Mutex<Connection>` 打开同一 db 文件），注入 runtime；注册命令。
- 测试：终态写入历史、历史加载不误报运行中、多项目列表、include_exited 开关、去重。

**验证**：`cargo test --manifest-path 'src-tauri/Cargo.toml' project_runtime run_center`；`cargo check`

### 3. Java / Makefile / Docker 候选识别

**文件**：`src-tauri/src/services/project_detector.rs`。

- marker：`pom.xml`、`build.gradle`/`build.gradle.kts`、`Makefile`/`makefile`/`GNUmakefile`、
  `Dockerfile`、`docker-compose.yml`/`compose.yaml`。
- Java：pom.xml + PATH 可解析 `mvn` 且 pom 含 `spring-boot-maven-plugin` →
  `mvn spring-boot:run`（80）；build.gradle + `gradle` 可解析且含 `org.springframework.boot` 插件 →
  `gradle bootRun`（80）；其余只出诊断（不猜测）。
- Makefile：makefile 存在 + PATH 可解析 `make` → `make`（60，默认目标），否则诊断。
- Docker：compose 文件 + `docker` 可解析 → `docker compose up`（70）；仅有 Dockerfile → 诊断。
- 全部遵循"只读清单文件、PATH 可执行仅解析、不执行项目脚本"边界。
- 测试：FakeFs 覆盖各 marker、可执行缺失诊断、插件缺失诊断、不读取源码。

**验证**：`cargo test --manifest-path 'src-tauri/Cargo.toml' project_detector`

### 4. 运行中心前端

**文件**：`src/features/runs/**`、`src/app/App.tsx`、`src/components/Sidebar.tsx`、`src/styles/app.css`。

- `listProjectRuns(includeExited)` 封装；项目名映射（`list_projects`）。
- Store：`runs: RunSnapshot[]`、`refreshing`、`load()`、`refresh()`、事件订阅（status/output 不刷全表，
  仅 status/exited/error 刷新对应行并触发 `load`）、`stop(runId)`、`restart(runId)`、
  `openPreview(runId)`（复用 `openProjectPreview` + `openUrl`）、`loadLogs(runId)`。
- 页面：活动实例卡片/表格（状态徽标、项目名、PID、启动时间、端口、操作：停止/重启/预览/日志）、
  已退出记录区（终态徽标、退出码、最近错误、启动/结束时间、查看日志）；行展开显示日志
  （复用 `ProjectLogViewer`，跨会话提示"日志仅保留在当前会话"）。
- 前端测试：store 的加载/事件刷新/操作、项目名映射。

**验证**：`npm test`；`npm run build`

### 5. 集成验证与提交

- 全量：`npm test`、`npm run build`、`cargo fmt`（新文件）、`cargo test`、`cargo clippy`（新代码无警告）、`npm run tauri build`。
- 提交策略（选择性暂存）：
  1. `docs(project-runtime): add phase 3 run center plan`
  2. `feat(project-runtime): persist run history and add list runs command`
  3. `feat(project-detector): detect java makefile docker candidates`
  4. `feat(runs): add run center page with activity and history`
  5. `style(project-runtime): rustfmt and clippy on run center modules`

## 验收清单（对应设计文档第三阶段）

- 运行中心能管理多个不同项目，项目页和运行中心状态一致。
- 同一项目默认只有一个活动实例。
- 每项目最多保留 20 条、最多 7 天的退出记录。
- 应用重启后能看到运行历史，但不会把旧运行误报为仍在运行。
- `git status` 确认没有生成临时文件，其他未提交改动未被混入提交。
