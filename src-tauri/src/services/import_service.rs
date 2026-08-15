use std::path::{Path, PathBuf};

use rusqlite::params;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;
use crate::db::connection::now_unix;
use crate::db::models::{new_id, FileMetadata, ResourceKind, ResourceLocation, SourceType};
use crate::db::repositories as repo;
use crate::error::AppError;
use crate::events::EVENT_TASK_PROGRESS;
use crate::services::file_service as fsutil;
use crate::services::task_service as tasks;

/// 导入时需要跳过的目录（项目依赖、构建产物和缓存）。
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    ".cache",
    "__pycache__",
    ".venv",
    "venv",
    ".idea",
    ".vscode",
    ".next",
    "build",
];

/// 批量提交间隔（条）。
const BATCH_SIZE: usize = 500;
/// 进度事件间隔（条）。
const PROGRESS_EVERY: usize = 50;
/// 取消检查间隔（条）。
const CANCEL_CHECK_EVERY: usize = 100;

#[derive(Clone)]
pub struct ImportRequest {
    pub paths: Vec<String>,
    pub mode: SourceType,
    pub parent_id: Option<String>,
}

/// 待批量写入数据库的一条导入结果。
struct PendingImport {
    resource_id: String,
    name: String,
    parent_id: Option<String>,
    mode: SourceType,
    path: String,
    canonical: String,
    size: i64,
    modified: Option<i64>,
    extension: Option<String>,
    mime: Option<String>,
    now: i64,
}

/// 启动后台导入任务。
pub fn start_import(app: AppHandle, task_id: String, req: ImportRequest) {
    std::thread::spawn(move || {
        if let Err(e) = run_import(&app, &task_id, &req) {
            let state = app.state::<AppState>();
            let conn = state.conn.lock().expect("db lock");
            let _ = tasks::mark_failed(&conn, &task_id, &e.to_string());
        }
    });
}

fn run_import(
    app: &AppHandle,
    task_id: &str,
    req: &ImportRequest,
) -> Result<(), AppError> {
    let state = app.state::<AppState>();

    // 收集待导入文件
    let mut files: Vec<PathBuf> = Vec::new();
    for p in &req.paths {
        let path = PathBuf::from(p);
        if path.exists() {
            collect_files(&path, &mut files);
        }
    }
    let total = files.len() as i64;

    {
        let conn = state.conn.lock().expect("db lock");
        tasks::mark_running(&conn, task_id)?;
        conn.execute(
            "UPDATE tasks SET total_count = ?2 WHERE id = ?1",
            params![task_id, total],
        )?;
    }
    emit_progress(app, task_id, "running", 0, 0, total);

    let managed_root = state.data_dir.join("managed-files");
    std::fs::create_dir_all(&managed_root)?;

    let mut completed: i64 = 0;
    let mut failed: i64 = 0;
    let mut batch: Vec<PendingImport> = Vec::with_capacity(BATCH_SIZE);

    for (idx, file) in files.iter().enumerate() {
        // 周期检查取消
        if idx % CANCEL_CHECK_EVERY == 0 {
            let conn = state.conn.lock().expect("db lock");
            if let Ok(Some(task)) = tasks::get_task(&conn, task_id) {
                if task.status == "cancelled" {
                    tasks::mark_cancelled(&conn, task_id)?;
                    return Ok(());
                }
            }
        }

        match import_one(&state, &managed_root, file, req.mode, req.parent_id.as_deref()) {
            Ok(pending) => {
                batch.push(pending);
                completed += 1;
            }
            Err(e) => {
                failed += 1;
                record_failure(&state, task_id, file, &e.to_string());
            }
        }

        // 批量提交
        if batch.len() >= BATCH_SIZE {
            let mut conn = state.conn.lock().expect("db lock");
            flush_batch(&mut conn, &batch)?;
            drop(conn);
            batch.clear();
        }

        // 周期发送进度
        if (idx + 1) % PROGRESS_EVERY == 0 || idx + 1 == files.len() {
            let conn = state.conn.lock().expect("db lock");
            let _ = tasks::update_progress(&conn, task_id, completed, failed);
            drop(conn);
            emit_progress(app, task_id, "running", completed, failed, total);
        }
    }

    // 提交剩余批次
    if !batch.is_empty() {
        let mut conn = state.conn.lock().expect("db lock");
        flush_batch(&mut conn, &batch)?;
    }

    {
        let conn = state.conn.lock().expect("db lock");
        tasks::update_progress(&conn, task_id, completed, failed)?;
        tasks::mark_completed(&conn, task_id)?;
    }
    emit_progress(app, task_id, "completed", completed, failed, total);

    let _ = app.emit(
        "resource-changed",
        serde_json::json!({ "parent_id": req.parent_id }),
    );

    Ok(())
}

