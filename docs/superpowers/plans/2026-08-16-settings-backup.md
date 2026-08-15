# 应用设置中心实施计划 · 备份管理完善与自动备份

对应设计文档第 6 节。本计划在计划 1（设置核心）之后执行，交付：备份记录删除/导出/打开目录、恢复安全增强（保护备份、版本校验、失败回滚）、每天/每周自动备份与保留数量清理。

## 前置说明

- 依赖：先完成 `2026-08-16-settings-core.md` 的阶段 1（`settings_service.rs` 已含 backup 设置项）。
- 本机无 git，提交步骤在 git 可用时执行。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml`；前端 `npm run build`、`npm test`。

## 阶段 4：备份管理完善

### 任务 4.1 迁移 0007：备份来源标记

新建 `src-tauri/migrations/0007_backup_source.sql`：

```sql
-- 备份记录来源：manual（手动）、auto（自动）、protect（恢复前保护备份）
ALTER TABLE backup_records ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';
```

修改 `src-tauri/src/db/migrations.rs`：在 `MIGRATIONS` 数组末尾追加：

```rust
    Migration {
        version: 7,
        name: "backup_source",
        sql: include_str!("../../migrations/0007_backup_source.sql"),
    },
```

在 `tests::all_expected_tables_exist` 后的 `global_search_tables_exist` 前不需要改动；`applies_all_migrations` 会自动覆盖 version 7。

### 任务 4.2 备份服务增强

修改 `src-tauri/src/services/backup_service.rs`：

- `BackupRecord` 增加 `source` 字段（`src-tauri/src/db/models.rs`）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRecord {
    pub id: String,
    pub path: String,
    pub backup_type: String,
    pub database_version: i64,
    pub resource_count: Option<i64>,
    pub file_count: Option<i64>,
    pub created_at: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub source: String,
}
```

- `create_backup` 增加 `source: &str` 参数，插入记录时写入 `source` 列，构造 `BackupRecord` 时填充：

```rust
pub fn create_backup(
    state: &AppState,
    include_files: bool,
    source: &str,
) -> Result<(PathBuf, crate::db::models::BackupRecord), AppError> {
    ...
    let record = crate::db::models::BackupRecord {
        ...
        source: source.to_string(),
    };
    conn.execute(
        "INSERT INTO backup_records (
            id, path, backup_type, database_version, resource_count,
            file_count, created_at, status, error_message, source
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'completed', NULL, ?8)",
        params![
            record.id, record.path, record.backup_type, record.database_version,
            record.resource_count, record.file_count, record.created_at, record.source
        ],
    )?;
```

- `list_backups` 的行映射追加 `source: row.get("source")?`。
- `commands/backups.rs::create_backup` 传 `"manual"`，保留原 IPC 签名。

### 任务 4.3 备份校验与删除、导出、打开

在 `backup_service.rs` 新增：

