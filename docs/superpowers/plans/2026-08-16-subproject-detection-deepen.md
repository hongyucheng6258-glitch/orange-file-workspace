# 子项目识别增强·补充实施计划（深度与状态查询）

> 前置：子项目识别（1 层 + 候选 cwd）已提交（`f5a3489` / `a10ee4b`）。
> 真实用户项目（`e:\work\毕业设计`）结构为 `web/backend`（Spring Boot）、
> `web/frontend/admin`、`web/frontend/student`（Vue3），最深 3 层，当前 1 层探测
> 无法命中。工具链 `mvn`(3.9.10)、`npm`、`node`、`java`(22) 均在系统 PATH。

## 变更点

1. **探测加深到 3 层（有限深度递归）**
   - `detect_subprojects` 改为递归：目录无直接配置时继续下探其子目录，最多 3 层，
     仍跳过噪音目录（`node_modules`、`.git`、`target`、`.venv`、`dist`、`build`、
     `__pycache__`、`.idea`、`.vscode`）。
   - 候选 `cwd` 与 label 前缀使用相对项目根的路径（如 `web/backend`、
     `web/frontend/admin`）。
   - 预期对 `毕业设计` 识别出：`web/backend: mvn spring-boot:run`、
     `web/frontend/admin: npm run dev`、`web/frontend/student: npm run dev`。
   - 说明：单实例键基于规范化工作目录（`canonical(cwd)`），子项目 cwd 不同即可
     并行运行前后端，无需改单实例逻辑。

2. **项目页状态查询适配子项目**
   - `RuntimeManager` 新增 `get_run_by_project_id(project_id)`：活动运行按
     `project_id` 匹配（多个取最近），无活动时回退 `list_runs(true)` 按
     `project_id` 取最近终态。
   - 命令 `get_project_run` 先查项目根键（保持原语义），无结果时回退
     `get_run_by_project_id`，使项目页与运行中心状态一致。

3. 前端无需改动（仍按 `project_id` 查询）。

## 测试

- 识别器：3 层结构（`web/backend` + `web/frontend/admin`）产生带正确相对 cwd 的候选；
  3 层深度不再下探；噪音目录在任意层均跳过；1 层结构回归通过。
- `get_run_by_project_id`：子项目运行（cwd 为子目录）可被项目页查询命中；
  多个子项目运行取最近；无运行时返回 None。

## 验证与提交

- `cargo test project_detector project_runtime`、`npm test`、`npm run build`、
  `cargo clippy`（新代码无警告）、`tauri build`。
- 提交：
  1. `feat(project-detector): deepen subproject scan to 3 levels`
  2. `feat(project-runtime): query project run by project id for subprojects`
