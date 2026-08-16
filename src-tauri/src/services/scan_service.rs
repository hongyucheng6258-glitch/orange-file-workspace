//! 系统搜索：卷扫描状态仓库。
//!
//! 提供 `system_search_scan_state` 表的读写封装（Task 8），以及本机固定磁盘
//! 枚举。Task 9 的扫描工作线程通过 `update_volume_progress` / `set_volume_status`
//! 汇报进度，Task 10 的状态查询命令通过 `get_volume_state` 读取。

use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// 单卷扫描状态。
#[derive(Debug, Clone)]
pub struct VolumeState {
    pub volume_id: String,
    pub root_path: String,
    pub status: String,
    pub scan_generation: i64,
    pub checkpoint: Option<String>,
    pub indexed_count: i64,
    pub skipped_count: i64,
    pub last_error: Option<String>,
    pub completed_at: Option<i64>,
}

/// 新建或重置某卷的扫描状态。
///
/// 首次插入时计数清零、`started_at` 取当前时间；重复调用（同一卷开始新一轮扫描）
/// 时更新 root_path / status / scan_generation，并整体重置计数、断点与完成时间
/// （清除上一轮的 completed_at / checkpoint / counts），保留首次 started_at。
/// 断点恢复职责由调用方承担：先 `get_volume_state` 读旧 checkpoint，再自行决定续扫。
pub fn upsert_volume_state(
    conn: &mut Connection,
    volume_id: &str,
    root_path: &str,
    status: &str,
    scan_generation: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO system_search_scan_state
         (volume_id, root_path, status, scan_generation, indexed_count, skipped_count, started_at)
         VALUES (?1, ?2, ?3, ?4, 0, 0, ?5)
         ON CONFLICT(volume_id) DO UPDATE SET
           root_path=excluded.root_path,
           status=excluded.status,
           scan_generation=excluded.scan_generation,
           indexed_count=0,
           skipped_count=0,
           checkpoint=NULL,
           completed_at=NULL,
           last_error=NULL,
           started_at=COALESCE(system_search_scan_state.started_at, excluded.started_at)",
        params![volume_id, root_path, status, scan_generation, crate::db::connection::now_unix()],
    )
    .map(|_| ())
}

/// 读取某卷的扫描状态；不存在时返回 `None`。
pub fn get_volume_state(conn: &Connection, volume_id: &str) -> rusqlite::Result<Option<VolumeState>> {
    conn.query_row(
        "SELECT volume_id, root_path, status, scan_generation, checkpoint, indexed_count, skipped_count, last_error, completed_at
         FROM system_search_scan_state WHERE volume_id = ?1",
        [volume_id],
        |r| {
            Ok(VolumeState {
                volume_id: r.get(0)?,
                root_path: r.get(1)?,
                status: r.get(2)?,
                scan_generation: r.get(3)?,
                checkpoint: r.get(4)?,
                indexed_count: r.get(5)?,
                skipped_count: r.get(6)?,
                last_error: r.get(7)?,
                completed_at: r.get(8)?,
            })
        },
    )
    .optional()
}

/// 汇报扫描进度：更新断点路径与已索引/跳过计数。
///
/// `indexed_count` / `skipped_count` 为**绝对计数**（非增量）：续扫时调用方应先读旧值累加。
/// checkpoint 传 None 时保持原值。
pub fn update_volume_progress(
    conn: &mut Connection,
    volume_id: &str,
    checkpoint: Option<&str>,
    indexed_count: i64,
    skipped_count: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE system_search_scan_state
         SET checkpoint = COALESCE(?1, checkpoint), indexed_count = ?2, skipped_count = ?3
         WHERE volume_id = ?4",
        params![checkpoint, indexed_count, skipped_count, volume_id],
    )
    .map(|_| ())
}

/// 更新卷状态；进入终态（completed / error）时写入 completed_at。
pub fn set_volume_status(
    conn: &mut Connection,
    volume_id: &str,
    status: &str,
    last_error: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE system_search_scan_state
         SET status = ?1, last_error = ?2,
             completed_at = CASE WHEN ?1 IN ('completed','error') THEN ?3 ELSE completed_at END
         WHERE volume_id = ?4",
        params![status, last_error, crate::db::connection::now_unix(), volume_id],
    )
    .map(|_| ())
}

/// 枚举本地固定磁盘（Windows: GetLogicalDrives + 驱动器类型）。
#[cfg(windows)]
pub fn list_fixed_volumes() -> Vec<PathBuf> {
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    use windows::Win32::System::WindowsProgramming::DRIVE_FIXED;
    unsafe {
        let mask = GetLogicalDrives();
        let mut out = Vec::new();
        for i in 0..26u8 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i) as char;
            let root = format!("{letter}:\\");
            let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let ty = GetDriveTypeW(windows::core::PCWSTR(wide.as_ptr()));
            if ty == DRIVE_FIXED {
                out.push(PathBuf::from(&root));
            }
        }
        out
    }
}

#[cfg(not(windows))]
pub fn list_fixed_volumes() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}

/// 扫描控制：暂停标志与是否继续（Arc 便于工作线程与命令层共享）。
#[derive(Default, Clone)]
pub struct ScanControl {
    pub paused: Arc<AtomicBool>,
    pub cancelled: Arc<AtomicBool>,
}

