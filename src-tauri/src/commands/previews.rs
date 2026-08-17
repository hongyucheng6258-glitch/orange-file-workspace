use std::path::{Path, PathBuf};

use base64::Engine;
use tauri::State;

use crate::db::connection::now_unix;
use crate::db::repositories as repo;
use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::archive_service;
use crate::services::hash_service;
use crate::services::preview_service;
use crate::services::settings_service;
use crate::services::thumbnail_service;
use crate::AppState;

fn lock_db<'a>(state: &'a AppState) -> std::sync::MutexGuard<'a, rusqlite::Connection> {
    state.conn.lock().expect("db lock poisoned")
}

fn png_data_url(path: &Path) -> CommandResult<String> {
    let bytes = std::fs::read(path)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:image/png;base64,{encoded}"))
}

pub(crate) fn icon_data_url_for_path(
    path: &Path,
    cache_dir: &Path,
) -> CommandResult<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let icon_path = match thumbnail_service::extract_file_icon(path, cache_dir) {
        Ok(icon_path) => icon_path,
        Err(error) => {
            eprintln!(
                "get_path_icon: extract failed for {}: {} ({})",
                path.display(),
                error.message,
                error.code
            );
            return Ok(None);
        }
    };
    png_data_url(&icon_path).map(Some)
}

/// 获取资源的缩略图缓存路径（不存在时按需生成）。
/// 非图片或路径失效返回 None。
#[tauri::command]
pub fn get_thumbnail(state: State<AppState>, resource_id: String) -> CommandResult<Option<String>> {
    let conn = lock_db(&state);
    let locations = repo::list_locations(&conn, &resource_id)?;
    let Some(loc) = locations.first() else {
        return Ok(None);
    };

    let path = PathBuf::from(&loc.path);
    if !thumbnail_service::is_image(&path) || !path.exists() {
        return Ok(None);
    }

    // 命中缓存
    if let Some(cached) = repo::get_thumbnail_path(&conn, &resource_id)? {
        if PathBuf::from(&cached).exists() {
            return Ok(Some(cached));
        }
    }

    // 生成缩略图
    let cache_dir = state.data_dir.lock().expect("dir lock").join("thumbnails");
    std::fs::create_dir_all(&cache_dir)?;
    let (dest, w, h) = thumbnail_service::generate_thumbnail(&path, &cache_dir)?;
    let dest_str = dest.to_string_lossy().to_string();
    repo::upsert_thumbnail(
        &conn,
        &resource_id,
        &dest_str,
        w as i64,
        h as i64,
        loc.content_hash.as_deref(),
        now_unix(),
    )?;
    Ok(Some(dest_str))
}

/// 提取文件（可执行文件/快捷方式等）的应用图标，返回缓存 PNG 路径。
/// 无法提取时返回 None。
#[tauri::command]
pub fn get_file_icon(state: State<AppState>, resource_id: String) -> CommandResult<Option<String>> {
    let conn = lock_db(&state);
    let locations = repo::list_locations(&conn, &resource_id)?;
    let Some(loc) = locations.first() else {
        return Ok(None);
    };
    let path = PathBuf::from(&loc.path);
    if !path.exists() {
        return Ok(None);
    }

    // 命中缓存
    if let Some(cached) = repo::get_thumbnail_path(&conn, &resource_id)? {
        let cached_path = PathBuf::from(&cached);
        if cached_path.exists() {
            return png_data_url(&cached_path).map(Some);
        }
    }

    let cache_dir = state.data_dir.lock().expect("dir lock").join("thumbnails");
    let dest = match thumbnail_service::extract_file_icon(&path, &cache_dir) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("get_file_icon: extract failed for {}: {} ({})", path.display(), e.message, e.code);
            return Ok(None);
        }
    };
    let dest_str = dest.to_string_lossy().to_string();
    let _ = repo::upsert_thumbnail(
        &conn,
        &resource_id,
        &dest_str,
        0,
        0,
        loc.content_hash.as_deref(),
        now_unix(),
    );
    png_data_url(&dest).map(Some)
}

