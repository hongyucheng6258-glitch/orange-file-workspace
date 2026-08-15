use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::backup::Backup;
use rusqlite::{params, Connection};

use crate::AppState;
use crate::db::connection::now_unix;
use crate::db::models::new_id;
use crate::error::AppError;

/// 创建备份。include_files=true 时同时复制托管文件。
/// 返回 (备份目录, 记录)。
pub fn create_backup(
    state: &AppState,
    include_files: bool,
) -> Result<(PathBuf, crate::db::models::BackupRecord), AppError> {
    let backups_root = state.data_dir.join("backups");
    std::fs::create_dir_all(&backups_root)?;

    let ts = chrono_like_timestamp();
    let backup_dir = backups_root.join(format!("backup-{ts}"));
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
        copy_dir_all(
            &state.data_dir.join("managed-files"),
            &backup_dir.join("managed-files"),
        )?;
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
        backup_type: if include_files { "full".into() } else { "metadata".into() },
        database_version: crate::db::migrations::MIGRATIONS
            .last()
            .map(|m| m.version)
            .unwrap_or(0),
        resource_count: Some(resource_count),
        file_count: Some(file_count),
        created_at: now_unix(),
        status: "completed".to_string(),
        error_message: None,
    };
    conn.execute(
        "INSERT INTO backup_records (
            id, path, backup_type, database_version, resource_count,
            file_count, created_at, status, error_message
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'completed', NULL)",
        params![
            record.id,
            record.path,
            record.backup_type,
            record.database_version,
            record.resource_count,
            record.file_count,
            record.created_at
        ],
    )?;

    Ok((backup_dir, record))
}

/// 从备份目录恢复数据库（覆盖当前连接内容）。
pub fn restore_from_dir(
    state: &AppState,
    backup_dir: &Path,
) -> Result<(), AppError> {
    // 校验 manifest
    let manifest_path = backup_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(AppError::new("invalid_backup", "备份缺少 manifest.json"));
    }
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)?;
    let db_file = backup_dir.join("workspace.db");
    if !db_file.exists() {
        return Err(AppError::new(
            "invalid_backup",
            "备份缺少 workspace.db",
        ));
    }

    // 从备份文件恢复到当前打开的连接
    let src = Connection::open(&db_file)?;
    {
        let mut conn = state.conn.lock().expect("db lock");
        let backup = Backup::new(&src, &mut conn)?;
        backup.run_to_completion(5, Duration::from_millis(200), None)?;
    }

    // 可选：恢复托管文件
    let managed_src = backup_dir.join("managed-files");
    if managed_src.exists() {
        let managed_dst = state.data_dir.join("managed-files");
        if managed_dst.exists() {
            std::fs::remove_dir_all(&managed_dst)?;
        }
        copy_dir_all(&managed_src, &managed_dst)?;
    }

    let _ = manifest;
    Ok(())
}

/// 列出备份记录。
pub fn list_backups(state: &AppState) -> Result<Vec<crate::db::models::BackupRecord>, AppError> {
    let conn = state.conn.lock().expect("db lock");
    let mut stmt = conn.prepare(
        "SELECT * FROM backup_records ORDER BY created_at DESC LIMIT 50",
    )?;
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
        })
    })?;
    let mut records = Vec::new();
    for r in rows {
        records.push(r?);
    }
    Ok(records)
}

/// 复制目录（递归）。
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), AppError> {
    if !src.exists() {
        return Ok(());
    }
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

/// 备份目录名用的时间戳（YYYYMMDD-HHMMSS）。
fn chrono_like_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 简单可读格式：Unix 秒
    format!("{now}")
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
                data_dir: dir.clone(),
                conn: std::sync::Mutex::new(conn),
            },
            dir,
        )
    }

    #[test]
    fn create_backup_produces_consistent_snapshot() {
        let (state, dir) = test_state();
        // 插入一条资源
        {
            let conn = state.conn.lock().expect("lock");
            conn.execute(
                "INSERT INTO resources (id, kind, name, created_at, updated_at)
                 VALUES ('r1', 'file', 'a.txt', 1, 1)",
                [],
            )
            .expect("insert");
        }

        let (backup_dir, record) = create_backup(&state, false).expect("backup");
        assert!(backup_dir.join("workspace.db").exists());
        assert!(backup_dir.join("manifest.json").exists());
        assert_eq!(record.backup_type, "metadata");

        // 备份文件应包含数据
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
        let (backup_dir, _) = create_backup(&state, false).expect("backup");

        // 修改原库（添加新资源）
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

        // 恢复后只剩备份中的资源
        let conn = state.conn.lock().expect("lock");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 1);

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
}
