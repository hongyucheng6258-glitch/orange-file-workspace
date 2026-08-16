use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;
use crate::db::connection::now_unix;
use crate::db::models::{new_id, ResourceKind, SourceType};
use crate::error::AppError;
use crate::events::EVENT_TASK_PROGRESS;
use crate::services::file_service as fsutil;
use crate::services::settings_service;
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

/// 待批量写入数据库的一条导入结果（文件或文件夹）。
struct PendingImport {
    resource_id: String,
    kind: ResourceKind,
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

/// 解析 Windows 快捷方式（.lnk）的目标路径。
/// 非快捷方式或解析失败时返回原路径。
pub fn resolve_shortcut(path: &Path) -> PathBuf {
    let is_lnk = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("lnk"))
        .unwrap_or(false);
    if is_lnk {
        if let Some(target) = resolve_lnk_target(path) {
            return target;
        }
    }
    path.to_path_buf()
}

/// 收集一批待导入路径：快捷方式（.lnk）先解析为目标路径，再按目录/文件分流。
/// 目录走 collect_tree 保留层级，文件走 build_file；失败记录原始路径。
/// 拖拽事件可能返回 GBK 乱码路径（如 `E:\ʵϰ`），此处仅在原路径不存在时尝试修复。
fn collect_imports(
    paths: &[String],
    mode: SourceType,
    parent_id: Option<String>,
    managed_root: &Path,
    ignore_rules: &[settings_service::IgnoreRule],
    out: &mut Vec<PendingImport>,
    failures: &mut Vec<(PathBuf, String)>,
) {
    for p in paths {
        let raw = PathBuf::from(p);
        let mut path = raw.clone();
        if !path.exists() {
            if let Some(fixed) = repair_gbk_path(p) {
                let fixed_path = PathBuf::from(&fixed);
                if fixed_path.exists() {
                    path = fixed_path;
                }
            }
        }
        let path = resolve_shortcut(&path);
        if path.is_dir() {
            collect_tree(&path, managed_root, mode, parent_id.clone(), ignore_rules, out, failures);
        } else if path.is_file() {
            if is_ignored(&path, ignore_rules) {
                continue;
            }
            match build_file(&path, managed_root, mode, parent_id.clone()) {
                Ok(item) => out.push(item),
                Err(e) => failures.push((raw, e.to_string())),
            }
        } else {
            failures.push((raw, "路径不存在或不可访问".to_string()));
        }
    }
}

/// 尝试把 GBK 乱码路径修复为正确的 UTF-8 路径。
///
/// 中文 Windows 下拖拽事件返回的路径偶发以 GBK 字节被误按 UTF-8 解码，
/// 例如「实习」的 GBK 字节 `CA B5 CF B0` 会被解码成 `ʵϰ`。此处将该字符串
/// 的 UTF-8 字节流按 GBK(936) 重新解码；解码失败或结果不变时返回 None。
#[cfg(target_os = "windows")]
fn repair_gbk_path(s: &str) -> Option<String> {
    use windows::Win32::Globalization::{MultiByteToWideChar, MULTI_BYTE_TO_WIDE_CHAR_FLAGS};

    let bytes = s.as_bytes();
    let mut wide = vec![0u16; bytes.len() + 2];
    let written = unsafe { MultiByteToWideChar(936, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, Some(&mut wide)) };
    if written <= 0 {
        return None;
    }
    let repaired = String::from_utf16(&wide[..written as usize]).ok()?;
    if repaired == s {
        None
    } else {
        Some(repaired)
    }
}

#[cfg(not(target_os = "windows"))]
fn repair_gbk_path(_s: &str) -> Option<String> {
    None
}