/// 按物理路径提取系统文件图标，用于不属于资源库的全局搜索结果。
/// 路径失效时按显示名称回退到应用索引中的有效启动目标。
#[tauri::command]
pub fn get_path_icon(
    state: State<AppState>,
    path: String,
    name: Option<String>,
) -> CommandResult<Option<String>> {
    let cache_dir = state.data_dir.lock().expect("dir lock").join("thumbnails");
    let requested_path = PathBuf::from(&path);
    if requested_path.is_file() {
        return icon_data_url_for_path(&requested_path, &cache_dir);
    }

    let Some(display_name) = name
        .as_deref()
        .and_then(|value| Path::new(value).file_stem())
        .map(|value| value.to_string_lossy().to_string())
    else {
        return Ok(None);
    };

    let conn = lock_db(&state);
    let mut stmt = conn.prepare(
        "SELECT launch_target, icon_source FROM system_search_apps
         WHERE display_name = ?1 COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([display_name], |row| {
        Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    for row in rows {
        let (launch_target, icon_source) = row?;
        for candidate in [launch_target, icon_source].into_iter().flatten() {
            let candidate_path = PathBuf::from(candidate);
            if candidate_path.is_file() {
                return icon_data_url_for_path(&candidate_path, &cache_dir);
            }
        }
    }
    Ok(None)
}

/// 读取文本文件的预览内容（上限由设置决定）。
#[tauri::command]
pub fn get_text_preview(state: State<AppState>, resource_id: String) -> CommandResult<String> {
    let conn = lock_db(&state);
    let locations = repo::list_locations(&conn, &resource_id)?;
    let Some(loc) = locations.first() else {
        return Err(AppError::new("location_missing", "资源缺少物理位置"));
    };
    let path = PathBuf::from(&loc.path);
    if !path.exists() {
        return Err(AppError::new("path_missing", "文件路径不可用"));
    }
    let limit_kb = settings_service::load_settings(&conn)
        .map(|s| s.general.preview_size_limit_mb.saturating_mul(1024) as usize)
        .unwrap_or(256 * 1024);
    preview_service::read_text_preview(&path, limit_kb)
}

/// 为指定资源计算并保存内容哈希。
#[tauri::command]
pub fn hash_resources(state: State<AppState>, ids: Vec<String>) -> CommandResult<usize> {
    let conn = lock_db(&state);
    let mut hashed = 0;
    for id in &ids {
        let locations = repo::list_locations(&conn, id)?;
        if let Some(loc) = locations.first() {
            let path = PathBuf::from(&loc.path);
            if path.exists() && path.is_file() {
                if let Ok(h) = hash_service::sha256_file(&path) {
                    repo::update_location_hash(&conn, &loc.id, &h, "sha256")?;
                    hashed += 1;
                }
            }
        }
    }
    Ok(hashed)
}

/// 获取资源的预览类型（preview_kind），用于前端选择渲染器。
/// 如果数据库中未存储 preview_kind，则根据扩展名和 MIME 推断并回写。
#[tauri::command]
pub fn get_preview_kind(state: State<AppState>, resource_id: String) -> CommandResult<Option<String>> {
    let conn = lock_db(&state);
    // 从 file_metadata 读取 preview_kind + extension + mime_type
    let row: Option<(Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT preview_kind, extension, mime_type FROM file_metadata WHERE resource_id = ?1",
            rusqlite::params![&resource_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();
    let Some((stored, extension, mime_type)) = row else {
        return Ok(None);
    };
    if let Some(kind) = stored {
        return Ok(Some(kind));
    }
    // 推断
    let kind = preview_service::detect_preview_kind(extension.as_deref(), mime_type.as_deref());
    // 回写
    if let Some(ref k) = kind {
        let _ = conn.execute(
            "UPDATE file_metadata SET preview_kind = ?2 WHERE resource_id = ?1",
            rusqlite::params![&resource_id, k],
        );
    }
    Ok(kind)
}

/// 获取资源的物理文件路径，用于前端通过 asset 协议加载。
#[tauri::command]
pub fn get_resource_path(state: State<AppState>, resource_id: String) -> CommandResult<Option<String>> {
    let conn = lock_db(&state);
    let locs = repo::list_locations(&conn, &resource_id)?;
    let Some(loc) = locs.first() else {
        return Ok(None);
    };
    let path = PathBuf::from(&loc.path);
    if path.exists() {
        Ok(Some(loc.path.clone()))
    } else {
        Ok(None)
    }
}

/// 获取 CSV 预览数据。
#[tauri::command]
pub fn get_csv_preview(state: State<AppState>, resource_id: String) -> CommandResult<preview_service::CsvPreview> {
    let conn = lock_db(&state);
    let locs = repo::list_locations(&conn, &resource_id)?;
    let Some(loc) = locs.first() else {
        return Err(AppError::new("location_missing", "资源缺少物理位置"));
    };
    let path = PathBuf::from(&loc.path);
    if !path.exists() {
        return Err(AppError::new("path_missing", "文件路径不可用"));
    }
    preview_service::read_csv_preview(&path, 500)
}

/// 列出压缩包内容。
#[tauri::command]
pub fn get_archive_listing(
    state: State<AppState>,
    resource_id: String,
    max_entries: Option<usize>,
) -> CommandResult<archive_service::ArchiveInfo> {
    let conn = lock_db(&state);
    let locs = repo::list_locations(&conn, &resource_id)?;
    let Some(loc) = locs.first() else {
        return Err(AppError::new("location_missing", "资源缺少物理位置"));
    };
    let path = PathBuf::from(&loc.path);
    if !path.exists() {
        return Err(AppError::new("path_missing", "文件路径不可用"));
    }
    archive_service::list_archive(&path, max_entries.unwrap_or(1000))
}