/// 遍历目录并写入索引（批量写入，支持暂停/取消）。返回 (indexed, skipped)。
pub fn scan_directory(
    conn: &mut Connection,
    root: &std::path::Path,
    volume_id: &str,
    generation: i64,
    excluded: &[String],
    control: &ScanControl,
) -> Result<(i64, i64), String> {
    use std::collections::VecDeque;

    let mut queue: VecDeque<std::path::PathBuf> = VecDeque::new();
    queue.push_back(root.to_path_buf());
    let mut indexed: i64 = 0;
    let mut skipped: i64 = 0;
    let mut batch: Vec<(String, String, String, Option<String>, Option<i64>, Option<i64>)> = Vec::new();
    let now = crate::db::connection::now_unix();
    // 最近处理的目录：每批 flush 时作为断点写入（含最后一批，保证小目录也可续扫）
    // 注意：checkpoint 只用于进度展示；恢复续扫必须从卷根重新开始（见 scan_start），
    // 否则未持久化的兄弟目录会永久丢失。
    let mut last_checkpoint: Option<String> = None;

    // 暂停检查放在出队之前：暂停期间绝不能弹出并丢弃目录（否则恢复后队列已空、卷被误判完成）
    loop {
        if control.paused.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(200));
            continue;
        }
        if control.cancelled.load(Ordering::Relaxed) {
            return Ok((indexed, skipped));
        }
        let Some(dir) = queue.pop_front() else { break };
        last_checkpoint = Some(dir.to_string_lossy().to_string());
        let Ok(entries) = std::fs::read_dir(&dir) else {
            skipped += 1;
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            // 不跟随符号链接与目录联接（防循环）
            if file_type.is_symlink() {
                skipped += 1;
                continue;
            }
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let canon = crate::services::global_search::canonical_key(&p.to_string_lossy());
            if excluded.iter().any(|e| path_under(&p, std::path::Path::new(e))) {
                skipped += 1;
                continue;
            }
            if file_type.is_dir() {
                batch.push((canon, name.clone(), "folder".into(), None, None, None));
                queue.push_back(p);
            } else if file_type.is_file() {
                let ext = p.extension().map(|e| e.to_string_lossy().to_string());
                let meta = entry.metadata().ok();
                let size = meta.as_ref().and_then(|m| m.len().try_into().ok());
                let modified = meta.as_ref().and_then(|m| {
                    m.modified().ok().and_then(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_secs() as i64)
                    })
                });
                batch.push((canon, name, "file".into(), ext, size, modified));
            }
            if batch.len() >= 2000 {
                flush_batch(conn, &batch, volume_id, generation, now)
                    .map_err(|e| e.to_string())?;
                indexed += batch.len() as i64;
                // 定期写入断点（绝对计数），供中断后从当前目录续扫
                let _ = update_volume_progress(conn, volume_id, last_checkpoint.as_deref(), indexed, skipped);
                batch.clear();
            }
        }
    }
    if !batch.is_empty() {
        flush_batch(conn, &batch, volume_id, generation, now)
            .map_err(|e| e.to_string())?;
        indexed += batch.len() as i64;
        let _ = update_volume_progress(conn, volume_id, last_checkpoint.as_deref(), indexed, skipped);
    }
    Ok((indexed, skipped))
}

/// 判断路径是否位于排除目录下（大小写无关，按路径组件比较，避免 `C:\Windows` 误伤 `C:\Windows2`）。
fn path_under(p: &std::path::Path, excluded: &std::path::Path) -> bool {
    let p_l = p.to_string_lossy().to_lowercase();
    let e_l = excluded.to_string_lossy().to_lowercase();
    let e_trimmed = e_l.trim_end_matches(['/', '\\']);
    if e_trimmed.is_empty() {
        return false;
    }
    let p_parts: Vec<&str> = p_l.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let e_parts: Vec<&str> = e_trimmed.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let is_absolute = e_l.contains(':') || e_l.starts_with('/') || e_l.starts_with('\\');
    if is_absolute {
        // 绝对排除路径（含盘符）：组件前缀匹配（如 C:\Windows 只匹配 c:\windows\... 不误伤 c:\windows2）
        p_parts.starts_with(&e_parts)
    } else if e_parts.len() == 1 {
        // 相对目录名（如 $recycle.bin）：匹配任意盘的任意层级（skip 1 跳过盘符组件）
        p_parts.iter().skip(1).any(|s| *s == e_parts[0])
    } else {
        // 多段相对名：任意位置窗口匹配
        p_parts.windows(e_parts.len()).any(|w| w == e_parts)
    }
}

