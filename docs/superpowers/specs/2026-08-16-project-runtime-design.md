# 项目代码运行能力设计

## 背景

当前应用已经支持代码项目导入、文件树展示、代码编辑和冲突安全保存，但导入后的项目主要停留在“可查看、可编辑”阶段。用户需要在应用内识别项目类型、执行项目命令、查看实时输出，并能对 Web 项目打开预览。

本设计采用“项目页快速运行 + 独立运行中心”的最终形态。第一阶段先交付 Node.js、Python、Rust 的项目页运行闭环和必要的进程安全能力；运行中心、多项目并行和运行历史在后续阶段交付。

## 目标

- 导入项目后自动识别常见项目类型和可运行命令。
- 支持用户查看、修改并确认实际执行的命令。
- 支持启动、停止、重启、实时日志、退出码和错误原因。
- 首版支持 Node.js、Python、Rust 项目。
- 支持 Web 服务的端口识别和浏览器预览。
- 由后端保证启动、停止和重启操作的一致性。
- 终止应用创建的 Windows 进程树，减少后台进程和端口残留。
- 为 Java、Makefile、Docker 和完整运行中心保留统一扩展接口。

## 非目标

- 首版不实现完整终端仿真器，也不支持需要持续 stdin 交互的程序。
- 首版不自动安装运行时或项目依赖。
- 首版不识别或执行 Java、Makefile、Docker 项目；这些仅作为后续扩展。
- 首版不提供远程运行、容器隔离或沙箱安全。
- 应用被操作系统强制结束或电脑断电时，不保证子进程一定被清理。

## 分期范围

### 第一阶段

- Node.js、Python、Rust 静态识别。
- 项目页命令配置、首次确认和快速运行。
- 启动、停止、重启及后端并发保护。
- stdout/stderr 实时事件、受限内存日志和退出信息。
- Windows 进程树清理和应用正常退出清理。
- 同一项目单实例运行。

### 第二阶段

- 端口配置、日志地址识别和监听检测。
- 打开系统浏览器预览。
- 端口冲突和预览归属提示。

### 第三阶段

- 独立运行中心和多项目并行管理。
- 活动运行列表、已退出运行记录和日志保留策略。
- Java、Makefile、Docker 候选识别。

本文的“首版验收”指第一阶段；预览相关验收属于第二阶段；运行中心相关验收属于第三阶段。

## 用户流程

### 项目页快速运行

1. 用户打开已导入的代码项目。
2. 页面读取后端识别结果，展示项目类型和候选命令。
3. 用户选择候选命令，或编辑程序、参数、工作目录和环境变量。
4. 第一次执行新配置或配置发生变化时，页面展示最终命令并要求确认。
5. 已确认且未变化的配置允许快速运行；重启沿用当前运行快照，无需再次确认。
6. 页面展示状态、实时输出、退出码和失败原因。
7. 用户可以停止或重启进程；第二阶段可在端口验证后打开预览。

### 独立运行中心

第三阶段新增运行中心，展示活动实例和有限数量的已退出记录，包括项目名称、状态、启动时间、PID、端口和最近错误。运行中心复用统一后端命令和事件，不实现第二套进程逻辑。

## 项目识别

识别器只访问项目根目录和用于确认候选入口的受限元数据路径，不执行任何项目脚本，也不读取项目源码。首版允许三类探测：读取明确列出的配置/元数据文件；对候选路径做存在性、类型和规范路径检查；对 PATH 中的可执行文件做系统解析。清单外不得读取文件内容：

- `package.json`
- `pnpm-lock.yaml`、`yarn.lock`、`bun.lock`、`bun.lockb`、`package-lock.json`
- `Cargo.toml`
- `pyproject.toml`
- `requirements.txt`
- `main.py`、`app.py`、`manage.py`
- `package.json.main` 指向的项目内相对路径，仅做入口存在性和文件类型检查
- `.venv\Scripts\python.exe`，仅做解释器存在性和文件类型检查
- PATH 中按 Windows 规则解析的 `node`、`python`、`py`、`cargo` 等可执行名称，仅记录解析结果和文件类型，不读取可执行文件内容
- Cargo 元数据返回的 package、target、workspace 成员及其标准入口路径；仅做元数据解析和入口路径存在性、类型、规范路径检查，不读取 `src` 或其他项目源码

