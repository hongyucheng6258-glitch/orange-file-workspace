use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::backup::Backup;
use rusqlite::{params, Connection, OptionalExtension};

use crate::db::connection::now_unix;
use crate::db::models::new_id;
use crate::error::AppError;
use crate::services::settings_service;
use crate::AppState;

/// 自动备份最后执行时间键（app_settings）。
pub const KEY_LAST_RUN_TS: &str = "backup.last_run_ts";

/// 创建备份。include_files=true 时同时复制托管文件。
/// source: manual（手动）、auto（自动）、protect（恢复前保护备份）。
/// 返回 (备份目录, 记录)。
pub fn create_backup(
    state: &AppState,
    include_files: bool,
    source: &str,
) -> Result<(PathBuf, crate::db::models::BackupRecord), AppError> {
    let backups_root = state.data_dir.lock().expect("dir lock").join("backups");
    std::fs::create_dir_all(&backups_root)?;

    let ts = chrono_like_timestamp();
    let uniq = &crate::db::models::new_id()[..8];
    let backup_dir = backups_root.join(format!("backup-{ts}-{uniq}"));
    std::fs::create_dir_all(&backup_dir)?;

    // 1. 数据库一致性快照（SQLite Online Backup API）
    let db_path = backup_dir.join("workspace.db");
    {
        let conn = state.conn.lock().expect("db lock");
        conn.backup("main", &db_path, None)?;
    }

    // 2. manifest
    let manifest = serde_json::json!({
        "app": env!("CARGO_PKG_NAME"),
        "app_version": env!("CARGO_PKG_VERSION"),
        "created_at": now_unix(),
        "database_version": crate::db::migrations::MIGRATIONS
            .last()
            .map(|m| m.version)
            .unwrap_or(0),
        "include_files": include_files,
    });
    std::fs::write(
        backup_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    // 3. 可选：托管文件
    if include_files {
        let managed_dir = state.managed_dir.lock().expect("dir lock");
        copy_dir_all(&managed_dir, &backup_dir.join("managed-files"))?;
    }

    // 4. 记录
    let conn = state.conn.lock().expect("db lock");
    let resource_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
        .unwrap_or(0);
    let file_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM resource_locations WHERE source_type = 'managed'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let record = crate::db::models::BackupRecord {
        id: new_id(),
        path: backup_dir.to_string_lossy().to_string(),
        backup_type: if include_files {
            "full".into()
        } else {
            "metadata".into()
        },
        database_version: crate::db::migrations::MIGRATIONS
            .last()
            .map(|m| m.version)
            .unwrap_or(0),
        resource_count: Some(resource_count),
        file_count: Some(file_count),
        created_at: now_unix(),
        status: "completed".to_string(),
        error_message: None,
        source: source.to_string(),
    };
    conn.execute(
        "INSERT INTO backup_records (
            id, path, backup_type, database_version, resource_count,
            file_count, created_at, status, error_message, source
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'completed', NULL, ?8)",
        params![
            record.id,
            record.path,
            record.backup_type,
            record.database_version,
            record.resource_count,
            record.file_count,
            record.created_at,
            record.source
        ],
    )?;

    Ok((backup_dir, record))
}

/// 校验备份目录完整性，返回 manifest 关键字段。
/// 检查：manifest 存在且属于本应用、数据库文件存在、数据库版本不高于当前。
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
        .query_row(
            "SELECT path FROM backup_records WHERE id = ?1",
            [backup_id],
            |r| r.get(0),
        )
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
        .query_row(
            "SELECT path FROM backup_records WHERE id = ?1",
            [backup_id],
            |r| r.get(0),
        )
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

/// 从备份目录恢复数据库（覆盖当前连接内容）。
///
/// 安全流程：校验 → 创建保护备份 → 恢复数据库（失败回滚）→ 恢复托管文件（失败回滚）。
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

    // 3. 恢复托管文件（若备份含托管文件）；任何失败都会同时回滚数据库
    let managed_src = backup_dir.join("managed-files");
    if managed_src.exists() {
        let protect_managed = protect_dir.join("managed-files-current");
        let managed_dst = state.managed_dir.lock().expect("dir lock").clone();
        restore_managed_files(
            state,
            &managed_src,
            &managed_dst,
            &protect_managed,
            &protect_dir,
        )?;
    }

    Ok(())
}