/// 批量写入索引条目并同步 FTS 外部内容表（代次感知 upsert）。
///
/// 重建索引（rebuild_search_index 对每卷 scan_generation+1 后重扫全卷）时，
/// 同一 canonical_path 已存在旧代次行：若沿用 INSERT OR IGNORE，冲突行被静默跳过、
/// 保持旧代次，扫描完成时清理旧代次的 DELETE 会把整卷索引清空（Critical 缺陷）。
/// 因此改为 ON CONFLICT(canonical_path) DO UPDATE，把冲突行原地提升到新代次。
///
/// 注意 id 获取：upsert 的 DO UPDATE 冲突更新后 `changes()` 仍返回 1（与真正插入
/// 无法区分），且 `last_insert_rowid()` 保持旧值（不指向本行），所以不能走
/// `changed>0 -> last_insert_rowid()` 的捷径；统一按 canonical_path 回查 id
/// （canonical_path 有 UNIQUE COLLATE NOCASE 索引，回查开销可忽略）。
///
/// 批次整体包一个事务：逐条 upsert + FTS 同步后一次提交，避免逐行隐式事务。
fn flush_batch(
    conn: &mut Connection,
    batch: &[(String, String, String, Option<String>, Option<i64>, Option<i64>)],
    volume_id: &str,
    generation: i64,
    now: i64,
) -> rusqlite::Result<()> {
    let insert_sql = "INSERT INTO system_search_entries
        (canonical_path, display_name, entry_kind, extension, file_size, modified_at,
         volume_id, scan_generation, is_offline, indexed_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)
        ON CONFLICT(canonical_path) DO UPDATE SET
          display_name=excluded.display_name,
          entry_kind=excluded.entry_kind,
          extension=excluded.extension,
          file_size=excluded.file_size,
          modified_at=excluded.modified_at,
          scan_generation=excluded.scan_generation,
          is_offline=0,
          indexed_at=excluded.indexed_at";
    let tx = conn.transaction()?;
    for (canon, name, kind, ext, size, modified) in batch {
        tx.execute(
            insert_sql,
            rusqlite::params![canon, name, kind, ext, size, modified, volume_id, generation, now],
        )?;
        // DO UPDATE 冲突更新后 last_insert_rowid() 不可靠（保持旧值），统一按路径回查
        let id = tx
            .query_row(
                "SELECT id FROM system_search_entries WHERE canonical_path = ?1",
                [canon],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if id > 0 {
            // 同步 FTS 外部内容表（名称/路径可搜索）
            crate::services::global_search::fts_sync_upsert(&tx, id, name, canon);
        }
    }
    tx.commit()
}

/// 启动后台扫描线程：为每个固定磁盘创建/恢复扫描任务并持续处理。
pub fn start_scan_worker(app: AppHandle) {
    std::thread::spawn(move || {
        // 上次感知到的重建触发计数；rebuild_search_index 提升后立即重扫，跳过 30s 轮询等待
        let mut last_trigger = 0u64;
        loop {
            let trigger_now = {
                let st = app.state::<crate::AppState>();
                st.search.scan_trigger.load(Ordering::SeqCst)
            };
            {
                let st = app.state::<crate::AppState>();
                if st.search.scan_paused.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    continue;
                }
                // 空闲周期先做分卷校验（离线判定 / 重新上线 / 陈旧记录清理）
                run_verification_round(&app);
                let volumes = list_fixed_volumes();
                for vol in &volumes {
                    scan_one_volume(&app, vol);
                }
            }
            if trigger_now == last_trigger {
                std::thread::sleep(std::time::Duration::from_secs(30));
                continue;
            }
            // 触发过重建：更新已见值并立即进入下一轮（本轮刚完成扫描）
            last_trigger = trigger_now;
        }
    });
}

/// 决定扫描起点：checkpoint 只用于进度展示，不能作为恢复起点。
/// 单个 checkpoint 无法表达完整待扫描队列，从中续扫会漏掉兄弟目录；
/// 中断/重启后一律从卷根重扫，依赖同代次 upsert 幂等保证不丢不重。
fn scan_start(_checkpoint: Option<&str>, root: &std::path::Path) -> std::path::PathBuf {
    root.to_path_buf()
}

fn scan_one_volume(app: &AppHandle, vol: &std::path::Path) {
    let volume_id = format!("{:02x}", volume_hash(vol));
    let db_path = app
        .state::<crate::AppState>()
        .data_dir
        .lock()
        .expect("data dir lock")
        .clone()
        .join("workspace.db");
    let mut conn = match crate::db::connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let (generation, checkpoint, status) = match get_volume_state(&conn, &volume_id) {
        Ok(Some(s)) => (s.scan_generation, s.checkpoint, s.status),
        Ok(None) => {
            let _ = upsert_volume_state(&mut conn, &volume_id, &vol.to_string_lossy(), "pending", 1);
            (1, None, "pending".to_string())
        }
        Err(_) => return,
    };
    if status == "completed" {
        return;
    }
    let _ = set_volume_status(&mut conn, &volume_id, "scanning", None);
    drop(conn);

    let excluded = load_excluded_dirs(&db_path);
    let mut index_conn = match crate::db::connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let control = app.state::<crate::AppState>().scan_control();
    // 恢复起点始终为卷根（checkpoint 仅用于进度展示，见 scan_start 注释）
    let start = scan_start(checkpoint.as_deref(), vol);
    let (indexed, skipped) = match scan_directory(
        &mut index_conn,
        &start,
        &volume_id,
        generation,
        &excluded,
        &control,
    ) {
        Ok(r) => r,
        Err(e) => {
            let _ = set_volume_status(&mut index_conn, &volume_id, "error", Some(&e));
            emit_progress(app, &volume_id, "error", 0, 0, Some(&e));
            return;
        }
    };
    // 被取消：保留已索引数据并置回 paused，供下次启动续扫，绝不能标记 completed
    if control.cancelled.load(Ordering::Relaxed) {
        let _ = set_volume_status(&mut index_conn, &volume_id, "paused", None);
        emit_progress(app, &volume_id, "paused", indexed, skipped, None);
        return;
    }
    // 完成：清理旧代次并标记完成
    index_conn
        .execute(
            "DELETE FROM system_search_entries
             WHERE volume_id = ?1 AND scan_generation < ?2",
            rusqlite::params![volume_id, generation],
        )
        .ok();
    let _ = set_volume_status(&mut index_conn, &volume_id, "completed", None);
    let _ = update_volume_progress(&mut index_conn, &volume_id, None, indexed, skipped);
    emit_progress(app, &volume_id, "completed", indexed, skipped, None);
}

