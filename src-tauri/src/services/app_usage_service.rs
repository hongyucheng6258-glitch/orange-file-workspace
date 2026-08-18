//! 软件使用时间统计：按前台活跃时间累计。
//!
//! 采样策略：每 5 秒读取 Windows 前台窗口所属进程的可执行路径和系统空闲时长。
//! 仅当空闲时间小于阈值（默认 5 分钟）时，才将本次采样间隔计入活跃时间。
//! 锁屏、休眠、无前台窗口时不计时。单次增量上限 10 秒，避免休眠唤醒后误记。
//! 应用按规范化后的可执行路径归并，同一路径的多个窗口和进程归为一个软件。
//! Orange 自身默认不计入统计。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::connection::now_unix;
use crate::error::AppError;

/// 采样间隔（秒）。
const SAMPLE_INTERVAL_SECS: u64 = 5;
/// 单次增量上限（秒），避免休眠唤醒后一次性记入过长时间。
const MAX_INCREMENT_SECS: i64 = 10;
/// 默认空闲阈值（秒），超过则认为用户离开。
const DEFAULT_IDLE_THRESHOLD_SECS: i64 = 300;

/// 前台窗口快照：采样到的一条原始数据。
#[derive(Debug, Clone)]
struct ForegroundSnapshot {
    /// 规范化后的可执行路径（小写、去除 `\\?\` 前缀），作为应用唯一标识。
    canonical_path: String,
    /// 进程名（如 chrome.exe）。
    process_name: String,
}

/// 运行时状态：跨采样保持上次前台应用和采样时刻。
struct TrackerState {
    /// 上次采样的前台应用规范化路径。
    last_canonical: Option<String>,
    /// 上次采样时刻。
    last_at: Instant,
    /// 用户配置的空闲阈值（秒）。
    idle_threshold_secs: i64,
    /// 是否暂停统计。
    paused: bool,
}

/// 后台采样器：持有运行时状态和数据库连接。
pub struct AppUsageTracker {
    state: Mutex<TrackerState>,
    /// 停止标志，退出时设为 true。
    stop: Arc<AtomicBool>,
}

impl AppUsageTracker {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(TrackerState {
                last_canonical: None,
                last_at: Instant::now(),
                idle_threshold_secs: DEFAULT_IDLE_THRESHOLD_SECS,
                paused: false,
            }),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 停止后台采样线程。
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// 暂停统计。
    pub fn pause(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.paused = true;
            state.last_canonical = None;
        }
    }

    /// 恢复统计。
    pub fn resume(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.paused = false;
            state.last_at = Instant::now();
        }
    }

    /// 设置空闲阈值（秒）。
    pub fn set_idle_threshold(&self, secs: i64) {
        if let Ok(mut state) = self.state.lock() {
            state.idle_threshold_secs = secs.max(30);
        }
    }

    /// 执行一次采样并持久化。由后台线程周期调用，也可在测试中直接调用。
    pub fn tick(&self, conn: &Connection) {
        // 关闭窗口/退出时不再采样。
        if self.stop.load(Ordering::Relaxed) {
            return;
        }
        let now = Instant::now();
        // 读取前台窗口和空闲时长
        let fg = read_foreground();
        let idle = read_idle_secs();

        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        if state.paused {
            state.last_canonical = None;
            state.last_at = now;
            return;
        }

        // 计算本次增量秒数
        let elapsed = now.duration_since(state.last_at).as_secs() as i64;
        let increment = elapsed.min(MAX_INCREMENT_SECS);

        // 仅在以下条件全满足时才累计：
        // 1. 有前台窗口快照
        // 2. 空闲时间 < 阈值
        // 3. 增量 > 0
        // 4. 前台应用与上次相同（跨应用切换时上次的时间不计入新应用）
        if let Some(ref fg) = fg {
            if idle < state.idle_threshold_secs && increment > 0 {
                if let Some(ref last) = state.last_canonical {
                    if last == &fg.canonical_path {
                        // 持久化增量
                        if let Err(e) =
                            record_usage(conn, &fg.canonical_path, &fg.process_name, increment)
                        {
                            eprintln!("[tracker] record_usage failed: {e}");
                        }
                    }
                }
            }
        }

        // 更新状态
        state.last_canonical = fg.as_ref().map(|f| f.canonical_path.clone());
        state.last_at = now;
    }

    /// 获取当前空闲阈值。
    pub fn idle_threshold(&self) -> i64 {
        self.state
            .lock()
            .map(|s| s.idle_threshold_secs)
            .unwrap_or(DEFAULT_IDLE_THRESHOLD_SECS)
    }

    /// 是否暂停中。
    pub fn is_paused(&self) -> bool {
        self.state.lock().map(|s| s.paused).unwrap_or(false)
    }
}

