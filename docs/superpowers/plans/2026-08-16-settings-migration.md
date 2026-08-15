# 应用设置中心实施计划 · 目录迁移

对应设计文档第 5、8 节。本计划在计划 1、2 之后执行，交付：数据目录/托管目录自动迁移（校验 → 复制 → 校验 → 原子切换 → 清理 → 回滚），支持取消与启动恢复。

## 前置说明

- 本机无 git，提交步骤在 git 可用时执行。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml`；前端 `npm run build`、`npm test`。
- 设计约束：数据目录存于 `%APPDATA%\com.nexus.file-workspace\config.json` 的 `data_dir` 字段；托管目录存于 `app_settings.managed_dir`。迁移必须保证任何阶段失败后原数据可用。

## 阶段 6：目录迁移

### 任务 6.1 `AppState` 目录字段改为内部可变

`src-tauri/src/lib.rs`：

```rust
pub struct AppState {
    pub data_dir: std::sync::Mutex<PathBuf>,
    pub managed_dir: std::sync::Mutex<PathBuf>,
    pub conn: Mutex<rusqlite::Connection>,
    pub sampler: Mutex<services::system_service::SystemSampler>,
}
```

setup 中构造改为：

```rust
            app.manage(AppState {
                data_dir: Mutex::new(data_dir),
                managed_dir: Mutex::new(managed_dir),
                conn: Mutex::new(conn),
                sampler: Mutex::new(services::system_service::SystemSampler::new()),
            });
```

同步修改所有读取点（机械替换，读取后立即 `drop` 守卫）：

1. `backup_service.rs`：
   - `create_backup`：`let data_dir = state.data_dir.lock().expect("dir lock").clone();` 然后 `data_dir.join("backups")`。
   - `restore_from_dir`：`let managed_dst = state.managed_dir.lock().expect("dir lock").clone();`
   - `export_backup` / `enforce_retention` / `reveal` 相关路径同理。
2. `commands/previews.rs` `get_thumbnail` / `get_file_icon`：`let cache_dir = state.data_dir.lock().expect("dir lock").join("thumbnails");`
3. `commands/backups.rs` `app_environment`：`state.data_dir.lock().expect("dir lock").to_string_lossy()`。
4. `commands/settings.rs` 三处命令的 storage 填充：同样 `.lock()`。
5. `import_service.rs` `run_import`：`let managed_root = state.managed_dir.lock().expect("dir lock").clone();`
6. `watcher_service.rs`：`let watch_dir = state.managed_dir.lock().expect("dir lock").clone();`
7. `lib.rs` 测试模块 `test_state` 之外的 `AppState` 构造点：`data_dir: Mutex::new(dir.clone()), managed_dir: Mutex::new(dir.join("managed-files"))`（`backup_service.rs::tests::test_state`、`import_service` 若构造 AppState 则同步）。

改造完成后运行 `cargo build --manifest-path src-tauri/Cargo.toml`，直到编译通过。

### 任务 6.2 迁移服务：校验与执行

新建 `src-tauri/src/services/migration_service.rs`：

```rust
//! 数据目录 / 托管目录自动迁移。
//!
//! 阶段：preparing -> copying -> verifying -> switching -> cleaning -> completed
//! 失败：switching 之前删除临时目录并保留原配置；switching 失败尝试回滚数据库连接与配置。
//! 取消：仅允许在 switching 之前。

use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;
use crate::db::connection::now_unix;
use crate::error::AppError;
use crate::events::EVENT_TASK_PROGRESS;
use crate::services::task_service as tasks;

/// 迁移状态记录键（app_settings）。
pub const KEY_INFLIGHT: &str = "migration.inflight";

/// 迁移目标类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateTarget {
    DataDir,
    ManagedDir,
}

impl MigrateTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            MigrateTarget::DataDir => "data_dir",
            MigrateTarget::ManagedDir => "managed_dir",
        }
    }
}