/// 标记卷离线：保留记录并置灰（不删除），scan_state 置 offline。
pub fn mark_volume_offline(conn: &mut Connection, volume_id: &str) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE system_search_entries SET is_offline=1 WHERE volume_id=?1",
        [volume_id],
    )?;
    tx.execute(
        "UPDATE system_search_scan_state SET status='offline' WHERE volume_id=?1",
        [volume_id],
    )?;
    tx.commit()
}

/// 卷重新上线：清理离线条目（含 FTS 残留）并置回 pending 待重扫。
pub fn mark_volume_back_online(conn: &mut Connection, volume_id: &str) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    // 先收集离线条目 id，删除主记录后同步清理 FTS 外部内容表残留
    let ids: Vec<i64> = tx
        .prepare("SELECT id FROM system_search_entries WHERE volume_id=?1 AND is_offline=1")?
        .query_map([volume_id], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    tx.execute(
        "DELETE FROM system_search_entries WHERE volume_id=?1 AND is_offline=1",
        [volume_id],
    )?;
    for id in ids {
        crate::services::global_search::fts_sync_delete(&tx, id);
    }
    tx.execute(
        "UPDATE system_search_scan_state SET status='pending' WHERE volume_id=?1",
        [volume_id],
    )?;
    tx.commit()
}

/// 对已完成卷做低频一致性校验：删除磁盘上已不存在的记录（限量）。
pub fn verify_volume(
    conn: &mut Connection,
    volume_id: &str,
    limit: i64,
) -> rusqlite::Result<i64> {
    let mut stmt = conn.prepare(
        "SELECT id, canonical_path FROM system_search_entries
         WHERE volume_id = ?1 AND is_offline = 0 LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![volume_id, limit], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();
    drop(stmt);
    let mut removed = 0;
    for (id, path) in rows {
        if !std::path::Path::new(&path).exists() {
            conn.execute("DELETE FROM system_search_entries WHERE id=?1", [id])?;
            conn.execute("DELETE FROM system_search_entries_fts WHERE rowid=?1", [id]).ok();
            removed += 1;
        }
    }
    Ok(removed)
}

/// 在扫描线程空闲周期调用：每卷先判离线（根路径不存在→mark_volume_offline），
/// 重新在线（offline 但根存在→mark_volume_back_online），再限量校验陈旧记录。
pub fn run_verification_round(app: &AppHandle) {
    let st = app.state::<crate::AppState>();
    let db_path = st
        .data_dir
        .lock()
        .expect("data dir lock")
        .clone()
        .join("workspace.db");
    let Ok(mut conn) = crate::db::connection::open(&db_path) else { return };
    let mut volumes: Vec<(String, String, String)> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT volume_id, root_path, status FROM system_search_scan_state",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))) {
            volumes = rows.filter_map(|r| r.ok()).collect();
        }
    }
    for (vol, root, status) in volumes {
        let root_exists = std::path::Path::new(&root).exists();
        if !root_exists && status != "offline" {
            let _ = mark_volume_offline(&mut conn, &vol);
            continue;
        }
        if root_exists && status == "offline" {
            let _ = mark_volume_back_online(&mut conn, &vol);
        }
        if status == "completed" {
            let _ = verify_volume(&mut conn, &vol, 2000);
        }
    }
}

/// 卷标识：由根路径哈希生成（大小写无关）。
pub fn volume_hash(vol: &std::path::Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    vol.to_string_lossy().to_lowercase().hash(&mut h);
    h.finish()
}

fn load_excluded_dirs(db_path: &std::path::Path) -> Vec<String> {
    let conn = match crate::db::connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    crate::services::global_search::get_excluded_dirs(&conn).unwrap_or_default()
}

fn emit_progress(
    app: &AppHandle,
    volume_id: &str,
    status: &str,
    indexed: i64,
    skipped: i64,
    last_error: Option<&str>,
) {
    let _ = app.emit(
        crate::events::EVENT_GLOBAL_SEARCH_INDEX_PROGRESS,
        serde_json::json!({
            "volume_id": volume_id,
            "status": status,
            "indexed_count": indexed,
            "skipped_count": skipped,
            "last_error": last_error,
        }),
    );
}

/// 文件变化事件（由 watcher 回调归一化后传入）。
#[derive(Debug, Clone)]
pub enum WatchEvent {
    Create(String),
    Remove(String),
    /// 首版简化：normalize_notify_event 暂不产生该变体（Rename To 按 Create 处理），
    /// 旧路径残留由 Task 13 分卷校验清理；测试已覆盖其应用逻辑。
    #[allow(dead_code)]
    Rename(String, String), // from, to
}