impl Default for AppUsageTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Windows 原生 API 调用 ──────────────────────────────────────────

#[cfg(windows)]
mod win {
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    use super::{canonicalize_path, ForegroundSnapshot};
    use std::path::Path;

    /// 读取当前前台窗口所属进程的可执行路径和进程名。
    pub fn read_foreground() -> Option<ForegroundSnapshot> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            // 获取可执行路径
            let path = process_image_path(pid)?;
            let canonical = canonicalize_path(&path);
            let name = process_name_from_path(&path);
            Some(ForegroundSnapshot {
                canonical_path: canonical,
                process_name: name,
            })
        }
    }

    /// 通过 QueryFullProcessImageNameW 获取进程可执行路径。
    fn process_image_path(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len: u32 = buf.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let _ = handle; // HANDLE 自动释放
            if result.is_err() || len == 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    /// 读取系统空闲时间（秒）。
    pub fn read_idle_secs() -> i64 {
        unsafe {
            let mut info = LASTINPUTINFO::default();
            info.cbSize = std::mem::size_of::<LASTINPUTINFO>() as u32;
            if GetLastInputInfo(&mut info) == false {
                return 0;
            }
            let tick = windows::Win32::System::SystemInformation::GetTickCount64();
            let last = info.dwTime as u64;
            // GetTickCount64 和 dwTime 都是毫秒
            let idle_ms = tick.saturating_sub(last);
            (idle_ms / 1000) as i64
        }
    }

    fn process_name_from_path(path: &str) -> String {
        Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string())
    }
}

#[cfg(not(windows))]
mod win {
    use super::ForegroundSnapshot;
    pub fn read_foreground() -> Option<ForegroundSnapshot> {
        None
    }
    pub fn read_idle_secs() -> i64 {
        0
    }
}

/// 读取前台窗口快照。
fn read_foreground() -> Option<ForegroundSnapshot> {
    win::read_foreground()
}

/// 读取系统空闲时间（秒）。
fn read_idle_secs() -> i64 {
    win::read_idle_secs()
}