/// 手动解析 .lnk（Shell Link Binary Format）中的 LocalBasePath。
/// 返回快捷方式指向的绝对路径（文件或文件夹）。
fn resolve_lnk_target(path: &Path) -> Option<PathBuf> {
    use std::io::Read;

    let mut data = Vec::new();
    std::fs::File::open(path).ok()?.read_to_end(&mut data).ok()?;
    if data.len() < 0x4C || data[0..4] != [0x4C, 0x00, 0x00, 0x00] {
        return None;
    }
    let link_flags = u32::from_le_bytes(data[0x14..0x18].try_into().ok()?);
    const HAS_LINK_TARGET_IDLIST: u32 = 0x0000_0001;
    const HAS_LINK_INFO: u32 = 0x0000_0002;

    let mut offset: usize = 0x4C;
    if link_flags & HAS_LINK_TARGET_IDLIST != 0 {
        let id_list_size =
            u16::from_le_bytes(data.get(offset..offset + 2)?.try_into().ok()?) as usize;
        offset += 2 + id_list_size;
    }
    if link_flags & HAS_LINK_INFO == 0 || offset + 0x1C > data.len() {
        return None;
    }
    let link_info = offset;
    let link_info_flags = u32::from_le_bytes(data[link_info + 8..link_info + 12].try_into().ok()?);
    const VOLUME_ID_AND_LOCAL_BASE_PATH: u32 = 0x0000_0001;
    const LOCAL_BASE_PATH_UNICODE: u32 = 0x0000_0002;
    if link_info_flags & VOLUME_ID_AND_LOCAL_BASE_PATH == 0 {
        return None;
    }

    let base_path_at = |rel_field: usize| -> Option<usize> {
        let rel = u32::from_le_bytes(
            data.get(link_info + rel_field..link_info + rel_field + 4)?
                .try_into()
                .ok()?,
        ) as usize;
        Some(link_info + rel)
    };

    // Unicode 版本（Windows Vista+ 常用）
    if link_info_flags & LOCAL_BASE_PATH_UNICODE != 0 {
        if let Some(pos) = base_path_at(0x1C) {
            let mut units: Vec<u16> = Vec::new();
            let mut i = pos;
            while i + 1 < data.len() {
                let u = u16::from_le_bytes([data[i], data[i + 1]]);
                if u == 0 {
                    break;
                }
                units.push(u);
                i += 2;
            }
            if !units.is_empty() {
                if let Ok(target) = String::from_utf16(&units) {
                    if !target.is_empty() {
                        return Some(PathBuf::from(target));
                    }
                }
            }
        }
    }

    // ANSI 版本（回退）
    if let Some(pos) = base_path_at(0x10) {
        let end = data[pos..]
            .iter()
            .position(|&b| b == 0)
            .map(|p| pos + p)
            .unwrap_or(data.len());
        if end > pos {
            if let Ok(target) = String::from_utf8(data[pos..end].to_vec()) {
                if !target.is_empty() {
                    return Some(PathBuf::from(target));
                }
            }
        }
    }
    None
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

    let managed_root = state.managed_dir.lock().expect("dir lock").clone();
    std::fs::create_dir_all(&managed_root)?;

    // 读取设置：忽略规则与重复策略
    let (ignore_rules, allow_duplicates) = {
        let conn = state.conn.lock().expect("db lock");
        match settings_service::load_settings(&conn) {
            Ok(s) => (
                s.ignore.custom_rules,
                s.general.duplicate_policy == "keep_both",
            ),
            Err(_) => (Vec::new(), false),
        }
    };

    // 收集待导入的文件与目录（保留层级）。
    let mut pending: Vec<PendingImport> = Vec::new();
    let mut failures: Vec<(PathBuf, String)> = Vec::new();
    collect_imports(
        &req.paths,
        req.mode,
        req.parent_id.clone(),
        &managed_root,
        &ignore_rules,
        &mut pending,
        &mut failures,
    );

    let total = (pending.len() + failures.len()) as i64;
    let total_pending = pending.len();

    {
        let conn = state.conn.lock().expect("db lock");
        tasks::mark_running(&conn, task_id)?;
        conn.execute(
            "UPDATE tasks SET total_count = ?2 WHERE id = ?1",
            params![task_id, total],
        )?;
    }
    emit_progress(app, task_id, "running", 0, 0, total);

    let mut completed: i64 = 0;
    let mut failed: i64 = 0;
    let mut batch: Vec<PendingImport> = Vec::with_capacity(BATCH_SIZE);

    for (idx, item) in pending.into_iter().enumerate() {
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

        batch.push(item);
        completed += 1;

        // 批量提交
        if batch.len() >= BATCH_SIZE {
            let mut conn = state.conn.lock().expect("db lock");
            flush_batch(&mut conn, &batch, allow_duplicates)?;
            drop(conn);
            batch.clear();
        }

        // 周期发送进度
        if (idx + 1) % PROGRESS_EVERY == 0 || idx + 1 == total_pending + failures.len() {
            let conn = state.conn.lock().expect("db lock");
            let _ = tasks::update_progress(&conn, task_id, completed, failed);
            drop(conn);
            emit_progress(app, task_id, "running", completed, failed, total);
        }
    }

    // 记录收集阶段失败的路径
    for (path, message) in &failures {
        failed += 1;
        record_failure(&state, task_id, path, message);
    }

    // 提交剩余批次
    if !batch.is_empty() {
        let mut conn = state.conn.lock().expect("db lock");
        flush_batch(&mut conn, &batch, allow_duplicates)?;
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

/// 递归收集目录树：为每个目录创建 folder 资源，文件挂载到对应目录下。
/// managed 模式会在 managed_root 下镜像目录结构。
fn collect_tree(
    node: &Path,
    dest_parent: &Path,
    mode: SourceType,
    parent_resource_id: Option<String>,
    ignore_rules: &[settings_service::IgnoreRule],
    out: &mut Vec<PendingImport>,
    failures: &mut Vec<(PathBuf, String)>,
) {
    let Some(name_os) = node.file_name() else {
        failures.push((node.to_path_buf(), "无法获取目录名".to_string()));
        return;
    };
    let name = name_os.to_string_lossy().to_string();
    let dir_id = new_id();
    let now = now_unix();
    let dest_dir = dest_parent.join(&name);

    if mode == SourceType::Managed {
        if let Err(e) = std::fs::create_dir_all(&dest_dir) {
            failures.push((node.to_path_buf(), format!("创建目录失败: {e}")));
            return;
        }
    }

    let (path, canonical) = match mode {
        SourceType::Managed => {
            match fsutil::normalize_path(&dest_dir) {
                Ok(n) => (n.clone(), fsutil::canonical_path_key(&n)),
                Err(e) => {
                    failures.push((node.to_path_buf(), e.to_string()));
                    return;
                }
            }
        }
        SourceType::External => match fsutil::normalize_path(node) {
            Ok(n) => (n.clone(), fsutil::canonical_path_key(&n)),
            Err(e) => {
                failures.push((node.to_path_buf(), e.to_string()));
                return;
            }
        },
    };

    out.push(PendingImport {
        resource_id: dir_id.clone(),
        kind: ResourceKind::Folder,
        name,
        parent_id: parent_resource_id,
        mode,
        path,
        canonical,
        size: 0,
        modified: None,
        extension: None,
        mime: None,
        now,
    });

    let Ok(entries) = std::fs::read_dir(node) else {
        failures.push((node.to_path_buf(), "读取目录失败".to_string()));
        return;
    };

    for entry in entries.flatten() {
        // 不跟随符号链接与目录联接（防循环递归与越界复制）
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let p = entry.path();
        if is_ignored(&p, ignore_rules) {
            continue;
        }
        if ft.is_dir() {
            collect_tree(&p, &dest_dir, mode, Some(dir_id.clone()), ignore_rules, out, failures);
        } else if ft.is_file() {
            match build_file(&p, &dest_dir, mode, Some(dir_id.clone())) {
                Ok(item) => out.push(item),
                Err(e) => failures.push((p, e.to_string())),
            }
        }
    }
}

/// 单文件导入：managed 模式复制到 dest_dir 下，external 模式仅引用原路径。
fn build_file(
    src: &Path,
    dest_dir: &Path,
    mode: SourceType,
    parent_id: Option<String>,
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
            let dest = dest_dir.join(format!("{}{ext}", new_id()));
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
        kind: ResourceKind::File,
        name,
        parent_id,
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

/// 单事务批量写入一批导入结果（文件与文件夹）。
///
/// 幂等策略：已导入过的路径（source_type + canonical_path）不再重复创建。
/// 已存在的文件夹会复用其原资源 ID，新加入的子文件仍可挂载到该文件夹下。
/// 已被软删除（或位于已删除目录树下）的文件夹不复用，改为重建，避免新文件
/// 挂到用户不可见的旧目录树上。
fn flush_batch(
    conn: &mut rusqlite::Connection,
    batch: &[PendingImport],
    allow_duplicates: bool,
) -> Result<(), AppError> {
    let tx = conn.transaction()?;
    // new_id -> existing_id：本次收集的新文件夹 ID 若对应已存在位置，则重定向到原资源 ID
    let mut id_map: HashMap<String, String> = HashMap::new();
    for item in batch {
        let parent_id = item
            .parent_id
            .as_ref()
            .and_then(|p| id_map.get(p).cloned())
            .or_else(|| item.parent_id.clone());

        let existing: Option<(String, bool)> = tx
            .query_row(
                "SELECT rl.resource_id, (r.id IS NOT NULL AND r.is_deleted = 0) AS res_exists
                 FROM resource_locations rl
                 LEFT JOIN resources r ON r.id = rl.resource_id
                 WHERE rl.source_type = ?1 AND rl.canonical_path = ?2",
                params![item.mode.as_str(), item.canonical],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)),
            )
            .optional()?;
        if let Some((existing_id, res_exists)) = existing {
            if allow_duplicates {
                // keep_both：允许同一路径重复导入（文件夹不复用，重建新树）
                tx.execute(
                    "DELETE FROM resource_locations WHERE resource_id = ?1",
                    params![existing_id],
                )?;
                tx.execute(
                    "DELETE FROM file_metadata WHERE resource_id = ?1",
                    params![existing_id],
                )?;
            } else if res_exists {
                // 校验祖先链：祖先已被删除则不能复用（新文件会挂到不可见目录）。
                // 文件夹与文件都要检查：文件若挂在已删除的目录/项目下，复用后同样不可见。
                let reusable = !has_deleted_ancestor(&tx, &existing_id)?;
                if reusable {
                    // 资源仍在且可见：跳过重复导入，复用已有文件夹 ID
                    if item.kind == ResourceKind::Folder {
                        id_map.insert(item.resource_id.clone(), existing_id);
                    }
                    continue;
                }
                // 孤立或已删除的位置记录：清理后继续创建新资源
                tx.execute(
                    "DELETE FROM resource_locations WHERE resource_id = ?1",
                    params![existing_id],
                )?;
                tx.execute(
                    "DELETE FROM file_metadata WHERE resource_id = ?1",
                    params![existing_id],
                )?;
            } else {
                // 孤立位置记录：清理后继续创建新资源
                tx.execute(
                    "DELETE FROM resource_locations WHERE resource_id = ?1",
                    params![existing_id],
                )?;
                tx.execute(
                    "DELETE FROM file_metadata WHERE resource_id = ?1",
                    params![existing_id],
                )?;
            }
        }

        let kind = match item.kind {
            ResourceKind::Folder => "folder",
            _ => "file",
        };
        tx.execute(
            "INSERT INTO resources (id, kind, name, parent_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![item.resource_id, kind, item.name, parent_id, item.now],
        )?;
        let size: Option<i64> = if item.kind == ResourceKind::Folder {
            None
        } else {
            Some(item.size)
        };
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
                size,
                item.modified,
                item.now,
            ],
        )?;
        if item.kind == ResourceKind::File {
            tx.execute(
                "INSERT INTO file_metadata (
                    resource_id, extension, mime_type, size_bytes,
                    width, height, duration_ms, encoding, line_count,
                    is_binary, preview_kind, metadata_json
                 ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, NULL, NULL, 0, NULL, NULL)",
                params![item.resource_id, item.extension, item.mime, item.size],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// 判断资源自身的祖先链（含自身）中是否存在已删除节点。
///
/// 软删除只标记单个节点，其子树节点仍为未删除状态；若按 canonical 路径复用
/// 这类子树文件夹，新导入的文件会挂到用户不可见的目录树上。
fn has_deleted_ancestor(
    tx: &rusqlite::Transaction<'_>,
    resource_id: &str,
) -> Result<bool, AppError> {
    let deleted: bool = tx.query_row(
        "WITH RECURSIVE anc(id, parent_id, is_deleted) AS (
            SELECT id, parent_id, is_deleted FROM resources WHERE id = ?1
            UNION ALL
            SELECT p.id, p.parent_id, p.is_deleted
            FROM resources p JOIN anc a ON a.parent_id = p.id
         )
         SELECT EXISTS(SELECT 1 FROM anc WHERE is_deleted = 1)",
        params![resource_id],
        |r| r.get::<_, bool>(0),
    )?;
    Ok(deleted)
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

/// 是否跳过该目录。
fn should_skip(path: &Path) -> bool {
    path.file_name()
        .map(|n| {
            let n = n.to_string_lossy();
            SKIP_DIRS.iter().any(|s| *s == n.as_ref())
        })
        .unwrap_or(false)
}

/// 判断路径是否应被忽略：内置目录 + 自定义规则（仅 enabled）。
/// name 规则做精确或前缀（`*` 结尾）匹配；path 规则做路径包含匹配。
pub fn is_ignored(path: &Path, rules: &[settings_service::IgnoreRule]) -> bool {
    if should_skip(path) {
        return true;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let path_lower = path.to_string_lossy().to_lowercase();
    for rule in rules {
        if !rule.enabled {
            continue;
        }
        let pat = rule.pattern.to_lowercase();
        let matched = match rule.kind.as_str() {
            "name" => {
                name == pat
                    || (pat.ends_with('*') && name.starts_with(&pat.trim_end_matches('*')))
            }
            _ => path_lower.contains(&pat),
        };
        if matched {
            return true;
        }
    }
    false
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

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nexus-{tag}-{}", new_id()))
    }

    #[test]
    fn collect_tree_preserves_folder_hierarchy() {
        let root = temp_dir("tree");
        std::fs::create_dir_all(root.join("src/sub")).expect("mkdir");
        std::fs::create_dir_all(root.join("node_modules/pkg")).expect("mkdir");
        std::fs::write(root.join("src/main.rs"), "fn main() {}").expect("write");
        std::fs::write(root.join("src/sub/lib.rs"), "pub fn f() {}").expect("write");
        std::fs::write(root.join("node_modules/pkg/index.js"), "x").expect("write");
        std::fs::write(root.join("Cargo.toml"), "[package]").expect("write");

        let managed = temp_dir("mroot");
        let mut pending = Vec::new();
        let mut failures = Vec::new();
        collect_tree(&root, &managed, SourceType::External, None, &[], &mut pending, &mut failures);

        assert!(failures.is_empty(), "failures: {failures:?}");

        // 顶层目录 + src + src/sub 三个文件夹
        let folders: Vec<&PendingImport> = pending
            .iter()
            .filter(|p| p.kind == ResourceKind::Folder)
            .collect();
        assert_eq!(folders.len(), 3, "folders: {:?}", folders.iter().map(|f| &f.name).collect::<Vec<_>>());

        // 文件：src/main.rs、src/sub/lib.rs、Cargo.toml（跳过 node_modules）
        let files: Vec<&PendingImport> = pending
            .iter()
            .filter(|p| p.kind == ResourceKind::File)
            .collect();
        assert_eq!(files.len(), 3);

        // 层级：src 下的文件挂到 src 目录资源
        let src_dir = folders.iter().find(|f| f.name == "src").expect("src folder");
        let main_rs = files.iter().find(|f| f.name == "main.rs").expect("main.rs");
        assert_eq!(main_rs.parent_id.as_deref(), Some(src_dir.resource_id.as_str()));

        // 忽略目录不出现
        assert!(
            !files.iter().any(|f| f.name == "index.js"),
            "node_modules must be skipped"
        );

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&managed);
    }

    #[test]
    fn single_file_collection() {
        let tmp = temp_dir("file");
        std::fs::write(&tmp, b"hello").expect("write");

        let managed = temp_dir("mroot");
        let item = build_file(&tmp, &managed, SourceType::External, None).expect("build");
        assert_eq!(item.kind, ResourceKind::File);
        assert_eq!(item.name, tmp.file_name().unwrap().to_string_lossy());
        assert!(item.parent_id.is_none());

        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_dir_all(&managed);
    }

    #[test]
    fn managed_mode_creates_mirror_structure() {
        let root = temp_dir("mmirror");
        std::fs::create_dir_all(root.join("docs")).expect("mkdir");
        std::fs::write(root.join("docs/a.txt"), "hi").expect("write");

        let managed = temp_dir("mroot");
        let mut pending = Vec::new();
        let mut failures = Vec::new();
        collect_tree(&root, &managed, SourceType::Managed, None, &[], &mut pending, &mut failures);

        assert!(failures.is_empty(), "failures: {failures:?}");

        // managed 目录被镜像创建
        assert!(managed.join(root.file_name().unwrap()).join("docs").is_dir());
        let file = pending
            .iter()
            .find(|p| p.kind == ResourceKind::File)
            .expect("file");
        assert!(Path::new(&file.path).exists(), "copied file exists");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&managed);
    }

    /// 构造最小 Shell Link 二进制（ANSI LocalBasePath），指向 target 路径。
    fn make_lnk(target: &str) -> Vec<u8> {
        let mut v: Vec<u8> = Vec::new();
        v.extend_from_slice(&[0x4C, 0x00, 0x00, 0x00]); // HeaderSize
        v.extend_from_slice(&[
            0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x46,
        ]); // LinkCLSID
        v.extend_from_slice(&2u32.to_le_bytes()); // LinkFlags = HasLinkInfo
        v.extend_from_slice(&0x10u32.to_le_bytes()); // FileAttributes = DIRECTORY
        v.extend_from_slice(&[0u8; 8]); // CreationTime
        v.extend_from_slice(&[0u8; 8]); // AccessTime
        v.extend_from_slice(&[0u8; 8]); // WriteTime
        v.extend_from_slice(&0u32.to_le_bytes()); // FileSize
        v.extend_from_slice(&0u32.to_le_bytes()); // IconIndex
        v.extend_from_slice(&1u32.to_le_bytes()); // ShowCommand
        v.extend_from_slice(&[0u8; 2]); // HotKey
        v.extend_from_slice(&[0u8; 2]); // Reserved1
        v.extend_from_slice(&[0u8; 4]); // Reserved2
        v.extend_from_slice(&[0u8; 4]); // Reserved3
        // LinkInfo
        let mut vol_id: Vec<u8> = Vec::new();
        vol_id.extend_from_slice(&0x10u32.to_le_bytes()); // VolumeIDSize
        vol_id.extend_from_slice(&0u32.to_le_bytes()); // DriveType
        vol_id.extend_from_slice(&0u32.to_le_bytes()); // DriveSerialNumber
        vol_id.extend_from_slice(&0x10u32.to_le_bytes()); // VolumeLabelOffset
        let base = target.as_bytes();
        let local_base_path_offset = 0x1C + vol_id.len();
        let common_suffix_offset = local_base_path_offset + base.len() + 1;
        let link_info_size = common_suffix_offset + 1;
        v.extend_from_slice(&(link_info_size as u32).to_le_bytes()); // LinkInfoSize
        v.extend_from_slice(&0x1Cu32.to_le_bytes()); // LinkInfoHeaderSize
        v.extend_from_slice(&1u32.to_le_bytes()); // LinkInfoFlags = VolumeIDAndLocalBasePath
        v.extend_from_slice(&0x1Cu32.to_le_bytes()); // VolumeIDOffset
        v.extend_from_slice(&(local_base_path_offset as u32).to_le_bytes()); // LocalBasePathOffset
        v.extend_from_slice(&0u32.to_le_bytes()); // CommonNetworkRelativeLinkOffset
        v.extend_from_slice(&(common_suffix_offset as u32).to_le_bytes()); // CommonPathSuffixOffset
        v.extend_from_slice(&vol_id);
        v.extend_from_slice(base);
        v.push(0); // null 结尾
        v.push(0); // CommonPathSuffix
        v
    }

    #[test]
    fn resolves_lnk_shortcut_target() {
        let target = "C:\\some\\folder";
        let lnk = temp_dir("shortcut").with_extension("lnk");
        std::fs::write(&lnk, make_lnk(target)).expect("write lnk");
        assert_eq!(resolve_shortcut(&lnk), PathBuf::from(target));

        // 非快捷方式文件返回原路径
        let plain = temp_dir("plain-file");
        std::fs::write(&plain, b"hello").expect("write");
        assert_eq!(resolve_shortcut(&plain), plain);

        let _ = std::fs::remove_file(&lnk);
        let _ = std::fs::remove_file(&plain);
    }

    #[test]
    fn collect_imports_resolves_folder_shortcut_to_folder() {
        // 快捷方式指向一个真实目录：拖入 .lnk 应导入目录内容，而不是把 .lnk 当文件。
        let target = temp_dir("lnk-target");
        std::fs::create_dir_all(target.join("docs")).expect("mkdir");
        std::fs::write(target.join("docs/a.txt"), "hi").expect("write");
        std::fs::write(target.join("b.txt"), "x").expect("write");

        let lnk = temp_dir("dir-shortcut").with_extension("lnk");
        std::fs::write(&lnk, make_lnk(&target.to_string_lossy())).expect("write lnk");

        let managed = temp_dir("mroot");
        let mut pending = Vec::new();
        let mut failures = Vec::new();
        collect_imports(
            &[lnk.to_string_lossy().to_string()],
            SourceType::External,
            None,
            &managed,
            &[],
            &mut pending,
            &mut failures,
        );

        assert!(failures.is_empty(), "failures: {failures:?}");

        // 生成了目录树：顶层目录 + docs 子目录 + 2 个文件，而不是单个 .lnk 文件。
        let folders: Vec<&PendingImport> = pending
            .iter()
            .filter(|p| p.kind == ResourceKind::Folder)
            .collect();
        assert_eq!(folders.len(), 2, "top folder + docs: {:?}", folders.iter().map(|f| &f.name).collect::<Vec<_>>());
        let files: Vec<&PendingImport> = pending
            .iter()
            .filter(|p| p.kind == ResourceKind::File)
            .collect();
        assert_eq!(files.len(), 2);

        let _ = std::fs::remove_dir_all(&target);
        let _ = std::fs::remove_file(&lnk);
        let _ = std::fs::remove_dir_all(&managed);
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
    fn repairs_gbk_mojibake_path() {
        // 「实习」的 GBK 字节 CA B5 CF B0 被误按 UTF-8 解码后得到 "ʵϰ"。
        let mojibake = "E:\\\u{02b5}\u{03f0}";
        assert_eq!(
            repair_gbk_path(mojibake).as_deref(),
            Some("E:\\\u{5b9e}\u{4e60}"),
            "GBK 乱码应被修复为正确的「实习」路径"
        );

        // 纯 ASCII 路径按 GBK 重解码结果不变，返回 None。
        assert_eq!(repair_gbk_path("C:\\work\\a.txt"), None);
    }

    #[test]
    fn collect_imports_recovers_mojibake_folder_path() {
        // 构造真实中文目录：<temp>\实习moji
        let base = std::env::temp_dir();
        let dir = base.join("实习moji");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("a.txt"), "hi").expect("write");

        // 模拟拖拽乱码：name 部分的 GBK 字节 CA B5 CF B0 被按 UTF-8 解读为 ʵϰ，
        // 于是路径变成 <temp>\ʵϰmoji（该路径不存在，修复后应指向真实目录）。
        let mojibake_name = String::from_utf8(vec![0xCA, 0xB5, 0xCF, 0xB0]).unwrap() + "moji";
        assert_eq!(mojibake_name, "\u{02b5}\u{03f0}moji");
        let mojibake_path = format!("{}\\{}", base.to_string_lossy(), mojibake_name);
        assert!(!Path::new(&mojibake_path).exists(), "mojibake 路径不应存在");

        let managed = temp_dir("mroot");
        let mut pending = Vec::new();
        let mut failures = Vec::new();
        collect_imports(
            &[mojibake_path],
            SourceType::External,
            None,
            &managed,
            &[],
            &mut pending,
            &mut failures,
        );

        assert!(failures.is_empty(), "failures: {failures:?}");
        assert_eq!(
            pending.iter().filter(|p| p.kind == ResourceKind::File).count(),
            1,
            "mojibake 路径应被修复并导入目录内容"
        );
        assert_eq!(
            pending.iter().filter(|p| p.kind == ResourceKind::Folder).count(),
            1,
            "顶层文件夹应被收集"
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&managed);
    }

    #[test]
    fn end_to_end_folder_import_can_be_queried_by_parent() {
        // 完整链路：目录树收集 → 批量入库 → 按父目录查询子项
        let root = temp_dir("e2e");
        std::fs::create_dir_all(root.join("docs")).expect("mkdir");
        std::fs::write(root.join("docs/a.txt"), "hello").expect("write");
        std::fs::write(root.join("b.txt"), "world").expect("write");

        let managed = temp_dir("mroot");
        let mut pending = Vec::new();
        let mut failures = Vec::new();
        collect_tree(&root, &managed, SourceType::External, None, &[], &mut pending, &mut failures);
        assert!(failures.is_empty(), "failures: {failures:?}");

        let mut conn = rusqlite::Connection::open_in_memory().expect("db");
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
        crate::db::migrations::run_migrations(&mut conn).expect("migrations");
        flush_batch(&mut conn, &pending, false).expect("flush");

        // 根目录应看到顶层文件夹
        let roots: Vec<String> = conn
            .prepare("SELECT name FROM resources WHERE parent_id IS NULL ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .expect("roots");
        assert_eq!(roots.len(), 1, "one top folder: {roots:?}");

        // 顶层文件夹下应看到子文件 b.txt
        let top_folder_id: String = conn
            .query_row(
                "SELECT id FROM resources WHERE kind = 'folder' AND parent_id IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("top folder");
        let children: Vec<String> = conn
            .prepare("SELECT name FROM resources WHERE parent_id = ?1 ORDER BY name")
            .unwrap()
            .query_map([&top_folder_id], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .expect("children");
        assert_eq!(children, vec!["b.txt".to_string(), "docs".to_string()], "top folder children");

        // docs 子文件夹下应有 a.txt
        let docs_id: String = conn
            .query_row(
                "SELECT id FROM resources WHERE kind = 'folder' AND name = 'docs'",
                [],
                |r| r.get(0),
            )
            .expect("docs folder");
        let docs_children: Vec<String> = conn
            .prepare("SELECT name FROM resources WHERE parent_id = ?1")
            .unwrap()
            .query_map([&docs_id], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .expect("docs children");
        assert_eq!(docs_children, vec!["a.txt".to_string()], "docs children");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&managed);
    }

    #[test]
    fn flush_batch_inserts_files_and_folders() {
        let mut conn = rusqlite::Connection::open_in_memory().expect("db");
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
        crate::db::migrations::run_migrations(&mut conn).expect("migrations");

        let now = now_unix();
        let batch = vec![
            PendingImport {
                resource_id: "f1".to_string(),
                kind: ResourceKind::Folder,
                name: "folder".to_string(),
                parent_id: None,
                mode: SourceType::External,
                path: "C:\\x\\folder".to_string(),
                canonical: "c:\\x\\folder".to_string(),
                size: 0,
                modified: Some(now),
                extension: None,
                mime: None,
                now,
            },
            PendingImport {
                resource_id: "r1".to_string(),
                kind: ResourceKind::File,
                name: "a.txt".to_string(),
                parent_id: Some("f1".to_string()),
                mode: SourceType::External,
                path: "C:\\x\\folder\\a.txt".to_string(),
                canonical: "c:\\x\\folder\\a.txt".to_string(),
                size: 12,
                modified: Some(now),
                extension: Some("txt".to_string()),
                mime: Some("text/plain".to_string()),
                now,
            },
        ];

        flush_batch(&mut conn, &batch, false).expect("flush");

        let fc: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources WHERE kind = 'folder'", [], |r| r.get(0))
            .expect("count");
        assert_eq!(fc, 1);
        let rc: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
            .expect("count");
        assert_eq!(rc, 2);
        let lc: i64 = conn
            .query_row("SELECT COUNT(*) FROM resource_locations", [], |r| r.get(0))
            .expect("count");
        assert_eq!(lc, 2);
        let mc: i64 = conn
            .query_row("SELECT COUNT(*) FROM file_metadata", [], |r| r.get(0))
            .expect("count");
        assert_eq!(mc, 1, "folder must not insert file_metadata");

        // 父子关系正确
        let parent: Option<String> = conn
            .query_row("SELECT parent_id FROM resources WHERE id = 'r1'", [], |r| r.get(0))
            .expect("parent");
        assert_eq!(parent.as_deref(), Some("f1"));
    }

    #[test]
    fn flush_batch_skips_already_imported_paths() {
        let mut conn = rusqlite::Connection::open_in_memory().expect("db");
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
        crate::db::migrations::run_migrations(&mut conn).expect("migrations");

        let now = now_unix();
        let mk = |id: &str, canonical: &str| PendingImport {
            resource_id: id.to_string(),
            kind: ResourceKind::File,
            name: "a.txt".to_string(),
            parent_id: None,
            mode: SourceType::External,
            path: canonical.to_string(),
            canonical: canonical.to_string(),
            size: 12,
            modified: Some(now),
            extension: Some("txt".to_string()),
            mime: Some("text/plain".to_string()),
            now,
        };

        flush_batch(&mut conn, &[mk("r1", "c:\\x\\a.txt")], false).expect("first ok");
        // 重复导入同一 canonical 路径：应跳过而非触发 UNIQUE 冲突
        flush_batch(&mut conn, &[mk("r2", "c:\\x\\a.txt")], false).expect("duplicate ok");

        let lc: i64 = conn
            .query_row("SELECT COUNT(*) FROM resource_locations", [], |r| r.get(0))
            .expect("count");
        assert_eq!(lc, 1, "duplicate location must be skipped");
        let rc: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
            .expect("count");
        assert_eq!(rc, 1, "duplicate resource must not be created");
    }

    #[test]
    fn flush_batch_reuses_existing_folder_for_new_children() {
        let mut conn = rusqlite::Connection::open_in_memory().expect("db");
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
        crate::db::migrations::run_migrations(&mut conn).expect("migrations");

        let now = now_unix();
        let mk_folder = |id: &str, canonical: &str| PendingImport {
            resource_id: id.to_string(),
            kind: ResourceKind::Folder,
            name: "folder".to_string(),
            parent_id: None,
            mode: SourceType::External,
            path: canonical.to_string(),
            canonical: canonical.to_string(),
            size: 0,
            modified: Some(now),
            extension: None,
            mime: None,
            now,
        };
        let mk_file = |id: &str, parent: Option<&str>, canonical: &str| PendingImport {
            resource_id: id.to_string(),
            kind: ResourceKind::File,
            name: "x.txt".to_string(),
            parent_id: parent.map(|p| p.to_string()),
            mode: SourceType::External,
            path: canonical.to_string(),
            canonical: canonical.to_string(),
            size: 1,
            modified: Some(now),
            extension: Some("txt".to_string()),
            mime: Some("text/plain".to_string()),
            now,
        };

        // 第一次导入：folder + a.txt
        flush_batch(
            &mut conn,
            &[
                mk_folder("f1", "c:\\x\\folder"),
                mk_file("r1", Some("f1"), "c:\\x\\folder\\a.txt"),
            ],
            false,
        )
        .expect("first ok");

        // 第二次导入同一文件夹（新 id f2），含新增文件 new.txt
        flush_batch(
            &mut conn,
            &[
                mk_folder("f2", "c:\\x\\folder"),
                mk_file("r2", Some("f2"), "c:\\x\\folder\\new.txt"),
            ],
            false,
        )
        .expect("second ok");

        // folder 只有一个，新文件挂到原 folder 下，无 FK 错误
        let folder_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources WHERE kind = 'folder'", [], |r| r.get(0))
            .expect("count");
        assert_eq!(folder_count, 1, "folder must not be duplicated");
        let file_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources WHERE kind = 'file'", [], |r| r.get(0))
            .expect("count");
        assert_eq!(file_count, 2);
        let parent: Option<String> = conn
            .query_row("SELECT parent_id FROM resources WHERE id = 'r2'", [], |r| r.get(0))
            .expect("parent");
        assert_eq!(parent.as_deref(), Some("f1"), "new file must attach to existing folder");
    }

    #[test]
    fn flush_batch_recreates_folder_after_it_was_trashed() {
        let mut conn = rusqlite::Connection::open_in_memory().expect("db");
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
        crate::db::migrations::run_migrations(&mut conn).expect("migrations");

        let now = now_unix();
        let mk_folder = |id: &str, canonical: &str| PendingImport {
            resource_id: id.to_string(),
            kind: ResourceKind::Folder,
            name: "dir".to_string(),
            parent_id: None,
            mode: SourceType::External,
            path: canonical.to_string(),
            canonical: canonical.to_string(),
            size: 0,
            modified: Some(now),
            extension: None,
            mime: None,
            now,
        };
        let mk_file = |id: &str, parent: Option<&str>, canonical: &str| PendingImport {
            resource_id: id.to_string(),
            kind: ResourceKind::File,
            name: "a.txt".to_string(),
            parent_id: parent.map(|p| p.to_string()),
            mode: SourceType::External,
            path: canonical.to_string(),
            canonical: canonical.to_string(),
            size: 1,
            modified: Some(now),
            extension: Some("txt".to_string()),
            mime: Some("text/plain".to_string()),
            now,
        };

        // 第一次导入：创建文件夹 dir（根目录，可见）。
        flush_batch(&mut conn, &[mk_folder("d1", "c:\\x\\dir")], false).expect("first ok");

        // 用户删除该文件夹（软删除，location 记录保留）。
        conn.execute(
            "UPDATE resources SET is_deleted = 1, deleted_at = ?1, updated_at = ?1 WHERE id = 'd1'",
            rusqlite::params![now],
        )
        .expect("trash");

        // 重新导入同一路径：必须重建可见文件夹，子文件挂到新文件夹。
        flush_batch(
            &mut conn,
            &[
                mk_folder("d2", "c:\\x\\dir"),
                mk_file("r2", Some("d2"), "c:\\x\\dir\\a.txt"),
            ],
            false,
        )
        .expect("second ok");

        // 根目录能看到重建后的文件夹，而不是挂在已删除的旧文件夹下。
        let visible: Vec<String> = conn
            .prepare("SELECT name FROM resources WHERE parent_id IS NULL AND is_deleted = 0 ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .expect("roots");
        assert_eq!(visible, vec!["dir".to_string()], "recreated folder visible at root");

        let d2_deleted: i64 = conn
            .query_row("SELECT is_deleted FROM resources WHERE id = 'd2'", [], |r| r.get(0))
            .expect("d2");
        assert_eq!(d2_deleted, 0, "new folder must not be trashed");

        let parent: Option<String> = conn
            .query_row("SELECT parent_id FROM resources WHERE id = 'r2'", [], |r| r.get(0))
            .expect("parent");
        assert_eq!(parent.as_deref(), Some("d2"), "file must attach to recreated folder");
    }

    #[test]
    fn flush_batch_rebuilds_subfolder_under_trashed_ancestor() {
        let mut conn = rusqlite::Connection::open_in_memory().expect("db");
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
        crate::db::migrations::run_migrations(&mut conn).expect("migrations");

        let now = now_unix();
        let mk_folder = |id: &str, name: &str, parent: Option<&str>, canonical: &str| PendingImport {
            resource_id: id.to_string(),
            kind: ResourceKind::Folder,
            name: name.to_string(),
            parent_id: parent.map(|p| p.to_string()),
            mode: SourceType::External,
            path: canonical.to_string(),
            canonical: canonical.to_string(),
            size: 0,
            modified: Some(now),
            extension: None,
            mime: None,
            now,
        };
        let mk_file = |id: &str, parent: &str, canonical: &str| PendingImport {
            resource_id: id.to_string(),
            kind: ResourceKind::File,
            name: "a.txt".to_string(),
            parent_id: Some(parent.to_string()),
            mode: SourceType::External,
            path: canonical.to_string(),
            canonical: canonical.to_string(),
            size: 1,
            modified: Some(now),
            extension: Some("txt".to_string()),
            mime: Some("text/plain".to_string()),
            now,
        };

        // 第一次导入：dir(d1) -> sub(s1)。
        flush_batch(
            &mut conn,
            &[
                mk_folder("d1", "dir", None, "c:\\x\\dir"),
                mk_folder("s1", "sub", Some("d1"), "c:\\x\\dir\\sub"),
            ],
            false,
        )
        .expect("first ok");

        // 删除 dir（软删除）。s1 自身未删除，但父链已删除。
        conn.execute(
            "UPDATE resources SET is_deleted = 1, deleted_at = ?1, updated_at = ?1 WHERE id = 'd1'",
            rusqlite::params![now],
        )
        .expect("trash");

        // 重新导入：dir(d2) -> sub(s2) + file，全部应重建为新树。
        flush_batch(
            &mut conn,
            &[
                mk_folder("d2", "dir", None, "c:\\x\\dir"),
                mk_folder("s2", "sub", Some("d2"), "c:\\x\\dir\\sub"),
                mk_file("r2", "s2", "c:\\x\\dir\\sub\\a.txt"),
            ],
            false,
        )
        .expect("second ok");

        // 子目录必须重建并挂到新 dir 下，而不是复用到已删除祖先下的旧 sub。
        let s2_parent: Option<String> = conn
            .query_row("SELECT parent_id FROM resources WHERE id = 's2'", [], |r| r.get(0))
            .expect("s2 parent");
        assert_eq!(s2_parent.as_deref(), Some("d2"), "subfolder must attach to recreated dir");

        // 文件挂到新 sub 下。
        let r2_parent: Option<String> = conn
            .query_row("SELECT parent_id FROM resources WHERE id = 'r2'", [], |r| r.get(0))
            .expect("r2 parent");
        assert_eq!(r2_parent.as_deref(), Some("s2"), "file must attach to recreated sub");

        // 新树整体可见：从根出发能查到 dir -> sub -> a.txt。
        let leaf: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM resources r
                 JOIN resources s ON s.id = r.parent_id
                 JOIN resources d ON d.id = s.parent_id
                 WHERE d.parent_id IS NULL AND d.is_deleted = 0
                   AND s.name = 'sub' AND r.name = 'a.txt'",
                [],
                |r| r.get(0),
            )
            .expect("leaf count");
        assert_eq!(leaf, 1, "a.txt reachable from visible root tree");
    }

    #[test]
    fn flush_batch_rebuilds_file_under_trashed_ancestor() {
        let mut conn = rusqlite::Connection::open_in_memory().expect("db");
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
        crate::db::migrations::run_migrations(&mut conn).expect("migrations");

        let now = now_unix();
        let mk_folder = |id: &str, name: &str, parent: Option<&str>, canonical: &str| PendingImport {
            resource_id: id.to_string(),
            kind: ResourceKind::Folder,
            name: name.to_string(),
            parent_id: parent.map(|p| p.to_string()),
            mode: SourceType::External,
            path: canonical.to_string(),
            canonical: canonical.to_string(),
            size: 0,
            modified: Some(now),
            extension: None,
            mime: None,
            now,
        };
        let mk_file = |id: &str, parent: &str, canonical: &str| PendingImport {
            resource_id: id.to_string(),
            kind: ResourceKind::File,
            name: "a.txt".to_string(),
            parent_id: Some(parent.to_string()),
            mode: SourceType::External,
            path: canonical.to_string(),
            canonical: canonical.to_string(),
            size: 1,
            modified: Some(now),
            extension: Some("txt".to_string()),
            mime: Some("text/plain".to_string()),
            now,
        };

        // 第一次导入：dir(d1) -> sub(s1) -> a.txt(r1)，文件与文件夹都存在。
        flush_batch(
            &mut conn,
            &[
                mk_folder("d1", "dir", None, "c:\\x\\dir"),
                mk_folder("s1", "sub", Some("d1"), "c:\\x\\dir\\sub"),
                mk_file("r1", "s1", "c:\\x\\dir\\sub\\a.txt"),
            ],
            false,
        )
        .expect("first ok");

        // 删除 dir（软删除）：整棵子树对外不可见，但节点自身未标记。
        conn.execute(
            "UPDATE resources SET is_deleted = 1, deleted_at = ?1, updated_at = ?1 WHERE id = 'd1'",
            rusqlite::params![now],
        )
        .expect("trash");

        // 重新导入：dir(d2) -> sub(s2) -> a.txt(r2)。
        // 文件已存在（r1 的 canonical 相同）但祖先已删除：必须重建，不能复用。
        flush_batch(
            &mut conn,
            &[
                mk_folder("d2", "dir", None, "c:\\x\\dir"),
                mk_folder("s2", "sub", Some("d2"), "c:\\x\\dir\\sub"),
                mk_file("r2", "s2", "c:\\x\\dir\\sub\\a.txt"),
            ],
            false,
        )
        .expect("second ok");

        // 文件 r2 必须重建并挂到新 sub 下，而不是被复用跳过。
        let r2_parent: Option<String> = conn
            .query_row("SELECT parent_id FROM resources WHERE id = 'r2'", [], |r| r.get(0))
            .expect("r2 parent");
        assert_eq!(r2_parent.as_deref(), Some("s2"), "file must attach to recreated sub");

        // 新树整体可见：从根出发能查到 dir -> sub -> a.txt。
        let visible: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM resources r
                 JOIN resources s ON s.id = r.parent_id
                 JOIN resources d ON d.id = s.parent_id
                 WHERE d.parent_id IS NULL AND d.is_deleted = 0
                   AND d.id = 'd2' AND s.id = 's2' AND r.id = 'r2'",
                [],
                |r| r.get(0),
            )
            .expect("visible count");
        assert_eq!(visible, 1, "r2 must be reachable from visible root tree");
    }

    #[test]
    fn collect_tree_does_not_follow_symlink_loops() {
        let root = temp_dir("linkloop");
        std::fs::create_dir_all(root.join("real")).expect("mkdir");
        std::fs::write(root.join("real/a.txt"), "x").expect("write");
        let managed = temp_dir("mroot");
        let mut pending = Vec::new();
        let mut failures = Vec::new();

        // 创建指向自身的目录联接（Windows junction / symlink dir），形成循环
        #[cfg(windows)]
        {
            let link = root.join("loop");
            if std::os::windows::fs::symlink_dir(&root, &link).is_ok()
                && std::fs::symlink_metadata(&link).is_ok()
            {
                collect_tree(&root, &managed, SourceType::External, None, &[], &mut pending, &mut failures);
                // 若循环被跟随会无限递归/栈溢出；此处只应收集真实目录与文件
                assert!(pending.len() < 100, "symlink loop must not explode, got {}", pending.len());
            }
        }
        #[cfg(not(windows))]
        {
            collect_tree(&root, &managed, SourceType::External, None, &[], &mut pending, &mut failures);
            assert!(pending.len() < 100, "symlink loop must not explode, got {}", pending.len());
        }

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&managed);
    }
}