对 `cargo metadata --no-deps --format-version 1` 的调用仅用于获取 Cargo 元数据，不构建、不运行、不执行项目脚本；调用失败只返回诊断。标准入口仅包括 Cargo 元数据声明的 target 路径，以及可由元数据确认的默认入口（例如 package 的 `src/main.rs`、`src/lib.rs` 和 `src/bin/*.rs`）；这些路径只允许探测存在性，不读取内容。需要解析内容的文件最多读取 1 MiB，解析失败时返回可读诊断并继续检查其他文件。锁文件只检查是否存在，不读取内容。所有候选路径必须先经过项目根目录边界校验，越界路径视为无效候选。

首版不递归搜索 `node_modules`、`.git`、`target`、`.venv` 等目录。以后若支持 monorepo，再单独增加有限深度和 package 选择设计。

### Node.js

1. 读取 `package.json.scripts`。
2. 推荐顺序为 `dev`、`start`，其余 scripts 仅作为可选候选。
3. 根据锁文件选择包管理器：`pnpm-lock.yaml`、`yarn.lock`、`bun.lock`/`bun.lockb`、`package-lock.json`；无锁文件时默认 npm。
4. 首版不自动执行依赖安装。
5. `main` 只在文件存在且没有 `dev`/`start` 时生成 `node <main>` 候选；路径不存在时只返回诊断，不生成候选。

### Rust

1. 读取根目录 `Cargo.toml`，并在 PATH 可解析 `cargo` 时调用 `cargo metadata --no-deps --format-version 1` 获取 package、workspace 和 target 元数据；不得读取 target 指向的源码内容。
2. 只有 Cargo 元数据或标准入口存在性探测确认存在可运行二进制 target 时，单 package 才生成 `cargo run`；仅有 library target 时返回诊断，不生成运行候选。
3. workspace 含多个成员或多个二进制目标时，不猜测目标，要求用户补充 `--package` 或 `--bin` 参数；Cargo 不可解析或元数据调用失败时返回诊断，可用受限标准入口存在性探测补充诊断，但不得通过读取源码推断 target。

### Python

1. 读取 `pyproject.toml` 的 `[project.scripts]`，首版只生成诊断，不据此生成可运行候选。该表的 `name = "module:function"` 表示安装后控制台入口，不能等价改写为 `python -m module`；诊断应列出脚本名和入口值，并提示用户安装项目后填写生成的可执行文件，或手动填写能调用该函数的明确命令。
2. 无论是否存在 `[project.scripts]`，都按 `main.py`、`app.py`、`manage.py` 顺序检查实际存在的入口文件并生成候选；不把任意模块或函数名猜测成命令。
3. Windows 解释器候选顺序为项目 `.venv\Scripts\python.exe`、PATH 中的 `python.exe`、`py.exe -3`；不存在的候选不生成，只写入诊断。
4. `requirements.txt` 只用于提示依赖，不独立证明项目可运行。
5. 首版不自动创建虚拟环境、安装项目或安装依赖，因此也不承诺 `[project.scripts]` 对应的控制台可执行文件已经存在。

识别结果包含 `runtime_kind`、`confidence`、`candidates`、`diagnostics`。候选命令必须转换为结构化的程序和参数数组，不能包含多步骤 shell 流程。

## 运行配置

```text
RunConfig
  project_id: string
  executable: string
  args: string[]
  cwd: string
  env_overrides: map<string, string | null>
  expected_port: number | null
  preview_scheme: http | https
```