/// 规范化路径：去除 `\\?\` 前缀，统一为小写，统一分隔符为 `\`。
fn canonicalize_path(path: &str) -> String {
    let mut p = path.trim().to_string();
    // 去除 `\\?\` 或 `\\.\` 前缀
    if p.starts_with(r"\\?\") {
        p = p[4..].to_string();
    } else if p.starts_with(r"\\.\") {
        p = p[4..].to_string();
    }
    // 统一分隔符
    p = p.replace('/', r"\");
    // 小写化用于唯一键
    p.to_lowercase()
}

// ── 数据库持久化与查询 ──────────────────────────────────────────────

/// 记录一次使用增量（秒）到当日聚合表。
fn record_usage(
    conn: &Connection,
    canonical_path: &str,
    process_name: &str,
    increment: i64,
) -> Result<(), AppError> {
    let now = now_unix();
    let date_ymd = date_ymd_from_unix(now);

    // 插入或更新应用记录
    let app_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM app_usage_apps WHERE canonical_path = ?1 COLLATE NOCASE",
            params![canonical_path],
            |r| r.get(0),
        )
        .optional()?;

    let app_id = match app_id {
        Some(id) => {
            conn.execute(
                "UPDATE app_usage_apps SET last_seen = ?1, process_name = ?2
                 WHERE id = ?3",
                params![now, process_name, id],
            )?;
            id
        }
        None => {
            // 尝试从路径提取显示名
            let display_name = display_name_from_path(canonical_path, process_name);
            conn.execute(
                "INSERT INTO app_usage_apps
                    (canonical_path, display_name, process_name, first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![canonical_path, display_name, process_name, now],
            )?;
            conn.last_insert_rowid()
        }
    };

    // 插入或更新当日聚合
    conn.execute(
        "INSERT INTO app_usage_daily (app_id, date_ymd, active_seconds, last_active_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (app_id, date_ymd) DO UPDATE SET
            active_seconds = active_seconds + ?3,
            last_active_at = ?4",
        params![app_id, date_ymd, increment, now],
    )?;

    Ok(())
}

/// 从路径提取显示名：去掉扩展名，首字母大写。
fn display_name_from_path(canonical_path: &str, process_name: &str) -> String {
    let stem = Path::new(process_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| process_name.to_string());
    if stem.is_empty() {
        canonical_path.to_string()
    } else {
        stem
    }
}

/// 将 UNIX 时间戳（秒）转换为本地日期 YYYYMMDD 数值。
fn date_ymd_from_unix(ts: i64) -> i64 {
    // 使用 chrono 转换为本地时区日期
    use chrono::TimeZone;
    let dt = chrono::Local.timestamp_opt(ts, 0).single();
    dt.map(|d| d.format("%Y%m%d").to_string().parse::<i64>().unwrap_or(0))
        .unwrap_or(0)
}

/// 清除所有使用时间统计数据。
pub fn clear_all_usage(conn: &Connection) -> Result<(), AppError> {
    conn.execute("DELETE FROM app_usage_daily", [])?;
    conn.execute("DELETE FROM app_usage_apps", [])?;
    Ok(())
}

// ── 查询接口 ──────────────────────────────────────────────────────

/// 单个应用的统计行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AppUsageStat {
    pub app_id: i64,
    pub display_name: String,
    pub process_name: String,
    pub canonical_path: String,
    pub active_seconds: i64,
    pub last_active_at: i64,
    pub percentage: f64,
}

/// 按日期范围的统计汇总。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AppUsageSummary {
    pub range_label: String,
    pub total_active_seconds: i64,
    pub apps: Vec<AppUsageStat>,
    pub daily_totals: Vec<DailyTotal>,
}

/// 每日总计。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DailyTotal {
    pub date_ymd: i64,
    pub active_seconds: i64,
}