/// 恢复托管文件，任何失败都回滚文件与数据库，保证库与磁盘一致。
fn restore_managed_files(
    state: &AppState,
    managed_src: &Path,
    managed_dst: &Path,
    protect_managed: &Path,
    protect_dir: &Path,
) -> Result<(), AppError> {
    // 先把当前托管目录整体移动到保护位置（移动不占双份空间）
    if managed_dst.exists() {
        if let Err(e) = std::fs::rename(managed_dst, protect_managed) {
            let _ = restore_db_snapshot(state, protect_dir);
            return Err(AppError::new(
                "restore_failed",
                format!("托管目录移动失败，数据库已回滚: {e}"),
            ));
        }
    }
    if let Err(e) = copy_dir_all(managed_src, managed_dst) {
        // 回滚：删除不完整的恢复目录，把原目录移回，并回滚数据库
        let _ = std::fs::remove_dir_all(managed_dst);
        if protect_managed.exists() {
            let _ = std::fs::rename(protect_managed, managed_dst);
        }
        let _ = restore_db_snapshot(state, protect_dir);
        return Err(AppError::new(
            "restore_failed",
            format!("托管文件恢复失败，数据库已回滚: {e}"),
        ));
    }
    // 校验恢复结果：文件数与总字节数一致
    let (src_count, src_bytes) = count_files_and_bytes(managed_src);
    let (dst_count, dst_bytes) = count_files_and_bytes(managed_dst);
    if src_count != dst_count || src_bytes != dst_bytes {
        let _ = std::fs::remove_dir_all(managed_dst);
        if protect_managed.exists() {
            let _ = std::fs::rename(protect_managed, managed_dst);
        }
        let _ = restore_db_snapshot(state, protect_dir);
        return Err(AppError::new(
            "restore_failed",
            "托管文件恢复校验不一致，数据库已回滚",
        ));
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

/// 列出备份记录。
pub fn list_backups(state: &AppState) -> Result<Vec<crate::db::models::BackupRecord>, AppError> {
    let conn = state.conn.lock().expect("db lock");
    let mut stmt =
        conn.prepare("SELECT * FROM backup_records ORDER BY created_at DESC LIMIT 50")?;
    let rows = stmt.query_map([], |row| {
        Ok(crate::db::models::BackupRecord {
            id: row.get("id")?,
            path: row.get("path")?,
            backup_type: row.get("backup_type")?,
            database_version: row.get("database_version")?,
            resource_count: row.get("resource_count")?,
            file_count: row.get("file_count")?,
            created_at: row.get("created_at")?,
            status: row.get("status")?,
            error_message: row.get("error_message")?,
            source: row.get("source")?,
        })
    })?;
    let mut records = Vec::new();
    for r in rows {
        records.push(r?);
    }
    Ok(records)
}

/// 复制目录（递归）。不跟随符号链接与目录联接（防循环与越界复制）。
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), AppError> {
    if !src.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        // 跳过符号链接与目录联接：避免循环递归与把托管目录之外的数据纳入备份
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// 统计目录内文件数与总字节数（含子目录）。不跟随符号链接与目录联接。
fn count_files_and_bytes(dir: &Path) -> (u64, u64) {
    fn walk(d: &Path, n: &mut u64, b: &mut u64) {
        if let Ok(entries) = std::fs::read_dir(d) {
            for e in entries.flatten() {
                // 用 file_type（symlink 元数据）判断：符号链接不进入统计（与复制逻辑一致）
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    walk(&e.path(), n, b);
                } else {
                    *n += 1;
                    *b += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
        }
    }
    let mut n = 0;
    let mut b = 0;
    walk(dir, &mut n, &mut b);
    (n, b)
}

/// 备份目录名用的时间戳（Unix 秒）。
fn chrono_like_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{now}")
}

/// 本地时间（时, 分）。
#[cfg(target_os = "windows")]
fn local_hhmm() -> (u8, u8) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    unsafe {
        let st = GetLocalTime();
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

/// 解析 "HH:MM"，非法返回 None。
fn parse_hhmm(s: &str) -> Option<(u8, u8)> {
    let (h, m) = s.split_once(':')?;
    let h: u8 = h.parse().ok()?;
    let m: u8 = m.parse().ok()?;
    if h < 24 && m < 60 {
        Some((h, m))
    } else {
        None
    }
}

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
    use tauri::Manager;
    let state = app.state::<AppState>();
    let settings = {
        let conn = state.conn.lock().expect("db lock");
        settings_service::load_settings(&conn)?
    };
    if !settings.backup.enabled {
        return Ok(());
    }

    let (now_h, now_m) = local_hhmm();
    let parsed = parse_hhmm(&settings.backup.run_time)
        .ok_or_else(|| AppError::new("invalid_setting", "自动备份时间格式错误"))?;
    if (now_h, now_m) < parsed {
        return Ok(()); // 当天时间点未到
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
        crate::db::repositories::set_setting(
            &conn,
            KEY_LAST_RUN_TS,
            &serde_json::to_string(&now)?,
        )?;
    }
    enforce_retention(&state, settings.backup.retention_count)?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> (AppState, PathBuf) {
        let dir = std::env::temp_dir().join(format!("nexus-bk-{}", crate::db::models::new_id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let conn = Connection::open(dir.join("workspace.db")).expect("open");
        let mut conn = conn;
        crate::db::migrations::run_migrations(&mut conn).expect("migrations");
        (
            AppState {
                data_dir: std::sync::Mutex::new(dir.clone()),
                managed_dir: std::sync::Mutex::new(dir.join("managed-files")),
                conn: std::sync::Arc::new(std::sync::Mutex::new(conn)),
                sampler: std::sync::Mutex::new(
                    crate::services::system_service::SystemSampler::new(),
                ),
                search: crate::SearchRuntime {
                    active_queries: std::sync::Mutex::new(std::collections::HashMap::new()),
                    next_query_id: std::sync::atomic::AtomicU64::new(1),
                    scan_paused: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    scan_trigger: std::sync::atomic::AtomicU64::new(0),
                },
                terminal: crate::services::terminal_service::TerminalRuntime::default(),
                runtime: std::sync::Arc::new(
                    crate::services::project_runtime::RuntimeManager::new(
                        std::sync::Arc::new(crate::services::process_api::Win32ProcessApiImpl),
                        std::sync::Arc::new(crate::services::project_runtime::NullRunEventSink),
                        std::sync::Arc::new(
                            crate::services::run_history::InMemoryRunHistoryStore::new(),
                        ),
                    ),
                ),
                preview: std::sync::Arc::new(
                    crate::services::web_preview_service::PreviewService::new(
                        std::sync::Arc::new(crate::services::project_runtime::RuntimeManager::new(
                            std::sync::Arc::new(crate::services::process_api::Win32ProcessApiImpl),
                            std::sync::Arc::new(crate::services::project_runtime::NullRunEventSink),
                            std::sync::Arc::new(
                                crate::services::run_history::InMemoryRunHistoryStore::new(),
                            ),
                        )),
                        std::sync::Arc::new(
                            crate::services::web_preview_service::UnsupportedPortProbe,
                        ),
                    ),
                ),
                app_usage: std::sync::Arc::new(
                    crate::services::app_usage_service::AppUsageTracker::new(),
                ),
            },
            dir,
        )
    }

    /// 统计 backups 目录下的备份子目录数（用于验证保护备份落盘）。
    fn backup_dir_count(dir: &Path) -> usize {
        std::fs::read_dir(dir.join("backups"))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn copy_dir_all_does_not_follow_symlink_loops() {
        let src = std::env::temp_dir().join(format!("nexus-cpsrc-{}", crate::db::models::new_id()));
        let dst = std::env::temp_dir().join(format!("nexus-cpdst-{}", crate::db::models::new_id()));
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
        std::fs::create_dir_all(src.join("real")).expect("mkdir");
        std::fs::write(src.join("real/a.txt"), "x").expect("write");

        // 尝试创建指向 src 自身的目录联接；无权限时跳过链接部分，仅验证正常复制
        #[cfg(windows)]
        {
            let link = src.join("loop");
            let _ = std::os::windows::fs::symlink_dir(&src, &link);
        }

        copy_dir_all(&src, &dst).expect("copy");

        // 符号链接目录不应被复制进目标（无论是否创建成功都验证）
        assert!(!dst.join("loop").exists(), "symlink loop must be skipped");
        assert!(dst.join("real/a.txt").exists(), "real file copied");

        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn create_backup_produces_consistent_snapshot() {
        let (state, dir) = test_state();
        {
            let conn = state.conn.lock().expect("lock");
            conn.execute(
                "INSERT INTO resources (id, kind, name, created_at, updated_at)
                 VALUES ('r1', 'file', 'a.txt', 1, 1)",
                [],
            )
            .expect("insert");
        }

        let (backup_dir, record) = create_backup(&state, false, "manual").expect("backup");
        assert!(backup_dir.join("workspace.db").exists());
        assert!(backup_dir.join("manifest.json").exists());
        assert_eq!(record.backup_type, "metadata");
        assert_eq!(record.source, "manual");

        let snap = Connection::open(backup_dir.join("workspace.db")).expect("open snap");
        let count: i64 = snap
            .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_replaces_database_content() {
        let (state, dir) = test_state();
        {
            let conn = state.conn.lock().expect("lock");
            conn.execute(
                "INSERT INTO resources (id, kind, name, created_at, updated_at)
                 VALUES ('r1', 'file', 'a.txt', 1, 1)",
                [],
            )
            .expect("insert");
        }
        let (backup_dir, _) = create_backup(&state, false, "manual").expect("backup");

        {
            let conn = state.conn.lock().expect("lock");
            conn.execute(
                "INSERT INTO resources (id, kind, name, created_at, updated_at)
                 VALUES ('r2', 'file', 'b.txt', 1, 1)",
                [],
            )
            .expect("insert2");
        }

        restore_from_dir(&state, &backup_dir).expect("restore");

        let conn = state.conn.lock().expect("lock");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 1);
        drop(conn);
        // 保护备份目录已落盘（恢复会覆盖 backup_records 表，因此校验磁盘目录）
        assert!(
            backup_dir_count(&dir) >= 2,
            "protect backup dir should exist on disk"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_rejects_missing_manifest() {
        let (state, dir) = test_state();
        let bad_dir = dir.join("bad-backup");
        std::fs::create_dir_all(&bad_dir).expect("mkdir");
        assert!(restore_from_dir(&state, &bad_dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

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
        let manifest_path = backup_dir.join("manifest.json");
        let mut m: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        m["database_version"] = serde_json::json!(9999);
        std::fs::write(&manifest_path, serde_json::to_string_pretty(&m).unwrap()).unwrap();
        assert!(validate_backup(&backup_dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_backup_rejects_missing_db() {
        let (state, dir) = test_state();
        let (backup_dir, _) = create_backup(&state, false, "manual").expect("backup");
        std::fs::remove_file(backup_dir.join("workspace.db")).unwrap();
        assert!(validate_backup(&backup_dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_rolls_back_on_corrupt_db() {
        let (state, dir) = test_state();
        {
            let conn = state.conn.lock().expect("lock");
            conn.execute(
                "INSERT INTO resources (id, kind, name, created_at, updated_at)
                 VALUES ('r1', 'file', 'a.txt', 1, 1)",
                [],
            )
            .expect("insert");
        }
        let (backup_dir, _) = create_backup(&state, false, "manual").expect("backup");

        // 修改原库
        {
            let conn = state.conn.lock().expect("lock");
            conn.execute(
                "INSERT INTO resources (id, kind, name, created_at, updated_at)
                 VALUES ('r2', 'file', 'b.txt', 1, 1)",
                [],
            )
            .expect("insert2");
        }
        // 备份中的 db 损坏：无法作为 SQLite 打开
        std::fs::write(backup_dir.join("workspace.db"), b"not a database").unwrap();

        assert!(restore_from_dir(&state, &backup_dir).is_err());

        // 原库内容保留（r1 与 r2 都在），保护备份目录已落盘
        let conn = state.conn.lock().expect("lock");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 2, "db content preserved after failed restore");
        drop(conn);
        assert!(
            backup_dir_count(&dir) >= 2,
            "protect backup dir should exist on disk"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

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
            let (_, record) = create_backup(&state, false, "auto").expect("backup");
            let conn = state.conn.lock().expect("lock");
            conn.execute(
                "UPDATE backup_records SET created_at = ?1 WHERE id = ?2",
                rusqlite::params![1000 + i, record.id],
            )
            .expect("update time");
            drop(conn);
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

    #[test]
    fn restore_managed_move_failure_rolls_back_db() {
        let (state, dir) = test_state();

        // 当前托管目录：有一个文件
        let managed = state.managed_dir.lock().expect("lock").clone();
        std::fs::create_dir_all(&managed).unwrap();
        std::fs::write(managed.join("current.txt"), "cur").unwrap();

        // 备份源：managed-files 含一个文件
        let src = dir.join("backup-src").join("managed-files");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), "a").unwrap();

        // 恢复前数据库含 r2（应存在于保护快照）
        {
            let conn = state.conn.lock().expect("lock");
            conn.execute(
                "INSERT INTO resources (id, kind, name, created_at, updated_at)
                 VALUES ('r2', 'file', 'b.txt', 1, 1)",
                [],
            )
            .expect("insert r2");
            // 保护快照 = 当前数据库（含 r2）
            let protect = dir.join("protect");
            std::fs::create_dir_all(&protect).unwrap();
            conn.backup("main", protect.join("workspace.db"), None)
                .unwrap();
        }

        // 制造移动失败：protect 目标已存在同名非空目录 → rename 失败
        let protect = dir.join("protect");
        let protect_managed = protect.join("managed-files-current");
        std::fs::create_dir_all(&protect_managed).unwrap();
        std::fs::write(protect_managed.join("conflict.txt"), "x").unwrap();

        let err = restore_managed_files(&state, &src, &managed, &protect_managed, &protect)
            .expect_err("should fail");
        assert!(
            err.message.contains("数据库已回滚"),
            "移动失败必须回滚数据库，实际: {}",
            err.message
        );

        // 数据库回滚到保护快照：r2 仍在
        let conn = state.conn.lock().expect("lock");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources WHERE id='r2'", [], |r| {
                r.get(0)
            })
            .expect("count");
        assert_eq!(count, 1, "数据库应回滚，r2 必须保留");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
