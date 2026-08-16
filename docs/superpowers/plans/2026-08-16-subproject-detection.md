# 子项目识别增强实施计划

> 背景：Spring Boot + Vue3 等前后端分离项目把 `pom.xml` / `package.json` 放在
> `backend/`、`frontend/` 子目录，当前识别器只扫描项目根直接子文件，导致整项目
> 报"未识别到受支持的运行时配置文件"。
>
> 目标：当项目根目录没有直接配置文件时，做**一层**子目录探测，识别 `backend` /
> `frontend` 等子项目并生成带正确工作目录的候选命令；不递归深层目录，不扫描噪音目录。

## 设计要点

1. 候选结构扩展：`RuntimeCandidate` 增加 `cwd: Option<String>`（相对项目根的子目录，
   `None` = 项目根）。现有 `push_candidate` 保持内部默认 `None`，新增
   `push_candidate_in(label, exe, args, confidence, cwd)` 供子项目候选使用。
2. 子项目探测仅当根目录 `detect_into` 未识别到任何 marker 时启用；扫描根目录
   **直接子目录**（1 层），跳过噪音目录：`node_modules`、`.git`、`target`、`.venv`、
   `dist`、`build`、`__pycache__`、`.idea`、`.vscode`。
3. 每个子目录复用现有 `detect_into`（不递归触发子探测，防止深层扫描与循环）。
4. 子项目候选：`label` 加 `"<子目录>: "` 前缀便于区分，`cwd` 设为子目录名；
   子项目诊断加前缀透出。
5. 项目根已有配置时完全不触发子探测（优先级不变）。
6. `ProjectFs` 增加 `list_child_dirs(&Path) -> Vec<PathBuf>`；磁盘实现读目录过滤目录，
   测试替身从 `dirs` 字段推断。
7. 前端：`pickCandidate` / `defaultConfig` 把 `candidate.cwd` 应用到配置的 `cwd`
   （空 = 项目根）。运行配置的路径安全校验（相对项目根解析、边界检查）已在后端完成，
   无需改动。

## 文件映射

后端：

- `src-tauri/src/services/project_detector.rs`：`RuntimeCandidate.cwd`、
  `ProjectFs::list_child_dirs`、`detect_into` 重构、`detect_subprojects`、噪音目录过滤、
  诊断文本更新。
- `src-tauri/src/services/project_detector.rs`（测试）：子项目识别、噪音目录跳过、
  根配置优先、cwd 断言、不读源码。

前端：

- `src/features/projects/lib/projectRuntime.ts`：`RuntimeCandidate` 类型加 `cwd?: string`。
- `src/features/projects/stores/projectRuntimeStore.ts`：`defaultConfig` 与 `pickCandidate`
  应用候选 `cwd`。
- `src/features/projects/stores/projectRuntimeStore.test.ts`：候选 cwd 应用测试。

## 实施顺序

1. 计划提交：`docs(project-runtime): add subproject detection plan`
2. 后端检测器 + 测试，`cargo test project_detector`
3. 前端类型 + store + 测试，`npm test`、`npm run build`
4. 全量验证（`cargo test`、`npm test`、`npm run build`、`tauri build`）并选择性提交：
   - `feat(project-detector): detect subprojects with per-candidate cwd`
   - `feat(projects): apply candidate cwd when picking run config`

## 验收清单

- `backend/pom.xml`（Spring Boot 插件）+ `frontend/package.json`（dev 脚本）的分离项目，
  在项目根识别出 2 个候选，cwd 分别为 `backend` / `frontend`。
- 噪音目录（`node_modules`、`.git`、`target`、`.venv`）不被探测。
- 根目录有配置文件时只出根候选，不触发子探测。
- 子项目候选的 cwd 进入前端配置并被后端路径校验接受。
- 识别过程不读取项目源码（`backend/src/**` 等）。
- `git status` 无临时文件，其他未提交改动未被混入。