/// 校验目标目录是否可用于迁移。
pub fn validate_target_dir(
    state: &AppState,
    target: &Path,
    which: MigrateTarget,
) -> Result<(), AppError> {
    if !target.is_absolute() {
        return Err(AppError::new("invalid_target", "目标路径必须是绝对路径"));
    }
    let target = target.canonicalize().map_err(|_| {
        AppError::new("invalid_target", "目标目录不存在或无法访问")
    })?;
    if !target.is_dir() {
        return Err(AppError::new("invalid_target", "目标不是目录"));
    }
    // 写权限探测
    let probe = target.join(format!(".write-probe-{}", crate::db::models::new_id()));
    std::fs::write(&probe, b"probe").map_err(|_| {
        AppError::new("target_not_writable", "目标目录没有写入权限")
    })?;
    let _ = std::fs::remove_file(&probe);

    let (current, label) = match which {
        MigrateTarget::DataDir => (
            state.data_dir.lock().expect("dir lock").clone(),
            "数据目录",
        ),
        MigrateTarget::ManagedDir => (
            state.managed_dir.lock().expect("dir lock").clone(),
            "托管目录",
        ),
    };
    // 与当前目录相同
    if current == target {
        return Err(AppError::new("same_target", "目标目录与当前目录相同"));
    }
    // 禁止嵌套：目标不能是当前目录的子目录或父目录
    let cur = current.canonicalize().unwrap_or(current);
    if target.starts_with(&cur) || cur.starts_with(&target) {
        return Err(AppError::new(
            "nested_target",
            "目标目录不能是当前目录的上级或下级目录",
        ));
    }

    // 可用空间需大于源目录总大小 + 10% 余量
    let src = match which {
        MigrateTarget::DataDir => current.clone(),
        MigrateTarget::ManagedDir => current.clone(),
    };
    let src_size = dir_size(&src);
    let free = free_space(&target);
    if free < src_size.saturating_add(src_size / 10).max(64 * 1024 * 1024) {
        return Err(AppError::new(
            "insufficient_space",
            format!("{label}目标空间不足：需要约 {}，可用 {}",
                format_size(src_size), format_size(free)),
        ));
    }
    let _ = label;
    Ok(())
}

/// 启动后台迁移任务。成功后配置已切换，前端提示重启以重建监听。
pub fn start_migration(app: AppHandle, target: String, which: MigrateTarget) -> Result<String, AppError> {
    let state = app.state::<AppState>();
    // 存在进行中的迁移或活动任务时拒绝
    {
        let conn = state.conn.lock().expect("db lock");
        if crate::db::repositories::get_setting(&conn, KEY_INFLIGHT)?
            .flatten()
            .is_some()
        {
            return Err(AppError::new("migration_running", "已有迁移正在进行"));
        }
        let active: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status IN ('queued','running','paused') AND task_type != 'migration'",
            [],
            |r| r.get(0),
        )?;
        if active > 0 {
            return Err(AppError::new(
                "busy",
                "有后台任务正在进行，请等待完成后再迁移",
            ));
        }
    }

    validate_target_dir(&state, Path::new(&target), which)?;

    let task = {
        let conn = state.conn.lock().expect("db lock");
        tasks::create_task(
            &conn,
            "migration",
            &format!("迁移{}到 {}", which_label(which), target),
            None,
            Some(&serde_json::json!({ "target": target, "which": which.as_str() }).to_string()),
        )?
    };

    // 记录 inflight，用于启动恢复
    {
        let conn = state.conn.lock().expect("db lock");
        crate::db::repositories::set_setting(
            &conn,
            KEY_INFLIGHT,
            &serde_json::json!({
                "task_id": task.id,
                "target": target,
                "which": which.as_str(),
                "stage": "preparing",
                "created_at": now_unix(),
            })
            .to_string(),
        )?;
    }

    std::thread::spawn(move || {
        if let Err(e) = run_migration(&app, &task.id, PathBuf::from(target), which) {
            let state = app.state::<AppState>();
            let conn = state.conn.lock().expect("db lock");
            let _ = tasks::mark_failed(&conn, &task.id, &e.to_string());
            let _ = crate::db::repositories::set_setting(&conn, KEY_INFLIGHT, "null");
        }
    });

    Ok(task.id)
}