- `executable` 必须非空；`args` 可以为空。
- `executable` 可以是绝对路径、项目内相对路径或由 Windows PATH 解析的名称。
- `.exe`、`.com` 直接创建；`.cmd`、`.bat` 使用系统命令解释器并保持参数转义；不依赖文件关联执行任意脚本。
- 环境变量以应用进程环境为基础应用覆盖；值为 `null` 表示从子进程环境删除。
- Windows 环境变量名按大小写不敏感处理。
- 名称含 NUL 或 `=` 时拒绝；值含 NUL 时拒绝。
- 名称匹配 `TOKEN`、`SECRET`、`PASSWORD`、`KEY` 等敏感模式时，确认界面和持久化摘要只显示脱敏值。
- 一次运行使用不可变配置快照。

### 确认规范化与签发

确认哈希绑定后端生成的规范化运行快照，前端不得自行计算或签发：

1. 前端把待确认的 `RunConfig` 发送给 `prepare_run_confirmation`。后端完成配置、路径和可执行文件解析校验，返回规范化后的可展示摘要和一次性 `confirmation_id`；完整规范化配置只保存在后端，敏感值在展示摘要中脱敏，但仍以原值参与哈希。
2. 规范化配置编码为固定字段顺序的 UTF-8 JSON：`version`、`project_id`、`executable`、`args`、`cwd`、`env_overrides`、`expected_port`、`preview_scheme`。`version` 首版固定为 `1`；字符串按 JSON 标准转义，不做 Unicode 等价归一化；数组保持原顺序；缺省可选值写为 `null`，不得省略字段；数字使用十进制 JSON 整数，不允许浮点或指数形式。
3. `project_id` 使用后端项目记录中的稳定 ID；`cwd` 使用路径安全校验后的绝对规范路径；`executable` 固化为后端基于应用环境叠加 `env_overrides` 后的 PATH 解析出的绝对路径，`.cmd`/`.bat` 则固化为系统命令解释器绝对路径及最终完整参数数组。Windows 路径去除等价长路径前缀、统一使用反斜杠，并将盘符和 UNC 主机/共享名折叠为小写。
4. `env_overrides` 按 Windows `CompareStringOrdinal(..., TRUE)` 语义判定键相等；同一键出现不同大小写的重复项时返回 `invalid_config`。规范化时键转换为 invariant uppercase，编码为按键 UTF-16 码元序升序排列的 `[key, value]` 数组，值为原始字符串或 `null`。`args` 逐项保留原始字符串和顺序。
5. 后端保存 `confirmation_id -> canonical_json`，有效期 10 分钟且绑定当前应用会话。用户确认摘要后，前端提交 `confirmation_id` 到 `confirm_run_config`；后端以原子操作取出并立即删除记录，再签发 `confirmation_hash = base64url(HMAC-SHA-256(session_secret, canonical_json))`。无论兑换成功、失败还是并发重复请求，同一 ID 最多只有一个请求能取到记录，后续请求一律返回 `confirmation_required`。`session_secret` 每次应用启动随机生成且只驻留内存，确认 ID 和确认哈希均不跨应用重启持久化。
6. `start_project_process` 接收当前 `config` 和 `confirmation_hash`。后端重新执行相同规范化并重新计算 HMAC，以恒定时间比较两个哈希；哈希缺失、无效、来自其他会话或规范化结果变化时返回 `confirmation_required`。验证成功后才登记 `starting`，并直接以本次规范化快照启动，避免校验后再次解析产生差异。
7. 重启只允许复用后端保存的旧运行规范化快照，不接收前端替换配置，也不重新确认；任何编辑后的配置必须走新的预览、确认和签发流程。

## 路径安全

项目根目录和工作目录必须在后端规范化后比较，禁止字符串前缀判断：

1. 项目根目录必须存在，并解析为绝对规范路径。
2. 工作目录先相对项目根解析，再要求存在且为目录。
3. 解析 `.`、`..`、盘符大小写和 Windows 长路径形式。
4. 跟随符号链接和 junction 后再次检查最终路径仍位于规范化项目根内。
5. UNC 项目允许运行，但项目根和工作目录必须位于同一规范化共享路径下。
6. 校验失败返回 `invalid_working_directory`，不得尝试启动进程。

