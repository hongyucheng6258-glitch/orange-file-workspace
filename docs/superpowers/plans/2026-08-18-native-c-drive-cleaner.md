# C 盘清理 Rust 原生接入实施计划

日期：2026-08-18

## 文件结构

- 新增 `src-tauri/src/services/c_drive_cleaner.rs`：清理目录白名单、扫描、路径保护、删除与回收站 API。
- 修改 `src-tauri/src/services/mod.rs`：导出服务。
- 修改 `src-tauri/src/commands/system.rs`：增加扫描与清理命令。
- 修改 `src-tauri/src/lib.rs`：注册命令。
- 修改 `src/lib/types.ts`：增加扫描和执行结果类型。
- 新增 `src/features/system/lib/cDriveCleaner.ts`：前端模式、默认选择和结果汇总纯函数。
- 新增 `src/features/system/lib/cDriveCleaner.test.ts`：前端纯逻辑测试。
- 新增 `src/features/system/components/CDriveCleaner.tsx`：扫描、选择、确认和结果界面。
- 修改 `src/features/system/components/ToolsTab.tsx`：挂载清理组件。
- 修改 `src/styles/app.css`：清理区域样式。

## TDD 步骤

1. 在 Rust 新模块先写目录数量、默认选择、未知 ID、回收站确认、临时目录扫描/清理及重解析点保护测试。
2. 运行目标测试，确认因接口尚不存在而失败。
3. 实现最小扫描与清理服务，使目标测试通过。
4. 在前端先写默认选择和汇总纯函数测试并确认失败。
5. 实现纯函数与类型，使前端测试通过。
6. 实现 `CDriveCleaner` 组件并接入工具页。
7. 注册后端命令并执行格式、类型和测试验证。

## 验证

```powershell
npm run test:rust
npm run test
npm run typecheck
npm run fmt
npm run clippy
```

预期所有命令退出码为 `0`，且清理服务测试不操作真实系统缓存，只使用测试临时目录验证删除行为。
