# C 盘双模式清理脚本实施计划

日期：2026-08-18

## 文件结构

- `scripts/clean-c-drive.ps1`：最终脚本，包含白名单定义、扫描、交互选择、确认、删除与结果输出。
- 临时测试脚本：验证参数默认值、静态白名单、危险路径保护、扫描模式和回收站确认，不进入最终项目目录。

## 任务

1. 编写失败测试，断言最终脚本存在且公开 `Safe`、`Deep`、`ScanOnly`、`ConfirmRecycleBin` 接口。
2. 运行测试，确认因脚本尚不存在而失败。
3. 实现参数解析、清理项定义和固定白名单。
4. 实现路径规范化、危险路径拒绝、重解析点跳过和限量扫描。
5. 实现普通目录扫描及浏览器多 profile 缓存发现。
6. 实现交互式逐项选择，安全模式默认选中安全项，深度模式只默认选中低风险项。
7. 实现删除统计、回收站独立确认、管理员权限提示和 JSON 输出。
8. 运行静态安全测试并修复失败项。
9. 使用 `-ScanOnly -OutputJson` 做无删除烟雾测试，确认能扫描且不执行清理。
10. 执行 PowerShell 语法解析，确保兼容 Windows PowerShell 5.1。

## 验证命令

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File <测试脚本>
powershell -NoProfile -Command "[void][scriptblock]::Create((Get-Content -Raw 'scripts\\clean-c-drive.ps1'))"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\\clean-c-drive.ps1 -ScanOnly -OutputJson
```

预期：测试全部通过；语法解析退出码为 `0`；扫描输出合法 JSON，且每个项目的删除数和释放字节均为 `0`。