/// 将事件批量应用到索引（单事务）。Create 需文件存在才写；Remove 清理条目并同步 FTS 删除；
/// Rename 更新路径，目标为新文件则插入。被忽略的路径（INSERT OR IGNORE 命中）同步 FTS。
pub fn apply_watch_events(
    conn: &mut Connection,
    volume_id: &str,
    generation: i64,
    events: &[WatchEvent],
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    for ev in events {
        match ev {
            WatchEvent::Create(path) => {
                if std::path::Path::new(path).is_file() {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if !name.is_empty() {
                        let canon = crate::services::global_search::canonical_key(path);
                        let now = crate::db::connection::now_unix();
                        let changed = tx.execute(
                            "INSERT OR IGNORE INTO system_search_entries
                             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
                             VALUES (?1, ?2, 'file', ?3, ?4, ?5)",
                            rusqlite::params![canon, name, volume_id, generation, now],
                        )?;
                        let id = if changed > 0 {
                            tx.last_insert_rowid()
                        } else {
                            tx.query_row(
                                "SELECT id FROM system_search_entries WHERE canonical_path=?1",
                                [&canon],
                                |r| r.get(0),
                            )?
                        };
                        crate::services::global_search::fts_sync_upsert(&tx, id, &name, &canon);
                    }
                }
            }
            WatchEvent::Remove(path) => {
                let canon = crate::services::global_search::canonical_key(path);
                let id: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM system_search_entries WHERE canonical_path = ?1",
                        [&canon],
                        |r| r.get(0),
                    )
                    .optional()?;
                tx.execute(
                    "DELETE FROM system_search_entries WHERE canonical_path = ?1",
                    [&canon],
                )?;
                if let Some(id) = id {
                    crate::services::global_search::fts_sync_delete(&tx, id);
                }
            }
            WatchEvent::Rename(from, to) => {
                let f = crate::services::global_search::canonical_key(from);
                let t = crate::services::global_search::canonical_key(to);
                let exists: Option<String> = tx
                    .query_row(
                        "SELECT display_name FROM system_search_entries WHERE canonical_path = ?1",
                        [&f],
                        |r| r.get(0),
                    )
                    .optional()?;
                match exists {
                    Some(_old_name) => {
                        // 重命名后用新文件名更新 display_name（与磁盘实际一致）
                        let new_name = std::path::Path::new(to)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let id: i64 = tx
                            .query_row(
                                "SELECT id FROM system_search_entries WHERE canonical_path = ?1",
                                [&f],
                                |r| r.get(0),
                            )?;
                        tx.execute(
                            "UPDATE system_search_entries
                             SET canonical_path = ?1, display_name = ?2 WHERE canonical_path = ?3",
                            rusqlite::params![t, new_name, f],
                        )?;
                        crate::services::global_search::fts_sync_upsert(&tx, id, &new_name, &t);
                    }
                    None => {
                        if std::path::Path::new(to).is_file() {
                            let name = std::path::Path::new(to)
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let now = crate::db::connection::now_unix();
                            let changed = tx.execute(
                                "INSERT OR IGNORE INTO system_search_entries
                                 (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
                                 VALUES (?1, ?2, 'file', ?3, ?4, ?5)",
                                rusqlite::params![t, name, volume_id, generation, now],
                            )?;
                            let id = if changed > 0 {
                                tx.last_insert_rowid()
                            } else {
                                tx.query_row(
                                    "SELECT id FROM system_search_entries WHERE canonical_path=?1",
                                    [&t],
                                    |r| r.get(0),
                                )?
                            };
                            crate::services::global_search::fts_sync_upsert(&tx, id, &name, &t);
                        }
                    }
                }
            }
        }
    }
    tx.commit()
}

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// 为所有已完成卷启动文件监听；事件经 2s 去抖后批量应用。
/// 首版简化：Rename 的 from/to 在 notify 中分属两个事件，To 按 Create 处理，
/// from 遗留旧路径由分卷校验（Task 13）清理。
pub fn start_watchers(app: AppHandle) {
    std::thread::spawn(move || {
        let mut watchers: Vec<RecommendedWatcher> = Vec::new();
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        let mut roots: Vec<(String, PathBuf)> = Vec::new();
        loop {
            // 每次轮询刷新已完成卷的监听集合
            {
                let st = app.state::<crate::AppState>();
                let db_path = st
                    .data_dir
                    .lock()
                    .expect("data dir lock")
                    .clone()
                    .join("workspace.db");
                let Ok(conn) = crate::db::connection::open(&db_path) else { continue };
                let mut completed: Vec<(String, String)> = Vec::new();
                if let Ok(mut stmt) = conn.prepare(
                    "SELECT volume_id, root_path FROM system_search_scan_state WHERE status='completed'",
                ) {
                    if let Ok(rows) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))) {
                        for row in rows.flatten() {
                            completed.push(row);
                        }
                    }
                }
                drop(conn);
                let current: Vec<(String, PathBuf)> = completed
                    .into_iter()
                    .map(|(v, p)| (v, PathBuf::from(p)))
                    .collect();
                if current != roots {
                    roots = current;
                    watchers.clear();
                    for (_v, root) in &roots {
                        let tx_clone = tx.clone();
                        if let Ok(mut w) = RecommendedWatcher::new(
                            move |res: notify::Result<notify::Event>| {
                                if let Ok(ev) = res {
                                    for e in normalize_notify_event(&ev) {
                                        let _ = tx_clone.send(e);
                                    }
                                }
                            },
                            Config::default(),
                        ) {
                            if w.watch(root, RecursiveMode::Recursive).is_ok() {
                                watchers.push(w);
                            }
                        }
                    }
                }
            }
            // 批量消费去抖
            if let Ok(first) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
                let mut batch = vec![first];
                while let Ok(e) = rx.try_recv() {
                    batch.push(e);
                }
                std::thread::sleep(std::time::Duration::from_millis(2000));
                while let Ok(e) = rx.try_recv() {
                    batch.push(e);
                }
                let st = app.state::<crate::AppState>();
                let db_path = st
                    .data_dir
                    .lock()
                    .expect("data dir lock")
                    .clone()
                    .join("workspace.db");
                if let Ok(mut conn) = crate::db::connection::open(&db_path) {
                    for (vol, _root) in &roots {
                        let vol_events: Vec<WatchEvent> = batch
                            .iter()
                            .filter(|e| event_volume(e) == *vol)
                            .cloned()
                            .collect();
                        if !vol_events.is_empty() {
                            let gen = current_generation(&conn, vol).unwrap_or(1);
                            let _ = apply_watch_events(&mut conn, vol, gen, &vol_events);
                        }
                    }
                }
            }
        }
    });
}