/// 查询指定日期范围内的使用时间统计。
///
/// `days` 为查询天数：1=今日，7=近7天，30=近30天。
pub fn query_usage(conn: &Connection, days: u32) -> Result<AppUsageSummary, AppError> {
    let now = now_unix();
    let today_ymd = date_ymd_from_unix(now);

    // 计算起始日期
    let start_ymd = if days <= 1 {
        today_ymd
    } else {
        use chrono::TimeZone;
        let dt = chrono::Local
            .timestamp_opt(now, 0)
            .single()
            .unwrap_or_else(|| chrono::Local::now());
        let start = dt - chrono::Duration::days((days - 1) as i64);
        start
            .format("%Y%m%d")
            .to_string()
            .parse::<i64>()
            .unwrap_or(0)
    };

    let range_label = match days {
        1 => "今日".to_string(),
        7 => "近 7 天".to_string(),
        30 => "近 30 天".to_string(),
        _ => format!("近 {days} 天"),
    };

    // 查询应用级聚合
    let mut stmt = conn.prepare(
        "SELECT a.id, a.display_name, a.process_name, a.canonical_path,
                COALESCE(SUM(d.active_seconds), 0) AS total,
                COALESCE(MAX(d.last_active_at), 0) AS last
         FROM app_usage_apps a
         LEFT JOIN app_usage_daily d ON a.id = d.app_id
           AND d.date_ymd >= ?1 AND d.date_ymd <= ?2
         GROUP BY a.id
         HAVING total > 0
         ORDER BY total DESC",
    )?;
    let rows = stmt.query_map(params![start_ymd, today_ymd], |row| {
        Ok(AppUsageStat {
            app_id: row.get(0)?,
            display_name: row.get(1)?,
            process_name: row.get(2)?,
            canonical_path: row.get(3)?,
            active_seconds: row.get(4)?,
            last_active_at: row.get(5)?,
            percentage: 0.0,
        })
    })?;

    let mut apps: Vec<AppUsageStat> = rows.filter_map(|r| r.ok()).collect();
    let total: i64 = apps.iter().map(|a| a.active_seconds).sum();
    for app in &mut apps {
        app.percentage = if total > 0 {
            (app.active_seconds as f64 / total as f64) * 100.0
        } else {
            0.0
        };
    }

    // 查询每日总计
    let mut stmt2 = conn.prepare(
        "SELECT date_ymd, COALESCE(SUM(active_seconds), 0)
         FROM app_usage_daily
         WHERE date_ymd >= ?1 AND date_ymd <= ?2
         GROUP BY date_ymd
         ORDER BY date_ymd ASC",
    )?;
    let daily_rows = stmt2.query_map(params![start_ymd, today_ymd], |row| {
        Ok(DailyTotal {
            date_ymd: row.get(0)?,
            active_seconds: row.get(1)?,
        })
    })?;
    let daily_totals: Vec<DailyTotal> = daily_rows.filter_map(|r| r.ok()).collect();

    Ok(AppUsageSummary {
        range_label,
        total_active_seconds: total,
        apps,
        daily_totals,
    })
}

// ── 后台采样线程启动 ──────────────────────────────────────────────

/// 启动后台采样线程，使用共享主连接（避免 vmcache 虚拟盘下独立连接被降级为只读）。
pub fn start_tracker_thread(conn: Arc<Mutex<Connection>>, tracker: Arc<AppUsageTracker>) {
    // 注意：不要调用 tracker.stop()——AppUsageTracker 是新建的，初始 stop=false。
    // 调用 stop() 会将其设为 true，导致线程第一次检查就退出，永远不执行 tick。
    let stop = tracker.stop.clone();

    std::thread::spawn(move || {
        let interval = std::time::Duration::from_secs(SAMPLE_INTERVAL_SECS);
        loop {
            std::thread::sleep(interval);
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            match conn.lock() {
                Ok(guard) => tracker.tick(&guard),
                Err(e) => eprintln!("[tracker] conn lock poisoned: {e}"),
            }
        }
    });
}