/// 单文件导入：复制文件（managed 模式），暂不写库。
fn import_one(
    state: &AppState,
    managed_root: &Path,
    src: &Path,
    mode: SourceType,
    parent_id: Option<&str>,
) -> Result<PendingImport, AppError> {
    let (size, modified) = fsutil::stat_basic(src)?;
    let name = src
        .file_name()
        .ok_or_else(|| AppError::new("path_error", "无法获取文件名"))?
        .to_string_lossy()
        .to_string();

    let now = now_unix();
    let extension = src.extension().map(|e| e.to_string_lossy().to_lowercase());
    let mime = fsutil::infer_mime(src);

    let (path, canonical) = match mode {
        SourceType::Managed => {
            let ext = extension
                .as_deref()
                .map(|e| format!(".{e}"))
                .unwrap_or_default();
            let dest = managed_root.join(format!("{}{ext}", new_id()));
            std::fs::copy(src, &dest)?;
            let normalized = fsutil::normalize_path(&dest)?;
            (normalized.clone(), fsutil::canonical_path_key(&normalized))
        }
        SourceType::External => {
            let normalized = fsutil::normalize_path(src)?;
            (normalized.clone(), fsutil::canonical_path_key(&normalized))
        }
    };

    Ok(PendingImport {
        resource_id: new_id(),
        name,
        parent_id: parent_id.map(|s| s.to_string()),
        mode,
        path,
        canonical,
        size,
        modified,
        extension,
        mime,
        now,
    })
}