## 后端模型

### 命令接口

- `detect_project_runtime(project_id) -> DetectionResult`
- `prepare_run_confirmation(project_id, config) -> ConfirmationPreview`
- `confirm_run_config(confirmation_id) -> ConfirmationGrant`
- `start_project_process(project_id, config, confirmation_hash) -> RunSnapshot`
- `stop_project_process(run_id) -> RunSnapshot`
- `restart_project_process(run_id) -> RunSnapshot`
- `get_project_run(project_id) -> RunSnapshot | null`
- `get_process_logs(run_id, after_seq) -> LogPage`
- 第二阶段：`open_project_preview(run_id) -> PreviewTarget`
- 第三阶段：`list_project_runs(include_exited) -> RunSnapshot[]`

所有变更命令在后端串行化同一 `project_id` 的操作。重复停止返回当前快照；已有活动实例或未完成清理的旧实例时，重复启动返回 `project_already_running`；未知或已淘汰的 `run_id` 返回 `run_not_found`。

### 运行身份

- 每次成功创建进程前生成全局唯一 `run_id`。
- 重启必须等待旧进程进入终态，然后使用旧运行的配置快照创建新 `run_id`。
- 如果旧进程未能终止，重启失败且不得启动新进程。
- 第一阶段以规范化项目根目录作为单实例键；活动表或清理表中仍存在该项目的未确认退出进程时，占位都不能释放。
- 只有 Job Object 确认无活动进程且句柄清理完成后，项目才允许新的启动。
- 第三阶段若支持前后端多进程，需要把单实例键扩展为 `project_id + profile_id`。

## 状态机

状态全集：

```text
idle -> starting -> running -> stopping -> exited
                  |          -> failed
         |        -> failed
         -> stopping -> exited | failed
```

- `exited`：进程已结束，退出码可用；用户主动停止后也进入该状态并带 `stop_reason=user`。
- `failed`：进程创建失败、状态管理失败或停止超时；仅输出 stderr 不构成失败。
- `starting` 时允许停止，停止请求必须阻止启动完成后漏杀进程。
- `stopping` 时重复停止是幂等操作。
- 自然退出和停止请求竞态时，以实际退出结果为准，仅发布一次终态。
- 进程已退出但句柄尚未清理时保留清理记录，项目仍视为占用中。
- 只有确认进程退出、Job Object 无活动进程且所有句柄释放后，才从活动表和清理表移除。

## 进程创建与停止

### Win32ProcessApi 抽象

进程管理器不得直接散布 Win32 FFI 调用。首版定义可替换的 `Win32ProcessApi`，生产实现封装 Job Object、管道、进程和线程句柄操作，测试实现按调用点注入成功、指定 Win32 错误码、等待超时和竞态时序：

```text
Win32ProcessApi
  create_job() -> JobHandle
  set_job_kill_on_close(job) -> Result
  create_output_pipes() -> OutputPipes
  create_process_suspended(spec, pipes) -> SuspendedProcess
  assign_process_to_job(job, process) -> Result
  resume_thread(thread) -> Result
  terminate_process(process, exit_code) -> Result
  terminate_job(job, exit_code) -> Result
  wait_process(process, timeout) -> WaitResult
  query_job_active_processes(job) -> u32
  close_handle(handle)
```

句柄类型必须表达所有权并保证每个句柄最多关闭一次；进程管理器负责状态转换、唯一终态和清理表，`Win32ProcessApi` 只封装系统调用，不自行发布事件。故障注入至少能独立控制挂起进程创建、Job 创建/配置、加入 Job、恢复线程、终止调用、等待结果和查询活动进程数，并能用屏障控制“停止请求、恢复成功、自然退出、等待超时”的先后顺序。

### 创建