```rust
/// 校验备份目录完整性，返回 manifest 关键字段。
pub fn validate_backup(backup_dir: &Path) -> Result<serde_json::Value, AppError> {
    let manifest_path = backup_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(AppError::new("invalid_backup", "备份缺少 manifest.json"));
    }
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)?;
    if manifest.get("app").and_then(|v| v.as_str()) != Some(env!("CARGO_PKG_NAME")) {
        return Err(AppError::new("invalid_backup", "备份不属于本应用"));
    }
    let db_version = manifest
        .get("database_version")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let current = crate::db::migrations::MIGRATIONS
        .last()
        .map(|m| m.version)
        .unwrap_or(0);
    if db_version > current {
        return Err(AppError::new(
            "incompatible_backup",
            format!("备份数据库版本 {db_version} 高于当前 {current}，无法恢复"),
        ));
    }
    if !backup_dir.join("workspace.db").exists() {
        return Err(AppError::new("invalid_backup", "备份缺少 workspace.db"));
    }
    Ok(manifest)
}

/// 删除备份：先删记录，再删目录。目录删除失败时返回错误但记录已删除。
pub fn delete_backup(state: &AppState, backup_id: &str) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock");
    let path: Option<String> = conn
        .query_row("SELECT path FROM backup_records WHERE id = ?1", [backup_id], |r| r.get(0))
        .optional()?;
    conn.execute("DELETE FROM backup_records WHERE id = ?1", [backup_id])?;
    drop(conn);
    if let Some(p) = path {
        let dir = PathBuf::from(p);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
    }
    Ok(())
}

/// 把备份复制到用户指定目录（导出）。
pub fn export_backup(state: &AppState, backup_id: &str, dest_dir: &Path) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock");
    let src: String = conn
        .query_row("SELECT path FROM backup_records WHERE id = ?1", [backup_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::new("not_found", "备份不存在"))?;
    drop(conn);
    let src = PathBuf::from(src);
    if !src.exists() {
        return Err(AppError::new("not_found", "备份目录已不存在"));
    }
    std::fs::create_dir_all(dest_dir)?;
    let ts = chrono_like_timestamp();
    let dest = dest_dir.join(format!("backup-{ts}"));
    copy_dir_all(&src, &dest)?;
    Ok(())
}

/// 打开备份根目录（资源管理器）。
pub fn reveal_backups_dir(state: &AppState) -> Result<(), AppError> {
    let dir = state.data_dir.join("backups");
    std::fs::create_dir_all(&dir)?;
    let handle = tauri::Manager::app_handle(&state.conn);
    drop(handle);
    Ok(())
}
```

说明：`reveal_backups_dir` 需要 `AppHandle`，因此不在 `backup_service` 内实现，改由命令层完成（见 4.4）。

### 任务 4.4 恢复安全增强

重写 `backup_service.rs::restore_from_dir`：

```rust
/// 从备份目录恢复。流程：校验 → 创建保护备份 → 恢复数据库（失败回滚）→ 恢复托管文件（失败回滚）。
pub fn restore_from_dir(state: &AppState, backup_dir: &Path) -> Result<(), AppError> {
    validate_backup(backup_dir)?;

    // 1. 保护备份：当前数据库快照
    let (protect_dir, _) = create_backup(state, false, "protect")?;

    // 2. 恢复数据库；失败时从保护备份回滚
    if let Err(e) = restore_db_snapshot(state, backup_dir) {
        let _ = restore_db_snapshot(state, &protect_dir);
        return Err(AppError::new(
            "restore_failed",
            format!("数据库恢复失败，已回滚到恢复前状态: {e}"),
        ));
    }

    // 3. 恢复托管文件（若备份含托管文件）
    let managed_src = backup_dir.join("managed-files");
    if managed_src.exists() {
        // 先把当前托管目录整体移动到保护位置（移动不占双份空间）
        let protect_managed = protect_dir.join("managed-files-current");
        let managed_dst = state.managed_dir.clone();
        if managed_dst.exists() {
            std::fs::rename(&managed_dst, &protect_managed)?;
        }
        if let Err(e) = copy_dir_all(&managed_src, &managed_dst) {
            // 回滚：删除不完整的恢复目录，把原目录移回
            let _ = std::fs::remove_dir_all(&managed_dst);
            if protect_managed.exists() {
                let _ = std::fs::rename(&protect_managed, &managed_dst);
            }
            return Err(AppError::new(
                "restore_failed",
                format!("托管文件恢复失败，已回滚: {e}"),
            ));
        }
        // 校验恢复结果：文件数一致
        let src_count = count_files(&managed_src);
        let dst_count = count_files(&managed_dst);
        if src_count != dst_count {
            let _ = std::fs::remove_dir_all(&managed_dst);
            if protect_managed.exists() {
                let _ = std::fs::rename(&protect_managed, &managed_dst);
            }
            return Err(AppError::new(
                "restore_failed",
                "托管文件恢复校验不一致，已回滚",
            ));
        }
    }

    Ok(())
}

/// 用 SQLite Backup API 把备份库恢复到当前连接。
fn restore_db_snapshot(state: &AppState, backup_dir: &Path) -> Result<(), AppError> {
    let db_file = backup_dir.join("workspace.db");
    let src = Connection::open(&db_file)?;
    let mut conn = state.conn.lock().expect("db lock");
    let backup = Backup::new(&src, &mut conn)?;
    backup.run_to_completion(5, Duration::from_millis(200), None)?;
    Ok(())
}

/// 统计目录内文件数（含子目录）。
fn count_files(dir: &Path) -> u64 {
    fn walk(d: &Path, n: &mut u64) {
        if let Ok(entries) = std::fs::read_dir(d) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, n);
                } else {
                    *n += 1;
                }
            }
        }
    }
    let mut n = 0;
    walk(dir, &mut n);
    n
}
```