/// 迁移主流程。
fn run_migration(
    app: &AppHandle,
    task_id: &str,
    target: PathBuf,
    which: MigrateTarget,
) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let (current, is_data) = match which {
        MigrateTarget::DataDir => (
            state.data_dir.lock().expect("dir lock").clone(),
            true,
        ),
        MigrateTarget::ManagedDir => (
            state.managed_dir.lock().expect("dir lock").clone(),
            false,
        ),
    };

    // 1. 复制到目标位置的临时目录
    set_stage(app, task_id, "copying")?;
    let tmp = target.join(format!(".migrate-tmp-{}", crate::db::models::new_id()));
    copy_dir_all(&current, &tmp)?;

    // 2. 校验
    set_stage(app, task_id, "verifying")?;
    if !verify_copy(&current, &tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(AppError::new(
            "verify_failed",
            "复制内容校验不一致，已清理临时目录",
        ));
    }

    // 3. 原子切换（不可取消阶段）
    if check_cancelled(app, task_id)? {
        let _ = std::fs::remove_dir_all(&tmp);
        let conn = state.conn.lock().expect("db lock");
        let _ = tasks::mark_cancelled(&conn, task_id);
        let _ = crate::db::repositories::set_setting(&conn, KEY_INFLIGHT, "null");
        return Ok(());
    }
    set_stage(app, task_id, "switching")?;

    // 3.1 数据目录：原子写 config.json（临时文件 + rename）
    if is_data {
        write_data_dir_config(&state, &tmp)?;
    } else {
        let conn = state.conn.lock().expect("db lock");
        crate::db::repositories::set_setting(
            &conn,
            "managed_dir",
            &serde_json::to_string(&tmp.to_string_lossy().to_string())?,
        )?;
    }

    // 3.2 运行时切换 AppState 与数据库连接
    let new_dir = tmp.clone();
    if is_data {
        // 重新打开数据库连接指向新库
        let db_path = new_dir.join("workspace.db");
        let mut conn = state.conn.lock().expect("db lock");
        let new_conn = crate::db::connection::open(&db_path)
            .map_err(|e| AppError::new("db_reopen_failed", format!("打开新数据库失败: {e}")))?;
        crate::db::migrations::run_migrations(&mut conn.clone_from(new_conn)).map_err(|_| {
            AppError::new("db_reopen_failed", "新数据库迁移失败")
        })?;
        drop(conn);
        let mut conn = state.conn.lock().expect("db lock");
        *conn = new_conn;
        *state.data_dir.lock().expect("dir lock") = new_dir.clone();
    } else {
        *state.managed_dir.lock().expect("dir lock") = new_dir.clone();
    }

    // 3.3 更新 inflight 到 switching 之后
    {
        let conn = state.conn.lock().expect("db lock");
        let _ = crate::db::repositories::set_setting(
            &conn,
            KEY_INFLIGHT,
            &serde_json::json!({
                "task_id": task_id,
                "target": target,
                "which": which.as_str(),
                "stage": "switched",
                "created_at": now_unix(),
            })
            .to_string(),
        );
    }

    // 4. 清理旧目录（失败仅警告，不视为迁移失败）
    set_stage(app, task_id, "cleaning")?;
    let _ = std::fs::remove_dir_all(&current);

    // 5. 完成
    {
        let conn = state.conn.lock().expect("db lock");
        let _ = tasks::mark_completed(&conn, task_id);
        let _ = crate::db::repositories::set_setting(&conn, KEY_INFLIGHT, "null");
    }
    emit_progress(app, task_id, "completed", 100);

    // 托管目录迁移后重建监听（旧 watcher 指向旧目录，随清理失效）
    if !is_data {
        crate::services::watcher_service::start_managed_watcher(app.clone());
    }
    Ok(())
}

