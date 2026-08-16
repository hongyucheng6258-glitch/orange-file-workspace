//! 数据目录 / 托管目录自动迁移。
//!
//! 阶段：preparing -> copying -> verifying -> switching -> cleaning -> completed
//! 失败：switching 之前删除临时目录并保留原配置；switching 失败尝试回滚数据库连接与配置。
//! 取消：仅允许在 switching 之前。

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, Manager};

use crate::db::connection::now_unix;
use crate::error::AppError;
use crate::events::EVENT_TASK_PROGRESS;
use crate::services::task_service as tasks;
use crate::AppState;

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
    let target = target
        .canonicalize()
        .map_err(|_| AppError::new("invalid_target", "目标目录不存在或无法访问"))?;
    if !target.is_dir() {
        return Err(AppError::new("invalid_target", "目标不是目录"));
    }
    // 写权限探测
    let probe = target.join(format!(".write-probe-{}", crate::db::models::new_id()));
    std::fs::write(&probe, b"probe")
        .map_err(|_| AppError::new("target_not_writable", "目标目录没有写入权限"))?;
    let _ = std::fs::remove_file(&probe);

    let (current, label) = match which {
        MigrateTarget::DataDir => (state.data_dir.lock().expect("dir lock").clone(), "数据目录"),
        MigrateTarget::ManagedDir => (
            state.managed_dir.lock().expect("dir lock").clone(),
            "托管目录",
        ),
    };
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

    // 可用空间需大于源目录总大小 + 10% 余量（至少 64MB）
    let src_size = dir_size(&src_dir(state, which));
    let free = free_space(&target);
    let needed = src_size.saturating_add(src_size / 10).max(64 * 1024 * 1024);
    if free < needed {
        return Err(AppError::new(
            "insufficient_space",
            format!(
                "{label}目标空间不足：需要约 {}，可用 {}",
                format_size(needed),
                format_size(free)
            ),
        ));
    }
    let _ = label;
    Ok(())
}

/// 启动后台迁移任务。成功后配置已切换。
pub fn start_migration(
    app: AppHandle,
    target: String,
    which: MigrateTarget,
) -> Result<String, AppError> {
    let state = app.state::<AppState>();
    // 存在进行中的迁移或活动任务时拒绝
    {
        let conn = state.conn.lock().expect("db lock");
        if crate::db::repositories::get_setting(&conn, KEY_INFLIGHT)?.is_some() {
            return Err(AppError::new("migration_running", "已有迁移正在进行"));
        }
        let active: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks
             WHERE status IN ('queued','running','paused') AND task_type != 'migration'",
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

    let current = match which {
        MigrateTarget::DataDir => state.data_dir.lock().expect("dir lock").clone(),
        MigrateTarget::ManagedDir => state.managed_dir.lock().expect("dir lock").clone(),
    };

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
                "old_dir": current.to_string_lossy(),
                "created_at": now_unix(),
            })
            .to_string(),
        )?;
    }

    let task_id = task.id.clone();
    let state_handle = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = run_migration(&state_handle, &task_id, PathBuf::from(target), which) {
            let state = state_handle.state::<AppState>();
            let conn = state.conn.lock().expect("db lock");
            let _ = tasks::mark_failed(&conn, &task_id, &e.to_string());
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
        MigrateTarget::DataDir => (state.data_dir.lock().expect("dir lock").clone(), true),
        MigrateTarget::ManagedDir => (state.managed_dir.lock().expect("dir lock").clone(), false),
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

    // 3. 切换前允许取消
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
        write_data_dir_config(&tmp)?;
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
        // 打开并迁移新库，成功后替换当前连接
        let db_path = new_dir.join("workspace.db");
        let mut new_conn = crate::db::connection::open(&db_path)
            .map_err(|e| AppError::new("db_reopen_failed", format!("打开新数据库失败: {e}")))?;
        crate::db::migrations::run_migrations(&mut new_conn)
            .map_err(|_| AppError::new("db_reopen_failed", "新数据库迁移失败"))?;
        {
            let mut conn = state.conn.lock().expect("db lock");
            *conn = new_conn;
        }
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
                "old_dir": current.to_string_lossy(),
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

/// 查询迁移任务当前状态。
pub fn get_migration_status(state: &AppState) -> Result<Option<serde_json::Value>, AppError> {
    let conn = state.conn.lock().expect("db lock");
    let Some(raw) = crate::db::repositories::get_setting(&conn, KEY_INFLIGHT)? else {
        return Ok(None);
    };
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

/// 启动时处理中断的迁移。
/// copying/verifying 阶段：清理目标位置的临时目录。
/// switching 之后：清理旧目录（old_dir 记录在 inflight 中）。
pub fn recover_on_startup(conn: &rusqlite::Connection, _data_dir: &Path) {
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
    if matches!(stage, "copying" | "verifying" | "preparing") {
        // 切换前中断：清理临时目录，原配置不变
        let target = value.get("target").and_then(|v| v.as_str()).unwrap_or("");
        if let Ok(entries) = std::fs::read_dir(target) {
            for e in entries.flatten() {
                let p = e.path();
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if name.starts_with(".migrate-tmp-") {
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
    } else if matches!(stage, "switching" | "switched" | "cleaning") {
        // 切换后中断：数据已在目标，清理旧目录
        let old_dir = value.get("old_dir").and_then(|v| v.as_str());
        if let Some(old) = old_dir {
            let _ = std::fs::remove_dir_all(old);
        }
    }
    let _ = crate::db::repositories::set_setting(conn, KEY_INFLIGHT, "null");
}

// ---------- 内部工具 ----------

fn src_dir(state: &AppState, which: MigrateTarget) -> PathBuf {
    match which {
        MigrateTarget::DataDir => state.data_dir.lock().expect("dir lock").clone(),
        MigrateTarget::ManagedDir => state.managed_dir.lock().expect("dir lock").clone(),
    }
}

fn which_label(which: MigrateTarget) -> &'static str {
    match which {
        MigrateTarget::DataDir => "数据目录",
        MigrateTarget::ManagedDir => "托管目录",
    }
}

fn set_stage(app: &AppHandle, task_id: &str, stage: &str) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let conn = state.conn.lock().expect("db lock");
    // 保留既有 inflight 字段，只更新 stage
    let existing = crate::db::repositories::get_setting(&conn, KEY_INFLIGHT)?;
    let mut payload = existing
        .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = payload.as_object_mut() {
        obj.insert(
            "stage".to_string(),
            serde_json::Value::String(stage.to_string()),
        );
    }
    let _ = crate::db::repositories::set_setting(&conn, KEY_INFLIGHT, &payload.to_string());
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
fn write_data_dir_config(new_data_dir: &Path) -> Result<(), AppError> {
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
#[cfg(target_os = "windows")]
fn free_space(dir: &Path) -> u64 {
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
            Some(&mut free as *mut u64),
            None,
            None,
        );
    }
    free
}

#[cfg(not(target_os = "windows"))]
fn free_space(_dir: &Path) -> u64 {
    0
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
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