保留原 `copy_dir_all` 与 `chrono_like_timestamp` 函数。恢复成功后前端提示刷新。

### 任务 4.5 命令层：删除/导出/打开/恢复

修改 `src-tauri/src/commands/backups.rs`：

```rust
use tauri::State;

use crate::AppState;
use crate::ipc::CommandResult;
use crate::services::backup_service;
use rusqlite::OptionalExtension;

/// 删除一条备份记录及其目录。
#[tauri::command]
pub fn delete_backup(state: State<AppState>, backup_id: String) -> CommandResult<()> {
    backup_service::delete_backup(&state, &backup_id)?;
    Ok(())
}

/// 导出备份到指定目录。
#[tauri::command]
pub fn export_backup(
    state: State<AppState>,
    backup_id: String,
    dest_dir: String,
) -> CommandResult<()> {
    backup_service::export_backup(&state, &backup_id, std::path::Path::new(&dest_dir))?;
    Ok(())
}

/// 在资源管理器中打开备份根目录。
#[tauri::command]
pub fn reveal_backups(state: State<AppState>) -> CommandResult<String> {
    let dir = state.data_dir.join("backups");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.to_string_lossy().to_string())
}

/// 校验备份是否可恢复（前端在确认恢复前调用）。
#[tauri::command]
pub fn validate_backup(
    state: State<AppState>,
    backup_id: String,
) -> CommandResult<serde_json::Value> {
    let conn = state.conn.lock().expect("db lock");
    let path: Option<String> = conn
        .query_row("SELECT path FROM backup_records WHERE id = ?1", [&backup_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| crate::error::AppError::new("not_found", "备份不存在"))?;
    drop(conn);
    backup_service::validate_backup(std::path::Path::new(&path))
}
```

`restore_backup` 命令保持现有签名，但恢复失败时前端需能读取保护备份位置（错误消息已包含说明）。

在 `lib.rs` 注册新命令（追加在 `commands::backups::app_environment` 后）：

```rust
            commands::backups::delete_backup,
            commands::backups::export_backup,
            commands::backups::reveal_backups,
            commands::backups::validate_backup,
```

### 任务 4.6 前端备份组件增强

重写 `src/features/settings/components/BackupSettings.tsx`（替换原搬入的旧逻辑）：