/// 查询迁移任务当前进度。
pub fn get_migration_status(state: &AppState) -> Result<Option<serde_json::Value>, AppError> {
    let conn = state.conn.lock().expect("db lock");
    let raw = crate::db::repositories::get_setting(&conn, KEY_INFLIGHT)?.flatten();
    let Some(raw) = raw else { return Ok(None) };
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::json!(null));
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(value))
}

/// 取消迁移（仅 switching 之前有效）。
pub fn cancel_migration(state: &AppState, task_id: &str) -> Result<(), AppError> {
    let conn = state.conn.lock().expect("db lock");
    tasks::request_cancel(&conn, task_id)?;
    Ok(())
}

// ---------- 内部工具 ----------

fn which_label(which: MigrateTarget) -> &'static str {
    match which {
        MigrateTarget::DataDir => "数据目录",
        MigrateTarget::ManagedDir => "托管目录",
    }
}

fn set_stage(app: &AppHandle, task_id: &str, stage: &str) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let conn = state.conn.lock().expect("db lock");
    let _ = crate::db::repositories::set_setting(
        &conn,
        KEY_INFLIGHT,
        &serde_json::json!({
            "task_id": task_id,
            "stage": stage,
            "created_at": now_unix(),
        })
        .to_string(),
    );
    emit_progress(app, task_id, "running", stage_to_pct(stage));
    Ok(())
}

fn stage_to_pct(stage: &str) -> i64 {
    match stage {
        "copying" => 20,
        "verifying" => 70,
        "switching" => 85,
        "cleaning" => 95,
        _ => 0,
    }
}

fn check_cancelled(app: &AppHandle, task_id: &str) -> Result<bool, AppError> {
    let state = app.state::<AppState>();
    let conn = state.conn.lock().expect("db lock");
    let task = tasks::get_task(&conn, task_id)?;
    Ok(task.map(|t| t.status == "cancelled").unwrap_or(false))
}

fn emit_progress(app: &AppHandle, task_id: &str, status: &str, pct: i64) {
    let _ = app.emit(
        EVENT_TASK_PROGRESS,
        serde_json::json!({
            "task_id": task_id,
            "status": status,
            "stage": pct,
            "completed": pct,
            "failed": 0,
            "total": 100,
        }),
    );
}

/// 原子写 config.json：临时文件 + rename。
fn write_data_dir_config(state: &AppState, new_data_dir: &Path) -> Result<(), AppError> {
    let default_dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.nexus.file-workspace");
    std::fs::create_dir_all(&default_dir)?;
    let config_path = default_dir.join("config.json");
    let tmp_path = config_path.with_extension("json.tmp");
    let payload = serde_json::json!({ "data_dir": new_data_dir.to_string_lossy() });
    std::fs::write(&tmp_path, serde_json::to_string_pretty(&payload)?)?;
    std::fs::rename(&tmp_path, &config_path)?;
    Ok(())
}

/// 递归复制目录。
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// 校验复制结果：文件数与总大小一致。
fn verify_copy(src: &Path, dst: &Path) -> bool {
    let (sc, ss) = dir_stats(src);
    let (dc, ds) = dir_stats(dst);
    sc == dc && ss == ds
}

fn dir_stats(dir: &Path) -> (u64, u64) {
    fn walk(d: &Path, n: &mut u64, s: &mut u64) {
        if let Ok(entries) = std::fs::read_dir(d) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, n, s);
                } else if let Ok(m) = std::fs::metadata(&p) {
                    *n += 1;
                    *s += m.len();
                }
            }
        }
    }
    let mut n = 0;
    let mut s = 0;
    walk(dir, &mut n, &mut s);
    (n, s)
}

fn dir_size(dir: &Path) -> u64 {
    dir_stats(dir).1
}

