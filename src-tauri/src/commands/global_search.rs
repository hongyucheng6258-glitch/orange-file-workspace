use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::global_search::{dedup_hits, sort_hits, GlobalSearchHit};
use crate::AppState;

/// 首屏返回：来源状态 + 首批命中。
#[derive(serde::Serialize)]
pub struct SearchBatch {
    pub search_id: u64,
    pub hits: Vec<GlobalSearchHit>,
    pub sources: serde_json::Value,
    pub index_incomplete: bool,
}

/// 启动一次全局搜索：同步聚合即时来源，后台持续补充本地索引命中。
#[tauri::command]
pub fn start_global_search(
    app: AppHandle,
    state: State<AppState>,
    query: String,
    limit: Option<i64>,
) -> CommandResult<SearchBatch> {
    let q = query.trim().to_string();
    let lim = limit.unwrap_or(100).clamp(1, 500);
    let search_id = state.search.next_query_id.fetch_add(1, Ordering::SeqCst);
    state
        .search
        .active_queries
        .lock()
        .expect("search lock")
        .insert(search_id, ());

    if q.is_empty() {
        state.search.active_queries.lock().expect("lock").remove(&search_id);
        return Ok(SearchBatch {
            search_id,
            hits: Vec::new(),
            sources: serde_json::json!({ "query": "" }),
            index_incomplete: false,
        });
    }

    let mut hits: Vec<GlobalSearchHit> = Vec::new();
    let mut source_status = serde_json::json!({});

    // 1) 应用索引
    {
        let conn = state.conn.lock().expect("db lock");
        match crate::services::app_index::query_apps(&conn, &q, lim) {
            Ok(mut h) => hits.append(&mut h),
            Err(e) => {
                source_status["app_index"] = serde_json::json!({ "error": e.to_string() });
            }
        }
        // 2) NexusFile 业务资源
        match crate::commands::search::execute_search(&conn, &q, None, false, lim, 0) {
            Ok(nexus) => crate::services::global_search::merge_nexus_hits(&mut hits, nexus),
            Err(e) => {
                source_status["nexus"] = serde_json::json!({ "error": e.to_string() });
            }
        }
        // 3) 本地索引即时部分
        match crate::services::global_search::query_local_index(&conn, &q, lim / 2) {
            Ok(mut h) => hits.append(&mut h),
            Err(e) => {
                source_status["local_index"] = serde_json::json!({ "error": e.to_string() });
            }
        }
    }

    // 4) Windows Search 即时层（可降级）
    #[cfg(windows)]
    match crate::services::windows_search::search(&q, 50) {
        Ok(mut h) => hits.append(&mut h),
        Err(e) => {
            source_status["windows"] = serde_json::json!({ "error": e });
        }
    }

    dedup_hits(&mut hits);
    sort_hits(&mut hits);
    hits.truncate(lim as usize);

    let incomplete = {
        let conn = state.conn.lock().expect("db lock");
        let scanning: i64 = conn
            .query_row(
                "SELECT count(*) FROM system_search_scan_state WHERE status IN ('pending','scanning','paused')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(1);
        scanning > 0
    };

    // 后台补充：若索引未完成，启动持续补批任务
    if incomplete {
        let app2 = app.clone();
        let q2 = q.clone();
        std::thread::spawn(move || {
            // 线程退出（自然结束或出错）时清理活动查询条目，避免 map 累积泄漏。
            let cleanup = || {
                let st = app2.state::<AppState>();
                st.search
                    .active_queries
                    .lock()
                    .expect("search lock")
                    .remove(&search_id);
            };
            let mut seen_keys = std::collections::HashSet::new();
            for _ in 0..40 {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                let st = app2.state::<AppState>();
                let still_active = st
                    .search
                    .active_queries
                    .lock()
                    .expect("lock")
                    .contains_key(&search_id);
                if !still_active {
                    // 已被 cancel_global_search 移除，无需重复清理
                    return;
                }
                let conn = st.conn.lock().expect("db lock");
                match crate::services::global_search::query_local_index(&conn, &q2, 50) {
                    Ok(mut h) => {
                        let new_hits: Vec<GlobalSearchHit> = h
                            .drain(..)
                            .filter(|x| seen_keys.insert(x.key.clone()))
                            .collect();
                        if !new_hits.is_empty() {
                            let _ = app2.emit(
                                crate::events::EVENT_GLOBAL_SEARCH_BATCH,
                                serde_json::json!({
                                    "search_id": search_id,
                                    "hits": new_hits,
                                }),
                            );
                        }
                    }
                    Err(_) => {
                        cleanup();
                        return;
                    }
                }
            }
            cleanup();
        });
    }

    Ok(SearchBatch {
        search_id,
        hits,
        sources: source_status,
        index_incomplete: incomplete,
    })
}

/// 取消一次全局搜索（停止后台补批）。
#[tauri::command]
pub fn cancel_global_search(
    state: State<AppState>,
    search_id: u64,
) -> CommandResult<()> {
    state
        .search
        .active_queries
        .lock()
        .expect("search lock")
        .remove(&search_id);
    Ok(())
}

/// 打开搜索结果（后端校验后执行，不接受前端任意命令）。
#[tauri::command]
pub fn open_search_result(
    state: State<AppState>,
    key: String,
) -> CommandResult<()> {
    let clean_key = key.strip_prefix("app:").unwrap_or(&key);
    // NexusFile 无位置资源（页面/项目）没有本地文件路径，返回独立错误码，避免误导为文件不存在
    if clean_key.starts_with("nexus:") {
        return Err(AppError::new("no_location", "该资源没有本地位置，请在页面或项目中打开"));
    }
    let conn = state.conn.lock().expect("db lock");
    let row = conn
        .query_row(
            "SELECT launch_target, aumid FROM system_search_apps
             WHERE canonical_target = ?1 OR aumid = ?1 COLLATE NOCASE",
            [clean_key],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()?;

    if let Some((target, aumid)) = row {
        return open_target(target, aumid);
    }

    // 文件/文件夹：先按索引记录校验（canonical_path 命中取库中路径），
    // 未命中回退原始 key 再做 exists 检查（兼容 nexus 等未入库的外部路径）
    let mut path = clean_key.to_string();
    if let Some(db_path) = conn
        .query_row(
            "SELECT canonical_path FROM system_search_entries WHERE canonical_path = ?1",
            [clean_key],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        path = db_path;
    }
    if std::path::Path::new(&path).exists() {
        return crate::services::system_windows::shell_open_path(&path)
            .map_err(|e| AppError::new("shell_error", e));
    }
    Err(AppError::new("target_missing", "目标文件已不存在"))
}

/// 打开所在位置（仅文件/文件夹）。
#[tauri::command]
pub fn reveal_search_result(state: State<AppState>, key: String) -> CommandResult<()> {
    let clean_key = key.strip_prefix("app:").unwrap_or(&key);
    let conn = state.conn.lock().expect("db lock");
    let path: Option<String> = conn
        .query_row(
            "SELECT canonical_path FROM system_search_entries WHERE canonical_path = ?1",
            [clean_key],
            |r| r.get(0),
        )
        .optional()?;
    let path = path.unwrap_or_else(|| clean_key.to_string());
    if !std::path::Path::new(&path).exists() {
        return Err(AppError::new("target_missing", "目标文件已不存在"));
    }
    crate::services::system_windows::shell_reveal_path(&path)
        .map_err(|e| AppError::new("shell_error", e))
}

fn open_target(target: Option<String>, aumid: Option<String>) -> CommandResult<()> {
    if let Some(a) = aumid {
        return crate::services::system_windows::shell_open_aumid(&a)
            .map_err(|e| AppError::new("shell_error", e));
    }
    if let Some(t) = target {
        return crate::services::system_windows::shell_open_path(&t)
            .map_err(|e| AppError::new("shell_error", e));
    }
    Err(AppError::new("no_target", "应用缺少启动目标"))
}

use rusqlite::OptionalExtension;

/// 索引状态（供前端状态栏）。
#[derive(serde::Serialize)]
pub struct SearchIndexStatus {
    pub volumes: Vec<VolumeStatus>,
    pub paused: bool,
    pub fts_enabled: bool,
}

#[derive(serde::Serialize)]
pub struct VolumeStatus {
    pub volume_id: String,
    pub root_path: String,
    pub status: String,
    pub indexed_count: i64,
    pub skipped_count: i64,
    pub last_error: Option<String>,
    pub completed_at: Option<i64>,
}

#[tauri::command]
pub fn get_search_index_status(state: State<AppState>) -> CommandResult<SearchIndexStatus> {
    let conn = state.conn.lock().expect("db lock");
    let mut stmt = conn
        .prepare(
            "SELECT volume_id, root_path, status, indexed_count, skipped_count, last_error, completed_at
             FROM system_search_scan_state ORDER BY root_path",
        )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(VolumeStatus {
                volume_id: r.get(0)?,
                root_path: r.get(1)?,
                status: r.get(2)?,
                indexed_count: r.get(3)?,
                skipped_count: r.get(4)?,
                last_error: r.get(5)?,
                completed_at: r.get(6)?,
            })
        })?;
    let mut volumes = Vec::new();
    for r in rows {
        volumes.push(r?);
    }
    let fts_enabled = crate::services::global_search::ensure_fts(&conn);
    Ok(SearchIndexStatus {
        volumes,
        paused: state.search.scan_paused.load(Ordering::SeqCst),
        fts_enabled,
    })
}