```tsx
import { useCallback, useEffect, useState } from "react";
import { Archive, FolderOpen, RotateCcw, Trash2, Download } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { call, formatTime } from "../../../lib/tauri";
import { useSettingsStore } from "../stores/settingsStore";
import { SaveBadge, SettingRow } from "./SettingRow";

interface BackupRecord {
  id: string;
  path: string;
  backup_type: string;
  database_version: number;
  resource_count: number | null;
  file_count: number | null;
  created_at: number;
  status: string;
  error_message: string | null;
  source: string;
}

const SOURCE_LABEL: Record<string, string> = {
  manual: "手动",
  auto: "自动",
  protect: "保护",
};

export function BackupSettings() {
  const { settings, saveState, update } = useSettingsStore();
  const [backups, setBackups] = useState<BackupRecord[]>([]);
  const [creating, setCreating] = useState(false);
  const [restoring, setRestoring] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const list = await call<BackupRecord[]>("list_backups", {});
      setBackups(list);
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const doBackup = async (includeFiles: boolean) => {
    setCreating(true);
    setError(null);
    try {
      await call<BackupRecord>("create_backup", { includeFiles });
      await load();
    } catch (e) {
      setError(`备份失败：${(e as Error).message}`);
    } finally {
      setCreating(false);
    }
  };

  const doRestore = async (rec: BackupRecord) => {
    if (!window.confirm(`恢复将覆盖当前所有数据，确定继续？\n${rec.path}`)) return;
    setRestoring(rec.id);
    setError(null);
    try {
      await call("restore_backup", { backupPath: rec.path });
      await load();
      window.alert("恢复完成，请刷新界面");
    } catch (e) {
      setError(`恢复失败：${(e as Error).message}`);
    } finally {
      setRestoring(null);
    }
  };

  const doDelete = async (rec: BackupRecord) => {
    if (!window.confirm("删除后不可恢复，确定删除该备份？")) return;
    setBusy(rec.id);
    setError(null);
    try {
      await call("delete_backup", { backupId: rec.id });
      await load();
    } catch (e) {
      setError(`删除失败：${(e as Error).message}`);
    } finally {
      setBusy(null);
    }
  };

  const doExport = async (rec: BackupRecord) => {
    const dir = await open({ directory: true, title: "选择导出位置" });
    if (!dir || typeof dir !== "string") return;
    setBusy(rec.id);
    setError(null);
    try {
      await call("export_backup", { backupId: rec.id, destDir: dir });
    } catch (e) {
      setError(`导出失败：${(e as Error).message}`);
    } finally {
      setBusy(null);
    }
  };

  const doReveal = async () => {
    try {
      const dir = await call<string>("reveal_backups", {});
      await openPath(dir);
    } catch (e) {
      setError(`打开失败：${(e as Error).message}`);
    }
  };

  if (!settings) return null;
  const b = settings.backup;

  return (
    <div className="settings-section">
      <h3>自动备份</h3>
      <SaveBadge state={saveState} />

      <SettingRow label="启用自动备份" desc="按设定周期自动创建备份">
        <input
          type="checkbox"
          checked={b.enabled}
          onChange={(e) => update("backup.enabled", e.target.checked)}
          style={{ accentColor: "var(--primary)", width: 16, height: 16 }}
        />
      </SettingRow>

      <SettingRow label="备份周期">
        <select
          className="input"
          value={b.frequency}
          onChange={(e) => update("backup.frequency", e.target.value)}
        >
          <option value="daily">每天</option>
          <option value="weekly">每周</option>
        </select>
      </SettingRow>

      <SettingRow label="执行时间" desc="错过时间后将在下次启动时补执行一次">
        <input
          className="input"
          type="time"
          value={b.run_time}
          onChange={(e) => update("backup.run_time", e.target.value)}
        />
      </SettingRow>

      <SettingRow label="保留数量" desc="自动备份最多保留的数量，超出后清理最旧的">
        <input
          className="input"
          type="number"
          min={1}
          max={30}
          value={b.retention_count}
          onChange={(e) => {
            const n = Number(e.target.value);
            if (Number.isFinite(n) && n >= 1 && n <= 30) {
              update("backup.retention_count", n);
            }
          }}
        />
      </SettingRow>

      <SettingRow label="备份内容">
        <select
          className="input"
          value={b.backup_type}
          onChange={(e) => update("backup.backup_type", e.target.value)}
        >
          <option value="full">完整备份（含托管文件）</option>
          <option value="metadata">仅元数据</option>
        </select>
      </SettingRow>

      <h3 style={{ marginTop: 20 }}>立即备份</h3>
      {error && (
        <div className="settings-error" role="alert">
          {error}
        </div>
      )}
      <div className="settings-actions">
        <button className="btn" disabled={creating} onClick={() => doBackup(false)}>
          <Archive size={14} /> 元数据备份
        </button>
        <button className="btn btn-primary" disabled={creating} onClick={() => doBackup(true)}>
          <Archive size={14} /> 完整备份
        </button>
        <button className="btn" onClick={doReveal}>
          <FolderOpen size={14} /> 打开备份目录
        </button>
      </div>

      <h3 style={{ marginTop: 20 }}>备份记录</h3>
      {backups.length === 0 ? (
        <p className="settings-note">暂无备份记录</p>
      ) : (
        <div className="backup-list">
          {backups.map((rec) => (
            <div key={rec.id} className="backup-item">
              <div className="backup-info">
                <span className="backup-type">
                  {rec.backup_type === "full" ? "完整备份" : "元数据备份"}
                  <span className="backup-source">{SOURCE_LABEL[rec.source] ?? rec.source}</span>
                </span>
                <span className="backup-meta">
                  {formatTime(rec.created_at)} · {rec.resource_count ?? 0} 资源
                  {rec.status !== "completed" && ` · ${rec.status}`}
                  {rec.error_message && ` · ${rec.error_message}`}
                </span>
              </div>
              <div className="backup-item-actions">
                <button
                  className="icon-btn"
                  title="导出"
                  disabled={busy === rec.id}
                  onClick={() => doExport(rec)}
                >
                  <Download size={14} />
                </button>
                <button
                  className="icon-btn"
                  title="删除"
                  disabled={busy === rec.id}
                  onClick={() => doDelete(rec)}
                >
                  <Trash2 size={14} />
                </button>
                <button
                  className="btn btn-ghost"
                  disabled={restoring === rec.id || busy === rec.id}
                  onClick={() => doRestore(rec)}
                >
                  <RotateCcw size={13} /> 恢复
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
```