1. 完成配置、确认哈希和路径校验。
2. 原子检查项目单实例并登记 `starting` 占位。
3. 创建当前运行专属的 Windows Job Object，并在创建任何子进程前设置 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`；创建或配置失败时关闭已有句柄、撤销占位，返回 `process_containment_failed` 并进入 `failed`。
4. 创建 stdout/stderr 管道，调用 `CreateProcessW` 时同时使用隐藏窗口选项和 `CREATE_SUSPENDED`。创建失败时关闭管道和 Job Object、撤销占位，返回 `process_spawn_failed` 并进入 `failed`。
5. 在主线程仍挂起时调用 `AssignProcessToJobObject`。加入失败时绝不恢复主线程：先对该挂起进程调用 `TerminateProcess`，等待进程退出，再关闭线程、进程、管道和 Job Object 句柄，返回 `process_containment_failed` 并进入 `failed`。
6. 加入成功后注册 Job、进程和主线程句柄，再调用 `ResumeThread`。恢复失败时调用 `TerminateJobObject` 并等待进程退出，返回 `process_spawn_failed` 并进入 `failed`；不得发布 `running`。
7. 只有 `ResumeThread` 成功且未收到并发停止请求时才发布 `running`，随后关闭不再需要的主线程句柄，分别读取 stdout 和 stderr，并等待进程退出。若挂起期间已收到停止请求，则不恢复主线程，直接终止 Job Object 并按用户停止发布终态。

上述失败清理等待上限为 5 秒。终止调用失败或等待超时时，不得因进入 `failed` 就遗忘句柄；运行记录保留在清理表并继续重试终止，错误元数据记录 Win32 错误码，直到确认进程退出后才释放最后的进程和 Job Object 句柄。所有分支只发布一次终态。

### 停止

1. 原子转换为 `stopping`。
2. 首版不向任意程序伪造 Ctrl+C；直接请求终止该运行的 Job Object。
3. 最多等待 5 秒确认 Job Object 内进程全部退出。
4. 5 秒后仍有进程时返回 `process_stop_timeout` 并进入 `failed`，但项目单实例占位和清理记录继续保留，新的启动和重启都返回 `project_already_running`。
5. 成功条件是 Job Object 中无活动进程，随后发布唯一终态；只有句柄清理完成后才释放项目占位。

应用正常关闭时，对全部活动 Job Object 执行相同清理，但总等待时间上限为 5 秒，超时后记录错误并允许应用退出。强制结束应用的行为由 Job Object 的 kill-on-close 属性尽量兜底，但不作为断电场景的绝对保证。

## 日志与事件

事件：

- `project-process-status`
- `project-process-output`
- `project-process-exited`
- `project-process-error`
- 第二阶段：`project-preview-ready`

每个事件包含 `run_id`、`project_id` 和事件时间。输出事件还包含：

```text
seq: u64            # 每个 run_id 独立，从 1 开始
stream: stdout|stderr
text: string
truncated: boolean
```

日志按 UTF-8 解码，无效字节替换为替代字符。单个事件最大 32 KiB；超长无换行输出切块。每个运行在内存中保留最近 2 MiB，超限时从最旧内容开始淘汰并插入截断标记。因此界面提供的是“当前保留的完整日志”，不宣称包含已淘汰内容。

前端订阅后先记录当前最大 `seq`，再调用 `get_process_logs(after_seq)` 补齐间隙；按 `seq` 去重和排序。后端读取线程不得因前端消费慢而阻塞子进程管道。

stderr 是诊断流，不直接决定失败。失败依据是进程创建结果、后端状态错误或最终退出状态；退出码 0 为正常退出，非零退出显示为进程失败结果，但仍保留 stderr 上下文。

## Web 预览

第二阶段按以下顺序确定目标：

1. 用户明确配置 `expected_port` 和 `preview_scheme`。
2. 从结构化命令参数提取明确端口。
3. 从日志识别 `http://` 或 `https://` 本地地址及路径。
4. 无法确认时不生成预览链接。