// ── 单元测试 ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open memory db");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_usage_apps (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                canonical_path TEXT NOT NULL UNIQUE COLLATE NOCASE,
                display_name TEXT NOT NULL,
                process_name TEXT NOT NULL DEFAULT '',
                first_seen INTEGER NOT NULL,
                last_seen INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS app_usage_daily (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                app_id INTEGER NOT NULL,
                date_ymd INTEGER NOT NULL,
                active_seconds INTEGER NOT NULL DEFAULT 0,
                last_active_at INTEGER NOT NULL DEFAULT 0,
                UNIQUE (app_id, date_ymd)
            );",
        )
        .expect("create tables");
        conn
    }

    #[test]
    fn canonicalize_strips_prefix_and_lowercases() {
        assert_eq!(
            canonicalize_path(r"\\?\C:\Program Files\App\app.exe"),
            "c:\\program files\\app\\app.exe"
        );
        assert_eq!(
            canonicalize_path(r"\\.\C:\Users\Test\Run.exe"),
            "c:\\users\\test\\run.exe"
        );
        assert_eq!(
            canonicalize_path(r"C:/Program Files/App/app.exe"),
            "c:\\program files\\app\\app.exe"
        );
    }

    #[test]
    fn record_usage_accumulates_per_day() {
        let conn = setup_db();
        let path = "c:\\program files\\chrome\\chrome.exe";
        let name = "chrome.exe";

        record_usage(&conn, path, name, 5).unwrap();
        record_usage(&conn, path, name, 5).unwrap();
        record_usage(&conn, path, name, 3).unwrap();

        let total: i64 = conn
            .query_row(
                "SELECT active_seconds FROM app_usage_daily
                 WHERE app_id = (SELECT id FROM app_usage_apps WHERE canonical_path = ?1)",
                [path],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 13);
    }

    #[test]
    fn record_usage_creates_app_once() {
        let conn = setup_db();
        let path = "c:\\app\\notepad.exe";
        let name = "notepad.exe";

        record_usage(&conn, path, name, 5).unwrap();
        record_usage(&conn, path, name, 5).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM app_usage_apps", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn clear_all_wipes_data() {
        let conn = setup_db();
        record_usage(&conn, "c:\\app\\a.exe", "a.exe", 10).unwrap();
        record_usage(&conn, "c:\\app\\b.exe", "b.exe", 20).unwrap();

        clear_all_usage(&conn).unwrap();

        let apps: i64 = conn
            .query_row("SELECT COUNT(*) FROM app_usage_apps", [], |r| r.get(0))
            .unwrap();
        let daily: i64 = conn
            .query_row("SELECT COUNT(*) FROM app_usage_daily", [], |r| r.get(0))
            .unwrap();
        assert_eq!(apps, 0);
        assert_eq!(daily, 0);
    }

    #[test]
    fn query_usage_returns_sorted_apps() {
        let conn = setup_db();
        // 使用同一日期确保聚合
        record_usage(&conn, "c:\\a.exe", "a.exe", 30).unwrap();
        record_usage(&conn, "c:\\b.exe", "b.exe", 60).unwrap();
        record_usage(&conn, "c:\\c.exe", "c.exe", 10).unwrap();

        let summary = query_usage(&conn, 1).unwrap();
        assert_eq!(summary.apps.len(), 3);
        // 按时间降序：b > a > c
        assert_eq!(summary.apps[0].process_name, "b.exe");
        assert_eq!(summary.apps[1].process_name, "a.exe");
        assert_eq!(summary.apps[2].process_name, "c.exe");
        assert_eq!(summary.total_active_seconds, 100);
        // 百分比之和应接近 100
        let pct_sum: f64 = summary.apps.iter().map(|a| a.percentage).sum();
        assert!((pct_sum - 100.0).abs() < 0.1);
    }

    #[test]
    fn tracker_pause_and_resume() {
        let tracker = AppUsageTracker::new();
        assert!(!tracker.is_paused());
        tracker.pause();
        assert!(tracker.is_paused());
        tracker.resume();
        assert!(!tracker.is_paused());
    }

    #[test]
    fn tracker_idle_threshold_clamped() {
        let tracker = AppUsageTracker::new();
        tracker.set_idle_threshold(5); // 低于最小值
        assert_eq!(tracker.idle_threshold(), 30); // 被钳制为最小值
        tracker.set_idle_threshold(600);
        assert_eq!(tracker.idle_threshold(), 600);
    }

    #[test]
    fn display_name_extracts_stem() {
        assert_eq!(display_name_from_path("", "chrome.exe"), "chrome");
        assert_eq!(display_name_from_path("", "Code.exe"), "Code");
        assert_eq!(display_name_from_path("c:\\app\\", ""), "c:\\app\\");
    }

    #[test]
    fn date_ymd_is_eight_digits() {
        let ymd = date_ymd_from_unix(now_unix());
        assert!(ymd >= 20200000);
        assert!(ymd < 21000000);
    }
}