`package.json` 已有 `@tauri-apps/plugin-dialog` 与 `@tauri-apps/plugin-opener` 依赖，前端直接 `import { open } from "@tauri-apps/plugin-dialog"`、`import { openPath } from "@tauri-apps/plugin-opener"`。

在 `app.css` 追加：

```css
.backup-source {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: var(--radius-full);
  background: var(--surface-hover);
  color: var(--text-tertiary);
  margin-left: 8px;
}

.backup-item-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 0 0 auto;
}
```

### 任务 4.7 阶段 4 测试

`backup_service.rs` 测试更新：

- `create_backup` 调用追加 `"manual"` 参数。
- 新增测试：

```rust
#[test]
fn delete_backup_removes_record_and_dir() {
    let (state, dir) = test_state();
    let (backup_dir, record) = create_backup(&state, false, "manual").expect("backup");
    assert!(backup_dir.exists());
    delete_backup(&state, &record.id).expect("delete");
    let conn = state.conn.lock().expect("lock");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM backup_records", [], |r| r.get(0))
        .expect("count");
    assert_eq!(count, 0);
    drop(conn);
    assert!(!backup_dir.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn validate_backup_rejects_incompatible_version() {
    let (state, dir) = test_state();
    let (backup_dir, _) = create_backup(&state, false, "manual").expect("backup");
    // 篡改 manifest 版本为未来版本
    let manifest_path = backup_dir.join("manifest.json");
    let mut m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    m["database_version"] = serde_json::json!(9999);
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&m).unwrap()).unwrap();
    assert!(validate_backup(&backup_dir).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restore_creates_protect_backup_and_rolls_back_db() {
    let (state, dir) = test_state();
    {
        let conn = state.conn.lock().expect("lock");
        conn.execute(
            "INSERT INTO resources (id, kind, name, created_at, updated_at)
             VALUES ('r1', 'file', 'a.txt', 1, 1)",
            [],
        ).expect("insert");
    }
    let (backup_dir, _) = create_backup(&state, false, "manual").expect("backup");

    // 修改原库并删除备份目录中的 db（制造恢复失败）
    {
        let conn = state.conn.lock().expect("lock");
        conn.execute(
            "INSERT INTO resources (id, kind, name, created_at, updated_at)
             VALUES ('r2', 'file', 'b.txt', 1, 1)",
            [],
        ).expect("insert2");
    }
    std::fs::remove_file(backup_dir.join("workspace.db")).unwrap();

    assert!(restore_from_dir(&state, &backup_dir).is_err());

    // 数据库应回滚：r1 仍在、r2 仍在（恢复失败未覆盖），且保护备份已生成
    let conn = state.conn.lock().expect("lock");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
        .expect("count");
    assert_eq!(count, 2, "db content preserved after failed restore");
    let protects: i64 = conn
        .query_row("SELECT COUNT(*) FROM backup_records WHERE source = 'protect'", [], |r| r.get(0))
        .expect("count");
    assert_eq!(protects, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
```