/// 单事务批量写入一批导入结果。
fn flush_batch(conn: &mut rusqlite::Connection, batch: &[PendingImport]) -> Result<(), AppError> {
    let tx = conn.transaction()?;
    for item in batch {
        tx.execute(
            "INSERT INTO resources (id, kind, name, parent_id, created_at, updated_at)
             VALUES (?1, 'file', ?2, ?3, ?4, ?4)",
            params![item.resource_id, item.name, item.parent_id, item.now],
        )?;
        tx.execute(
            "INSERT INTO resource_locations (
                id, resource_id, source_type, path, canonical_path,
                file_size, modified_at, created_at, last_verified_at,
                content_hash, hash_algorithm, is_available
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, NULL, NULL, 1)",
            params![
                new_id(),
                item.resource_id,
                item.mode.as_str(),
                item.path,
                item.canonical,
                item.size,
                item.modified,
                item.now,
            ],
        )?;
        tx.execute(
            "INSERT INTO file_metadata (
                resource_id, extension, mime_type, size_bytes,
                width, height, duration_ms, encoding, line_count,
                is_binary, preview_kind, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, NULL, NULL, 0, NULL, NULL)",
            params![item.resource_id, item.extension, item.mime, item.size],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// 记录单个失败项到 task_items。
fn record_failure(state: &AppState, task_id: &str, src: &Path, message: &str) {
    let conn = state.conn.lock().expect("db lock");
    let _ = conn.execute(
        "INSERT INTO task_items (id, task_id, source_path, status, error_message, updated_at)
         VALUES (?1, ?2, ?3, 'failed', ?4, ?5)",
        params![new_id(), task_id, src.to_string_lossy(), message, now_unix()],
    );
}

/// 递归收集文件，跳过忽略目录。
fn collect_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        out.push(path.to_path_buf());
        return;
    }
    if !path.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if should_skip(&p) {
                continue;
            }
            collect_files(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// 是否跳过该目录。
fn should_skip(path: &Path) -> bool {
    path.file_name()
        .map(|n| {
            let n = n.to_string_lossy();
            SKIP_DIRS.iter().any(|s| *s == n.as_ref())
        })
        .unwrap_or(false)
}

fn emit_progress(
    app: &AppHandle,
    task_id: &str,
    status: &str,
    completed: i64,
    failed: i64,
    total: i64,
) {
    let _ = app.emit(
        EVENT_TASK_PROGRESS,
        serde_json::json!({
            "task_id": task_id,
            "status": status,
            "completed": completed,
            "failed": failed,
            "total": total,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_files_skips_ignored_dirs() {
        let root = std::env::temp_dir().join(format!("nexus-import-{}", crate::db::models::new_id()));
        std::fs::create_dir_all(root.join("node_modules/pkg")).expect("mkdir");
        std::fs::create_dir_all(root.join(".git")).expect("mkdir");
        std::fs::create_dir_all(root.join("src")).expect("mkdir");

        std::fs::write(root.join("src/main.rs"), "fn main() {}").expect("write");
        std::fs::write(root.join("node_modules/pkg/index.js"), "x").expect("write");
        std::fs::write(root.join(".git/config"), "x").expect("write");
        std::fs::write(root.join("Cargo.toml"), "[package]").expect("write");

        let mut files = Vec::new();
        collect_files(&root, &mut files);

        let names: Vec<String> = files
            .iter()
            .map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().to_string())
            .collect();

        assert!(names.contains(&"src\\main.rs".to_string()) || names.contains(&"src/main.rs".to_string()));
        assert!(names.contains(&"Cargo.toml".to_string()));
        assert!(
            !names.iter().any(|n| n.contains("node_modules") || n.contains(".git")),
            "ignored dirs must not appear: {names:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn single_file_collection() {
        let tmp = std::env::temp_dir().join(format!("nexus-file-{}", crate::db::models::new_id()));
        std::fs::write(&tmp, b"hello").expect("write");

        let mut files = Vec::new();
        collect_files(&tmp, &mut files);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], tmp);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn should_skip_matches_known_dirs() {
        assert!(should_skip(Path::new("C:\\proj\\node_modules")));
        assert!(should_skip(Path::new("C:\\proj\\target")));
        assert!(should_skip(Path::new("C:\\proj\\.git")));
        assert!(!should_skip(Path::new("C:\\proj\\src")));
        assert!(!should_skip(Path::new("C:\\proj\\README.md")));
    }

    #[test]
    fn flush_batch_inserts_three_tables() {
        let mut conn = rusqlite::Connection::open_in_memory().expect("db");
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
        crate::db::migrations::run_migrations(&mut conn).expect("migrations");

        let now = now_unix();
        let batch = vec![PendingImport {
            resource_id: "r1".to_string(),
            name: "a.txt".to_string(),
            parent_id: None,
            mode: SourceType::External,
            path: "C:\\x\\a.txt".to_string(),
            canonical: "c:\\x\\a.txt".to_string(),
            size: 12,
            modified: Some(now),
            extension: Some("txt".to_string()),
            mime: Some("text/plain".to_string()),
            now,
        }];

        flush_batch(&mut conn, &batch).expect("flush");

        let rc: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
            .expect("count");
        assert_eq!(rc, 1);
        let lc: i64 = conn
            .query_row("SELECT COUNT(*) FROM resource_locations", [], |r| r.get(0))
            .expect("count");
        assert_eq!(lc, 1);
        let mc: i64 = conn
            .query_row("SELECT COUNT(*) FROM file_metadata", [], |r| r.get(0))
            .expect("count");
        assert_eq!(mc, 1);
    }
}