打开前确认目标端口正在监听。Windows 上尽量校验监听 PID 属于当前 Job Object；如果无法确认归属，界面标记“端口归属未确认”并要求用户手动打开，而不是自动打开。保留日志给出的协议、主机和路径；仅在只有端口时使用 `http://127.0.0.1:<port>`。

字段使用 `auto_open_preview` 时表示验证成功后自动打开；首版建议只提供手动 `open_preview`，不保存自动打开选项。

## 错误分类

- `invalid_config`：程序为空、参数或环境变量非法。
- `invalid_working_directory`：目录不存在或越出项目根。
- `runtime_not_found`：Node、Python、Cargo 等不可解析。
- `confirmation_required`：配置未确认或确认哈希失效。
- `project_already_running`：项目已有活动实例或清理未完成的旧实例。
- `process_containment_failed`：无法将新进程纳入 Job Object。
- `process_spawn_failed`：权限不足或系统拒绝创建。
- `process_non_zero_exit`：进程创建成功但以非零状态退出，包括常见端口占用。
- `process_stop_timeout`：进程树未在 5 秒内退出。
- `run_not_found`：运行记录不存在或已淘汰。
- `preview_unavailable`：未发现可验证的预览目标。

前端展示错误摘要、错误码、命令摘要、工作目录、退出码和当前保留日志。敏感环境变量不得进入错误文本或日志元数据。

## 前端设计

### 项目页

- 项目类型、识别诊断和候选命令选择。
- 程序、参数、工作目录和环境变量配置。
- 启动、停止、重启按钮及状态禁用规则。
- 首次执行或配置变化时的确认对话框。
- PID、运行时长、退出码和最近错误。
- stdout/stderr 日志查看器，支持清空视图和复制当前保留日志。
- 第二阶段提供预览状态和打开操作。

前端状态只作为视图缓存；是否允许启动、停止和重启最终由后端状态机决定。

### 运行中心

第三阶段新增活动实例列表和已退出记录。默认每个项目保留最近 20 次已退出记录，最多保留 7 天；运行历史可落库，日志默认不跨应用重启持久化，除非后续单独设计文件日志。

## 安全边界

- 命令使用当前 Windows 用户权限在本机执行，界面必须明确提示这不是沙箱。
- 默认工作目录必须位于项目根内，并经过真实路径规范化。
- 程序和参数结构化传递；仅 `.cmd`、`.bat` 使用受控命令解释器。
- 不自动执行依赖安装、项目脚本或配置文件中的生命周期钩子。
- 用户选择的 `npm run`、`cargo run` 等命令可能间接执行项目代码，必须在首次运行或配置变化后确认。
- 敏感环境变量在界面、审计摘要和错误元数据中脱敏。
- 日志有单事件和总容量限制，避免异常程序耗尽内存。
- 后端执行项目级并发保护，不能只依赖按钮禁用。

## 验收标准

### 第一阶段