运行 `cargo test --manifest-path src-tauri/Cargo.toml backup_service` 通过。

---

## 阶段 5：自动备份调度

### 任务 5.1 本地时间辅助

在 `backup_service.rs` 增加本地时间工具（不引入新 crate，Windows 用 GetLocalTime）：

`Cargo.toml` 的 windows features 追加 `"Win32_System_SystemInformation"`：

```toml
    "Win32_System_SystemServices",
    "Win32_System_SystemInformation",
```

```rust
/// 本地时间（时, 分）。
#[cfg(target_os = "windows")]
fn local_hhmm() -> (u8, u8) {
    use windows::Win32::System::SystemInformation::{GetLocalTime, SYSTEMTIME};
    unsafe {
        let mut st = SYSTEMTIME::default();
        GetLocalTime(&mut st);
        (st.wHour as u8, st.wMinute as u8)
    }
}

#[cfg(not(target_os = "windows"))]
fn local_hhmm() -> (u8, u8) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (((secs / 3600) % 24) as u8, ((secs / 60) % 60) as u8)
}
```

### 任务 5.2 调度检查与保留清理

在 `backup_service.rs` 新增：

```rust
use crate::services::settings_service::{self, KEY_BACKUP_RUN_TIME};

/// 自动备份记录键（app_settings）。
pub const KEY_LAST_RUN_TS: &str = "backup.last_run_ts";

/// 启动后台调度线程：每 60 秒检查一次是否应执行自动备份。
pub fn start_backup_scheduler(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        let _ = run_auto_backup_if_due(&app);
        std::thread::sleep(Duration::from_secs(60));
    });
}

/// 若满足条件则执行一次自动备份并清理超量记录。
/// 条件：启用、当前本地时间 >= run_time、本周期未执行过。
pub fn run_auto_backup_if_due(app: &tauri::AppHandle) -> Result<(), AppError> {
    let state = app.state::<crate::AppState>();
    let settings = {
        let conn = state.conn.lock().expect("db lock");
        settings_service::load_settings(&conn)?
    };
    if !settings.backup.enabled {
        return Ok(());
    }

    let (now_h, now_m) = local_hhmm();
    let parsed = parse_hhmm(&settings.backup.run_time).ok_or_else(|| {
        AppError::new("invalid_setting", "自动备份时间格式错误")
    })?;
    if (now_h, now_m) < parsed {
        return Ok(()); // 今天的时间点未到
    }

    let last_run: i64 = {
        let conn = state.conn.lock().expect("db lock");
        crate::db::repositories::get_setting(&conn, KEY_LAST_RUN_TS)?
            .and_then(|v| serde_json::from_str::<i64>(&v).ok())
            .unwrap_or(0)
    };
    let now = now_unix();
    let due = match settings.backup.frequency.as_str() {
        "weekly" => now.saturating_sub(last_run) >= 7 * 86400,
        _ => now / 86400 != last_run / 86400, // daily：非今天
    };
    if !due {
        return Ok(());
    }

    let include_files = settings.backup.backup_type == "full";
    create_backup(&state, include_files, "auto")?;
    {
        let conn = state.conn.lock().expect("db lock");
        crate::db::repositories::set_setting(&conn, KEY_LAST_RUN_TS, &serde_json::to_string(&now)?)?;
    }
    enforce_retention(&state, settings.backup.retention_count)?;
    Ok(())
}

/// 解析 "HH:MM"。
fn parse_hhmm(s: &str) -> Option<(u8, u8)> {
    let (h, m) = s.split_once(':')?;
    let h: u8 = h.parse().ok()?;
    let m: u8 = m.parse().ok()?;
    if h < 24 && m < 60 { Some((h, m)) } else { None }
}

/// 清理自动备份，使数量不超过 retention。
pub fn enforce_retention(state: &AppState, retention: u32) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock");
    let ids: Vec<(String, String)> = conn
        .prepare(
            "SELECT id, path FROM backup_records
             WHERE source = 'auto' AND status = 'completed'
             ORDER BY created_at DESC",
        )?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let keep = retention as usize;
    if ids.len() <= keep {
        return Ok(());
    }
    for (id, path) in ids.into_iter().skip(keep) {
        conn.execute("DELETE FROM backup_records WHERE id = ?1", [&id])?;
        let dir = PathBuf::from(path);
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    Ok(())
}

/// 启动时补执行：若已过 run_time 且本周期未执行，立即执行一次。
pub fn run_auto_backup_on_startup(app: &tauri::AppHandle) {
    let _ = run_auto_backup_if_due(app);
}
```