#[tauri::command]
pub fn pause_search_index(state: State<AppState>) -> CommandResult<()> {
    state.search.scan_paused.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn resume_search_index(state: State<AppState>) -> CommandResult<()> {
    state.search.scan_paused.store(false, Ordering::SeqCst);
    Ok(())
}

/// 重建索引：所有卷置回 pending、提升代次并清空断点与计数（全量重扫）；
/// 通过提升 scan_trigger 计数让扫描线程立即感知并重扫，无需等待 30s 轮询。
#[tauri::command]
pub fn rebuild_search_index(state: State<AppState>) -> CommandResult<()> {
    let conn = state.conn.lock().expect("db lock");
    for vol in crate::services::scan_service::list_fixed_volumes() {
        let volume_id = format!("{:02x}", crate::services::scan_service::volume_hash(&vol));
        conn.execute(
            "INSERT INTO system_search_scan_state
             (volume_id, root_path, status, scan_generation, indexed_count, skipped_count, started_at)
             VALUES (?1, ?2, 'pending', 1, 0, 0, ?3)
             ON CONFLICT(volume_id) DO UPDATE SET
               status='pending',
               scan_generation=scan_generation+1,
               checkpoint=NULL,
               indexed_count=0,
               skipped_count=0,
               last_error=NULL",
            rusqlite::params![volume_id, vol.to_string_lossy(), crate::db::connection::now_unix()],
        )?;
    }
    // 提升触发计数：扫描线程下一轮立即感知并重扫，无需等待 30s 轮询
    state.search.scan_trigger.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SearchSettings {
    pub excluded_dirs: Vec<String>,
}

#[tauri::command]
pub fn get_search_settings(state: State<AppState>) -> CommandResult<SearchSettings> {
    let conn = state.conn.lock().expect("db lock");
    Ok(SearchSettings {
        excluded_dirs: crate::services::global_search::get_excluded_dirs(&conn).unwrap_or_default(),
    })
}

#[tauri::command]
pub fn update_search_settings(
    state: State<AppState>,
    settings: SearchSettings,
) -> CommandResult<SearchSettings> {
    let conn = state.conn.lock().expect("db lock");
    crate::services::global_search::set_excluded_dirs(&conn, &settings.excluded_dirs)?;
    Ok(settings)
}

#[cfg(test)]
mod perf {
    /// 本地索引查询计时（内存库 + 1000 条样本），验证 LIKE 兜底路径在合理范围。
    #[test]
    fn local_query_timing_with_1000_rows() {
        use crate::services::global_search::query_local_index;
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        let now = crate::db::connection::now_unix();
        {
            let mut stmt = conn
                .prepare(
                    "INSERT INTO system_search_entries
                     (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
                     VALUES (?1, ?2, 'file', 'v9', 1, ?3)",
                )
                .unwrap();
            for i in 0..1000 {
                stmt.execute(rusqlite::params![
                    format!("c:\\perf\\file_{i:04}.txt"),
                    format!("file_{i:04}.txt"),
                    now,
                ])
                .unwrap();
            }
        }
        let start = std::time::Instant::now();
        let hits = query_local_index(&conn, "file_0500", 50).unwrap();
        let elapsed = start.elapsed();
        assert!(!hits.is_empty(), "应命中 file_0500");
        assert!(
            elapsed.as_millis() < 500,
            "LIKE 路径 1000 行应 <500ms，实际 {}ms",
            elapsed.as_millis()
        );
    }
}