fn event_volume(e: &WatchEvent) -> String {
    let p = match e {
        WatchEvent::Create(p) | WatchEvent::Remove(p) => p,
        WatchEvent::Rename(f, _) => f,
    };
    let lower = p.to_lowercase();
    lower
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| format!("{:02x}", volume_hash(std::path::Path::new(&format!("{c}:\\")))))
        .unwrap_or_default()
}

fn current_generation(conn: &Connection, volume_id: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT scan_generation FROM system_search_scan_state WHERE volume_id = ?1",
        [volume_id],
        |r| r.get(0),
    )
}

/// 将 notify 事件归一化为 WatchEvent 列表。
fn normalize_notify_event(ev: &notify::Event) -> Vec<WatchEvent> {
    use notify::event::{ModifyKind, RenameMode};
    let mut out = Vec::new();
    match ev.kind {
        EventKind::Create(_) => {
            for p in &ev.paths {
                out.push(WatchEvent::Create(p.to_string_lossy().to_string()));
            }
        }
        EventKind::Remove(_) => {
            for p in &ev.paths {
                out.push(WatchEvent::Remove(p.to_string_lossy().to_string()));
            }
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            // 首版把 To 当作 Create 处理，旧路径残留由分卷校验清理
            for p in &ev.paths {
                out.push(WatchEvent::Create(p.to_string_lossy().to_string()));
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn conn() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut c).unwrap();
        c
    }

    #[test]
    fn upsert_volume_state_creates_then_updates() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v1", "C:\\", "pending", 1).unwrap();
        let s = get_volume_state(&c, "v1").unwrap().unwrap();
        assert_eq!(s.status, "pending");
        assert_eq!(s.root_path, "C:\\");
        // 先制造"完成"状态，再重置为新一轮扫描
        update_volume_progress(&mut c, "v1", Some("C:\\docs"), 999, 5).unwrap();
        set_volume_status(&mut c, "v1", "completed", None).unwrap();
        upsert_volume_state(&mut c, "v1", "C:\\", "scanning", 2).unwrap();
        let s = get_volume_state(&c, "v1").unwrap().unwrap();
        assert_eq!(s.status, "scanning");
        assert_eq!(s.scan_generation, 2);
        // 重置语义：旧完成时间、断点与计数必须清空，避免非终态与旧状态矛盾
        assert!(s.completed_at.is_none(), "新一轮扫描不应残留旧 completed_at");
        assert!(s.checkpoint.is_none(), "新一轮扫描不应残留旧 checkpoint");
        assert_eq!(s.indexed_count, 0);
        assert_eq!(s.skipped_count, 0);
    }

    #[test]
    fn list_fixed_volumes_returns_at_least_system_drive() {
        let vols = list_fixed_volumes();
        assert!(!vols.is_empty(), "本机应至少有一个固定磁盘");
        // Windows 下盘符应为 X:\ 形式
        #[cfg(windows)]
        assert!(
            vols.iter().all(|p| p.to_string_lossy().ends_with('\\')),
            "固定磁盘根路径应以反斜杠结尾"
        );
    }

    #[test]
    fn non_terminal_status_keeps_completed_at_unchanged() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v3", "E:\\", "scanning", 1).unwrap();
        set_volume_status(&mut c, "v3", "completed", None).unwrap();
        let s = get_volume_state(&c, "v3").unwrap().unwrap();
        assert!(s.completed_at.is_some());
        // 非终态更新不应写入/覆盖 completed_at
        set_volume_status(&mut c, "v3", "paused", Some("test pause")).unwrap();
        let s = get_volume_state(&c, "v3").unwrap().unwrap();
        assert_eq!(s.status, "paused");
        assert_eq!(s.last_error.as_deref(), Some("test pause"));
        assert!(s.completed_at.is_some(), "非终态不应清空 completed_at");
    }

    #[test]
    fn progress_checkpoint_none_keeps_previous() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v4", "F:\\", "scanning", 1).unwrap();
        update_volume_progress(&mut c, "v4", Some("F:\\a\\b"), 10, 2).unwrap();
        update_volume_progress(&mut c, "v4", None, 20, 3).unwrap();
        let s = get_volume_state(&c, "v4").unwrap().unwrap();
        assert_eq!(s.checkpoint.as_deref(), Some("F:\\a\\b"), "None 应保持原 checkpoint");
        assert_eq!(s.indexed_count, 20, "计数为绝对覆盖");
        assert_eq!(s.skipped_count, 3);
    }

    use std::fs;

    #[test]
    fn scan_creates_entries_and_skips_excluded() {
        let mut c = conn();
        let root = std::env::temp_dir().join(format!("nexus-scan-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.txt"), "x").unwrap();
        fs::write(root.join("sub/b.md"), "y").unwrap();
        fs::create_dir_all(root.join("Windows")).unwrap();
        fs::write(root.join("Windows/system32.dll"), "z").unwrap();

        let excluded = vec![root.join("Windows").to_string_lossy().to_string()];
        scan_directory(&mut c, &root, "v1", 1, &excluded, &ScanControl::default()).unwrap();

        let count: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE volume_id='v1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count >= 2, "应索引 a.txt 与 sub/b.md，实际 {count}");
        let win_count: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE canonical_path LIKE '%windows%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(win_count, 0, "排除目录不应被索引");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_does_not_follow_symlinks() {
        #[cfg(windows)]
        {
            let mut c = conn();
            let root = std::env::temp_dir().join(format!("nexus-scan-link-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("real")).unwrap();
            fs::write(root.join("real/t.txt"), "x").unwrap();
            let _ = std::os::windows::fs::symlink_dir(&root.join("real"), root.join("loop"));
            scan_directory(&mut c, &root, "v2", 1, &[], &ScanControl::default()).unwrap();
            let count: i64 = c
                .query_row("SELECT count(*) FROM system_search_entries", [], |r| r.get(0))
                .unwrap();
            assert!(count < 100, "符号链接循环不应造成爆炸式索引");
            let _ = fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn cancelled_scan_stops_early() {
        let mut c = conn();
        let control = ScanControl {
            cancelled: Arc::new(AtomicBool::new(true)),
            ..Default::default()
        };
        let root = std::env::temp_dir().join(format!("nexus-scan-cancel-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("x.txt"), "x").unwrap();
        let (indexed, _) = scan_directory(&mut c, &root, "v3", 1, &[], &control).unwrap();
        assert_eq!(indexed, 0, "取消标志应立即停止");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn path_under_respects_component_boundary() {
        let cases = [
            ("C:\\Windows\\System32\\a.dll", "C:\\Windows", true),
            ("C:\\Windows2\\a.dll", "C:\\Windows", false),
            ("C:\\WindowsUpdate\\a", "C:\\Windows", false),
            ("C:\\Windows", "C:\\Windows", true),
            ("C:\\Windows\\a", "C:\\Windows\\", true), // 尾随分隔符
            ("D:\\docs\\x", "C:\\Windows", false),
            ("C:\\windows\\system32\\a", "c:\\WINDOWS", true), // 大小写无关
            ("C:\\$Recycle.Bin\\s-1-5\\a.json", "$Recycle.Bin", true), // 相对名：任意盘任意层级
            ("D:\\$recycle.bin\\x", "$Recycle.Bin", true),
            ("C:\\Users\\me\\Desktop\\$recycle.bin\\f", "$Recycle.Bin", true),
            ("C:\\Users\\me\\docs\\a.txt", "$Recycle.Bin", false),
            ("E:\\System Volume Information\\x", "System Volume Information", true),
            ("E:\\Users\\me\\docs", "System Volume Information", false),
        ];
        for (path, excluded, expected) in cases {
            assert_eq!(
                path_under(std::path::Path::new(path), std::path::Path::new(excluded)),
                expected,
                "{path} under {excluded}"
            );
        }
    }

    #[test]
    fn scan_writes_checkpoint_for_resume() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v5", "Z:\\", "scanning", 1).unwrap();
        let root = std::env::temp_dir().join(format!("nexus-scan-cp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("deep")).unwrap();
        fs::write(root.join("deep/f.txt"), "x").unwrap();
        scan_directory(&mut c, &root, "v5", 1, &[], &ScanControl::default()).unwrap();
        let s = get_volume_state(&c, "v5").unwrap().unwrap();
        assert!(s.checkpoint.is_some(), "扫描完成后应写入断点");
        assert!(s.indexed_count >= 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_watch_events_inserts_removes_and_renames() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v3", "C:\\", "completed", 1).unwrap();
        c.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\x\\old.txt','old.txt','file','v3',1,1),
                    ('c:\\x\\keep.txt','keep.txt','file','v3',1,1)",
            [],
        )
        .unwrap();
        // Create 分支要求文件真实存在（is_file），用临时文件保证测试可复现
        let tmp = std::env::temp_dir().join(format!("nexus-watch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let new_path = tmp.join("new.txt");
        fs::write(&new_path, "x").unwrap();
        let new_path_str = new_path.to_string_lossy().to_string();
        let new_canon = crate::services::global_search::canonical_key(&new_path_str);
        let events = vec![
            WatchEvent::Create(new_path_str),
            WatchEvent::Remove("c:\\x\\old.txt".into()),
            WatchEvent::Rename("c:\\x\\keep.txt".into(), "c:\\x\\renamed.txt".into()),
        ];
        apply_watch_events(&mut c, "v3", 1, &events).unwrap();
        let count: i64 = c
            .query_row("SELECT count(*) FROM system_search_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let has_new: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM system_search_entries WHERE canonical_path=?1)",
                [&new_canon],
                |r| r.get(0),
            )
            .unwrap();
        assert!(has_new);
        let has_old: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM system_search_entries WHERE canonical_path='c:\\x\\old.txt')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!has_old);
        let has_renamed: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM system_search_entries WHERE canonical_path='c:\\x\\renamed.txt')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(has_renamed);
        let has_old_name: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM system_search_entries WHERE canonical_path='c:\\x\\keep.txt')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!has_old_name);
        // 重命名后 display_name 应使用新文件名
        let renamed_name: String = c
            .query_row(
                "SELECT display_name FROM system_search_entries WHERE canonical_path='c:\\x\\renamed.txt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(renamed_name, "renamed.txt");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn mark_volume_offline_flags_entries_and_status() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v4", "D:\\", "completed", 1).unwrap();
        c.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('d:\\a.txt','a.txt','file','v4',1,1),
                    ('d:\\b.txt','b.txt','file','v4',1,1)",
            [],
        )
        .unwrap();
        mark_volume_offline(&mut c, "v4").unwrap();
        let status: String = c
            .query_row("SELECT status FROM system_search_scan_state WHERE volume_id='v4'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "offline");
        let offline: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE volume_id='v4' AND is_offline=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(offline, 2, "离线时记录保留但置灰");
    }

    #[test]
    fn mark_volume_back_online_resets_to_pending() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v5", "E:\\", "completed", 1).unwrap();
        c.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('e:\\a.txt','a.txt','file','v5',1,1)",
            [],
        )
        .unwrap();
        mark_volume_offline(&mut c, "v5").unwrap();
        mark_volume_back_online(&mut c, "v5").unwrap();
        let status: String = c
            .query_row("SELECT status FROM system_search_scan_state WHERE volume_id='v5'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "pending", "重新上线后应置回 pending 待重扫");
        let offline: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE volume_id='v5' AND is_offline=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(offline, 0, "重新上线应清理离线条目");
    }

    #[test]
    fn rescan_with_new_generation_keeps_existing_entries() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v7", "G:\\", "completed", 1).unwrap();
        let root = std::env::temp_dir().join(format!("nexus-rescan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let f1 = root.join("a.txt");
        std::fs::write(&f1, "x").unwrap();
        // 第一代扫描
        scan_directory(&mut c, &root, "v7", 1, &[], &ScanControl::default()).unwrap();
        let gen1: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE volume_id='v7' AND scan_generation=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(gen1 >= 1);
        // 第二代重建（模拟 rebuild_search_index：gen+1，清断点）
        upsert_volume_state(&mut c, "v7", "G:\\", "scanning", 2).unwrap();
        std::fs::write(root.join("b.txt"), "y").unwrap();
        scan_directory(&mut c, &root, "v7", 2, &[], &ScanControl::default()).unwrap();
        // 完成时清理旧代次
        c.execute(
            "DELETE FROM system_search_entries WHERE volume_id='v7' AND scan_generation < 2",
            [],
        )
        .unwrap();
        let remains: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE volume_id='v7'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(remains >= 2, "重建后应保留 a.txt 与 b.txt，实际 {remains}");
        let gen2_entries: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE volume_id='v7' AND scan_generation=2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(gen2_entries, remains, "清理后所有条目应属于新代次");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_volume_prunes_stale_entries() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v6", "F:\\", "completed", 1).unwrap();
        // 用临时目录真实存在的文件作为保留项，用不存在的路径作为陈旧项
        let tmp = std::env::temp_dir().join(format!("nexus-verify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let real = tmp.join("real.txt");
        std::fs::write(&real, "x").unwrap();
        let real_str = real.to_string_lossy().to_lowercase().replace('/', "\\");
        c.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES (?1,'real.txt','file','v6',1,1),
                    ('f:\\gone.txt','gone.txt','file','v6',1,1)",
            [&real_str],
        )
        .unwrap();
        let removed = verify_volume(&mut c, "v6", 100).unwrap();
        assert_eq!(removed, 1, "应删除磁盘上不存在的记录");
        let remains: i64 = c
            .query_row("SELECT count(*) FROM system_search_entries WHERE volume_id='v6'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remains, 1, "真实存在的文件应保留");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn scan_start_ignores_checkpoint_as_resume_origin() {
        // checkpoint 只用于进度展示，绝不能作为恢复起点（会漏掉兄弟目录）
        let root = std::path::PathBuf::from("C:\\");
        assert_eq!(
            scan_start(Some("C:\\docs\\sub1"), &root),
            root,
            "恢复起点必须始终是卷根"
        );
        assert_eq!(scan_start(None, &root), root);
    }

    #[test]
    fn same_generation_rescan_from_root_keeps_all_entries() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v9", "Z:\\", "scanning", 1).unwrap();
        let root = std::env::temp_dir().join(format!("nexus-resume-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub1")).unwrap();
        fs::create_dir_all(root.join("sub2")).unwrap();
        fs::write(root.join("sub1/a.txt"), "x").unwrap();
        fs::write(root.join("sub2/b.txt"), "y").unwrap();

        // 第一轮：模拟中断（只完成一部分，checkpoint 停在 sub1）
        let control = ScanControl {
            paused: Arc::new(AtomicBool::new(false)),
            cancelled: Arc::new(AtomicBool::new(true)),
        };
        // 直接构造：第一轮用 cancelled 提前终止
        let (_i1, _s1) = scan_directory(&mut c, &root, "v9", 1, &[], &control).unwrap();
        let _ = update_volume_progress(&mut c, "v9", Some("Z:\\sub1"), 1, 0);

        // 第二轮：从卷根重扫（同代次），不应漏掉任何目录
        let (indexed, _skipped) =
            scan_directory(&mut c, &root, "v9", 1, &[], &ScanControl::default()).unwrap();
        assert!(indexed >= 2, "重扫应覆盖所有文件，实际 {indexed}");

        let count: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE volume_id='v9'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count >= 2, "恢复后索引必须包含全部目录，实际 {count}");

        let _ = fs::remove_dir_all(&root);
    }
}