- Node fixture 的 `package.json` 包含 `dev`、`start` 时，识别顺序确定且参数结构正确；PATH 中可解析的 `node`、`npm` 等名称只通过系统解析得到绝对路径，不读取可执行文件内容。
- Rust 单 package fixture 通过 `cargo metadata --no-deps --format-version 1` 或标准入口存在性探测确认二进制 target 后生成 `cargo run`；仅 library target、不存在标准入口、Cargo 不在 PATH 或元数据调用失败时返回明确诊断；多成员 workspace 返回需选择目标的诊断，测试确认识别过程不读取 `src` 源码。
- Python fixture 优先识别项目脚本诊断，其次按规定顺序识别实际存在的入口文件和解释器；PATH 中的 `python`、`py` 仅做可执行解析和文件类型检查，不读取可执行文件内容；不存在的解释器只进入诊断。
- 配置中 `args=[]` 可以启动；空程序、NUL 参数或越界工作目录被后端拒绝。
- 两个并发启动请求只有一个创建进程，另一个返回 `project_already_running`。
- 测试进程输出 stdout、stderr 后，事件 `seq` 从 1 单调递增，查询补发不重复。
- 超过 2 MiB 的日志淘汰最旧内容并出现截断标记，后端不阻塞测试进程退出。
- 退出码 0 进入 `exited`；非零退出报告 `process_non_zero_exit`；仅 stderr 输出且退出码 0 不报告失败。
- 停止测试进程树后 5 秒内 Job Object 无活动进程；重复停止返回同一终态。
- 使用故障注入的 `Win32ProcessApi` 覆盖 Job 创建/配置失败、挂起创建失败、加入 Job 失败、恢复线程失败、终止调用失败、等待超时和句柄重复关闭；每个分支都验证不发布 `running`、只发布一次终态、清理表保留必要句柄且错误码可追踪。
- 使用可控屏障覆盖“挂起期间收到停止请求”“恢复与停止并发”“自然退出与停止竞态”“等待超时后清理完成”；验证不会漏杀、不会重复发布终态，未确认清理完成前项目占位不释放。
- 重启等待旧运行终止并产生新 `run_id`；旧运行停止失败时不创建新进程。
- 停止超时后项目仍被占位，新的启动请求返回 `project_already_running`，直到清理完成。
- 应用正常退出清理全部活动 Job Object，不永久占用测试端口。
- 确认 ID 只能兑换一次且兑换采用原子取出并删除；并发重复兑换最多一个成功，其余、过期和未知 ID 均返回 `confirmation_required`，有效期为签发后 10 分钟。
- 确认哈希在应用重启或新会话中失效；修改任一配置字段、环境变量值或 PATH 解析结果后，旧确认均返回 `confirmation_required`。
- 规范化测试覆盖字段顺序、缺省值、参数顺序、Windows 路径等价形式、环境变量大小写冲突及 JSON 编码稳定性；相同规范化配置必须得到相同哈希输入。
- 敏感环境变量在确认摘要、错误文本、日志元数据和持久化摘要中脱敏，且脱敏不改变后端内部哈希使用的原始值。

### 第二阶段

- 明确配置的监听端口通过验证后可打开预览。
- 日志中的 HTTPS 地址和路径被保留。
- 端口未监听时返回 `preview_unavailable`。
- 端口监听者无法确认属于当前运行时，不自动打开并展示归属提示。

### 第三阶段

- 运行中心能管理多个不同项目，项目页和运行中心状态一致。
- 同一项目默认只有一个活动实例。
- 每项目最多保留 20 条、最多 7 天的退出记录。
- 应用重启后能看到运行历史，但不会把旧运行误报为仍在运行。

## 验证策略

必跑自动化测试使用项目 fixture 和当前测试二进制生成短生命周期子进程，不依赖系统安装 Node、Python 或 Cargo。真实运行时集成测试仅在检测到对应工具时执行，否则明确跳过。

Rust 测试覆盖识别器、PATH 可执行解析、Cargo 元数据与入口存在性探测、禁止源码读取、路径规范化、配置校验、确认 ID 生命周期、跨会话与配置变化失效、敏感值脱敏、状态机、并发幂等、事件序列、日志截断、退出状态和 Job Object 清理。`Win32ProcessApi` 单元测试使用故障注入和可控屏障覆盖挂起、加入 Job、恢复、终止、超时及其竞态；真实 Windows 专属进程树测试在 Windows CI 或本机执行。前端测试覆盖候选选择、确认失效与脱敏摘要、状态按钮、日志去重、错误展示和预览条件。

## 实施约束

- 保持现有 Tauri v2、React、Zustand 和 Rust 架构。
- 优先复用已有项目服务、Tauri 事件、打开资源和错误模型。
- 不覆盖或回滚当前工作区中与本功能无关的未提交改动。
- 不把临时日志、fixture 输出、构建目录或安装包提交到仓库。
- 实施前创建细化计划，先完成第一阶段闭环，再分别规划预览和运行中心。