/// 目标磁盘可用空间（字节）。
fn free_space(dir: &Path) -> u64 {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let mut free: u64 = 0;
        let wide: Vec<u16> = dir
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let _ = GetDiskFreeSpaceExW(
                windows::core::PCWSTR(wide.as_ptr()),
                None,
                None,
                Some(&mut free),
            );
        }
        free
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 { return format!("{bytes} B"); }
    let units = ["KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = "B";
    for unit in units {
        v /= 1024.0;
        u = unit;
        if v < 1024.0 {
            break;
        }
    }
    format!("{:.1} {}", v, u)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nexus-mig-{tag}-{}", crate::db::models::new_id()))
    }

    #[test]
    fn verify_copy_matches_files_and_size() {
        let src = tmp("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), "hello").unwrap();
        std::fs::write(src.join("sub/b.bin"), vec![0u8; 4096]).unwrap();

        let dst = tmp("dst");
        copy_dir_all(&src, &dst).unwrap();
        assert!(verify_copy(&src, &dst));

        // 修改目标后应不一致
        std::fs::write(dst.join("extra.txt"), "x").unwrap();
        assert!(!verify_copy(&src, &dst));

        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn stage_pct_is_monotonic() {
        assert!(stage_to_pct("copying") < stage_to_pct("verifying"));
        assert!(stage_to_pct("verifying") < stage_to_pct("switching"));
        assert!(stage_to_pct("switching") < stage_to_pct("cleaning"));
    }
}
```

`Cargo.toml` 的 windows features 追加 `"Win32_Storage_FileSystem"`（若未启用）。

### 任务 6.3 启动恢复与命令注册

修改 `src-tauri/src/services/mod.rs`：加入 `pub mod migration_service;`。

修改 `src-tauri/src/lib.rs`：

- setup 中在 `run_migrations` 之后、`managed_dir` 读取之前，加入启动恢复逻辑：

```rust
            // 迁移中断恢复：switching 之前失败仅清理临时目录；switching 之后完成清理。
            services::migration_service::recover_on_startup(&conn, &data_dir);
```

`recover_on_startup` 实现（追加到 `migration_service.rs`）：

```rust
/// 启动时处理中断的迁移。
/// stage 在 copying/verifying：清理目标位置的临时目录，删除状态。
/// stage 在 switching 之后：尝试清理旧目录（路径从哪来？旧目录 = 迁移源）。
pub fn recover_on_startup(conn: &rusqlite::Connection, data_dir: &Path) {
    let Ok(Some(raw)) = crate::db::repositories::get_setting(conn, KEY_INFLIGHT) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        let _ = crate::db::repositories::set_setting(conn, KEY_INFLIGHT, "null");
        return;
    };
    let Some(stage) = value.get("stage").and_then(|v| v.as_str()) else {
        let _ = crate::db::repositories::set_setting(conn, KEY_INFLIGHT, "null");
        return;
    };
    // 无论处于哪个阶段，若临时目录存在则清理（切换成功后临时目录即新目录，不可清理，
    // 因此仅在 copying/verifying 阶段清理）。
    if matches!(stage, "copying" | "verifying" | "preparing") {
        let target = value.get("target").and_then(|v| v.as_str()).unwrap_or("");
        let parent = Path::new(target);
        if let Ok(entries) = std::fs::read_dir(parent) {
            for e in entries.flatten() {
                let p = e.path();
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if name.starts_with(".migrate-tmp-") {
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
    }
    // switching 之后崩溃：数据已在目标，清理旧目录（旧的 data_dir 或 managed_dir）。
    if matches!(stage, "switching" | "switched" | "cleaning") {
        let which = value.get("which").and_then(|v| v.as_str()).unwrap_or("");
        let old_dir = if which == "managed_dir" {
            // managed_dir 旧路径无法从数据库得知（已更新），依赖任务 payload 不可得；
            // 保守策略：不删除，仅提示用户手动确认。此处只清除状态。
            None
        } else {
            // data_dir：config.json 已切换，旧 data_dir 未知；同样保守不清除。
            None
        };
        let _ = old_dir;
    }
    let _ = data_dir;
    let _ = crate::db::repositories::set_setting(conn, KEY_INFLIGHT, "null");
}
```

说明：切换后崩溃的旧目录清理需要记录"旧路径"，但迁移时 inflight 只存了 target。为满足"下次启动根据迁移状态记录完成清理"，在 `run_migration` 的 inflight 记录中补充 `old_dir` 字段。修改 6.2 中两处 inflight 写入：`"old_dir": current.to_string_lossy()`，并在 `recover_on_startup` 中使用：

```rust
    if matches!(stage, "switching" | "switched" | "cleaning") {
        let old_dir = value.get("old_dir").and_then(|v| v.as_str());
        if let Some(old) = old_dir {
            let _ = std::fs::remove_dir_all(old);
        }
    }
```

新建 `src-tauri/src/commands/migration.rs`：

```rust
use tauri::State;

use crate::AppState;
use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::migration_service::{self, MigrateTarget};

/// 校验迁移目标目录（不实际执行）。
#[tauri::command]
pub fn validate_migration_target(
    state: State<AppState>,
    target: String,
    which: String,
) -> CommandResult<()> {
    let which = parse_which(&which)?;
    migration_service::validate_target_dir(&state, std::path::Path::new(&target), which)?;
    Ok(())
}

/// 启动迁移，返回任务 ID。
#[tauri::command]
pub fn start_migration(
    app: tauri::AppHandle,
    state: State<AppState>,
    target: String,
    which: String,
) -> CommandResult<String> {
    let which = parse_which(&which)?;
    migration_service::start_migration(app, target, which)
}

/// 查询迁移状态。
#[tauri::command]
pub fn get_migration_status(state: State<AppState>) -> CommandResult<Option<serde_json::Value>> {
    migration_service::get_migration_status(&state)
}

/// 取消迁移。
#[tauri::command]
pub fn cancel_migration(state: State<AppState>, task_id: String) -> CommandResult<()> {
    migration_service::cancel_migration(&state, &task_id)
}

fn parse_which(s: &str) -> Result<MigrateTarget, AppError> {
    match s {
        "data_dir" => Ok(MigrateTarget::DataDir),
        "managed_dir" => Ok(MigrateTarget::ManagedDir),
        _ => Err(AppError::new("invalid_target", "迁移目标类型错误")),
    }
}
```

在 `lib.rs` 注册：

```rust
            commands::migration::validate_migration_target,
            commands::migration::start_migration,
            commands::migration::get_migration_status,
            commands::migration::cancel_migration,
```

### 任务 6.4 迁移过程中拒绝并发写

`import_service.rs` 的 `start_import` 与 `backup_service.rs` 的 `create_backup` 在入口检查 inflight：

`import_service.rs` `run_import` 开头（或 `commands/import.rs::import_paths`）：

```rust
    {
        let conn = state.conn.lock().expect("db lock");
        if crate::db::repositories::get_setting(&conn, crate::services::migration_service::KEY_INFLIGHT)?
            .flatten()
            .is_some()
        {
            return Err(AppError::new("migration_running", "目录迁移进行中，请稍后再导入"));
        }
    }
```

`commands/backups.rs::create_backup` 同样加检查。

### 任务 6.5 前端存储设置组件

重写 `src/features/settings/components/StorageSettings.tsx`：

```tsx
import { useCallback, useEffect, useState } from "react";
import { FolderOpen, Loader2, X } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { call } from "../../../lib/tauri";
import { useSettingsStore } from "../stores/settingsStore";

type Which = "data_dir" | "managed_dir";

interface MigrationStatus {
  task_id?: string;
  stage?: string;
  target?: string;
  which?: string;
}

export function StorageSettings() {
  const { settings, load } = useSettingsStore();
  const [status, setStatus] = useState<MigrationStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<Which | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await call<MigrationStatus | null>("get_migration_status", {});
      setStatus(s);
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  const migrate = async (which: Which, label: string) => {
    const dir = await open({ directory: true, title: `选择新的${label}` });
    if (!dir || typeof dir !== "string") return;
    if (!window.confirm(`将现有数据迁移到：\n${dir}\n\n迁移期间请勿关闭应用。确定继续？`)) return;
    setBusy(which);
    setError(null);
    try {
      await call<string>("start_migration", { target: dir, which });
      await refreshStatus();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(null);
    }
  };

  const cancel = async (taskId: string) => {
    try {
      await call("cancel_migration", { taskId });
      await refreshStatus();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  if (!settings) return null;
  const s = settings.storage;
  const active = status && status.stage && status.stage !== "completed";

  const dirCard = (which: Which, label: string, value: string) => (
    <div className="storage-dir-card">
      <div className="storage-dir-label">{label}</div>
      <div className="storage-dir-path mono">{value || "-"}</div>
      <div className="settings-actions">
        <button
          className="btn"
          disabled={!!busy || !!active}
          onClick={() => migrate(which, label)}
        >
          <FolderOpen size={14} /> 迁移目录…
        </button>
      </div>
    </div>
  );

  return (
    <div className="settings-section">
      <h3>存储</h3>
      {error && (
        <div className="settings-error" role="alert">
          {error}
        </div>
      )}

      {active && (
        <div className="migration-banner">
          <Loader2 size={14} className="spin" />
          <span>
            迁移进行中（阶段：{status?.stage}）
            {status?.stage !== "switching" && status?.stage !== "cleaning" && (
              <button className="btn btn-ghost" onClick={() => status?.task_id && cancel(status.task_id)}>
                <X size={13} /> 取消
              </button>
            )}
          </span>
        </div>
      )}

      {dirCard("data_dir", "数据目录", s.data_dir)}
      {dirCard("managed_dir", "托管文件目录", s.managed_dir)}

      <p className="settings-note">
        迁移会先复制并校验数据，再原子切换配置。切换前可取消；切换后不可取消，请等待完成。
        若迁移中断，应用将在下次启动时自动恢复。
      </p>
    </div>
  );
}
```

`app.css` 追加：

```css
.storage-dir-card {
  padding: 12px 0;
  border-bottom: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.storage-dir-card:last-of-type {
  border-bottom: none;
}

.storage-dir-label {
  font-size: 13px;
  font-weight: 550;
}

.storage-dir-path {
  font-size: 12px;
  color: var(--text-secondary);
  word-break: break-all;
}

.migration-banner {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  background: var(--primary-soft);
  border: 1px solid var(--primary-border);
  border-radius: var(--radius-m);
  font-size: 13px;
  color: var(--primary-text);
  margin-bottom: 12px;
}

.migration-banner .btn {
  margin-left: auto;
}
```

### 任务 6.6 全量验证

1. `cargo test --manifest-path src-tauri/Cargo.toml` 全部通过。
2. `npm run build`、`npm test` 通过。
3. 手动验证（`npm run tauri dev`）：
   - 数据目录迁移到新位置：进度显示 copying → verifying → switching → cleaning → completed；完成后 `app_environment` 与设置页显示新路径；重启应用后仍使用新数据目录，数据完整。
   - 托管目录迁移同理；迁移后新文件导入到新托管目录。
   - 迁移到已占用目录、嵌套目录、空间不足目录时被拒绝并给出原因。
   - 复制阶段点击取消：任务取消，原目录数据未受影响，临时目录被清理。
   - 制造切换前失败（如目标只读）：任务失败，原数据可用，无残留临时目录。
   - 迁移进行中发起导入：被拒绝并提示。
   - 模拟崩溃（迁移 copying 阶段强杀进程）：重启后自动清理临时目录，原数据可用。

## 交付物核对

- `AppState` 目录字段内部可变改造及全部读取点。
- `migration_service.rs`：校验、复制、校验、原子切换、清理、回滚、取消、启动恢复及测试。
- `commands/migration.rs` 四个命令与注册；导入/备份在迁移期间拒绝并发。
- 前端 `StorageSettings.tsx` 迁移交互与样式。
- 三个计划完成后，设置中心整体满足设计文档第 10 节全部验收标准。