### 任务 5.3 注册调度

修改 `src-tauri/src/lib.rs`：

- `services::backup_service::start_backup_scheduler(app.handle().clone());` 加在 `start_managed_watcher` 之后（setup 内）：

```rust
            services::watcher_service::start_managed_watcher(app.handle().clone());
            services::backup_service::start_backup_scheduler(app.handle().clone());
```

### 任务 5.4 自动备份测试

在 `backup_service.rs` 测试模块追加：

```rust
#[test]
fn parse_hhmm_accepts_valid_and_rejects_invalid() {
    assert_eq!(parse_hhmm("02:00"), Some((2, 0)));
    assert_eq!(parse_hhmm("23:59"), Some((23, 59)));
    assert_eq!(parse_hhmm("24:00"), None);
    assert_eq!(parse_hhmm("12:60"), None);
    assert_eq!(parse_hhmm("abc"), None);
}

#[test]
fn enforce_retention_keeps_only_newest() {
    let (state, dir) = test_state();
    for i in 0..5 {
        let (backup_dir, record) = create_backup(&state, false, "auto").expect("backup");
        // 让 created_at 递增：直接更新记录时间
        let conn = state.conn.lock().expect("lock");
        conn.execute(
            "UPDATE backup_records SET created_at = ?1 WHERE id = ?2",
            rusqlite::params![1000 + i, record.id],
        ).expect("update time");
        drop(conn);
        let _ = &backup_dir;
    }
    enforce_retention(&state, 2).expect("retention");
    let conn = state.conn.lock().expect("lock");
    let remaining: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM backup_records WHERE source = 'auto'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(remaining, 2, "only newest two remain");
    let _ = std::fs::remove_dir_all(&dir);
}
```

注意 `create_backup` 测试里 `let _ = &backup_dir;` 仅为消除未使用告警；实际断言可省略。

运行 `cargo test --manifest-path src-tauri/Cargo.toml`，全部通过。

### 任务 5.5 全量验证

1. Rust 全部测试通过。
2. `npm run build` 通过（`@tauri-apps/plugin-dialog`、`@tauri-apps/plugin-opener` 类型可用）。
3. `npm test` 通过。
4. 手动验证（`npm run tauri dev`）：
   - 立即备份（元数据/完整）成功后出现在列表，来源标记「手动」。
   - 删除备份记录后目录同步消失。
   - 导出备份到用户目录成功。
   - 打开备份目录在资源管理器弹出。
   - 恢复含托管文件的备份：恢复后文件数一致；制造损坏备份时提示「备份不属于本应用/版本不兼容」且不进入恢复。
   - 开启自动备份（周期每天、时间设为 1 分钟后）：到点自动生成「自动」来源备份；保留数量超出时最旧的自动备份被清理，手动与保护备份不受影响。
   - 手动备份不受保留数量清理影响。

## 交付物核对

- 迁移 0007 + `BackupRecord.source`。
- `backup_service.rs`：校验、删除、导出、保护备份、失败回滚、本地时间、调度、保留清理及测试。
- `commands/backups.rs`：delete/export/reveal/validate 命令与注册。
- 前端 `BackupSettings.tsx`：自动备份设置 + 立即备份 + 记录管理（删除/导出/打开/恢复）。
- 下一计划：`2026-08-16-settings-migration.md`（目录迁移）。
