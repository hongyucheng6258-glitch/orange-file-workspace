//! 运行历史存储：已退出运行记录落库，默认保留策略为每项目 20 条、最多 7 天。
//!
//! 日志不跨应用重启持久化；历史只保存运行快照与脱敏摘要。

use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::db::connection::now_unix;
use crate::services::project_runtime::{RunSnapshot, RunState};

/// 每项目最多保留的已退出记录数。
pub const MAX_PER_PROJECT: usize = 20;
/// 全局保留天数。
pub const MAX_DAYS: i64 = 7;

/// 历史运行记录（终态）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRun {
    pub run_id: String,
    pub project_id: String,
    pub project_key: String,
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: Vec<(String, String)>,
    pub expected_port: Option<u16>,
    pub preview_scheme: String,
    pub state: RunState,
    pub exit_code: Option<u32>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub stop_reason: Option<String>,
    pub started_at: i64,
    pub exited_at: i64,
}

impl HistoryRun {
    /// 从运行快照构造历史记录（`exited_at` 取当前时间）。
    pub fn from_snapshot(snap: &RunSnapshot, project_key: &str) -> Self {
        let summary = snap.summary.as_object().cloned().unwrap_or_default();
        let get = |k: &str| {
            summary
                .get(k)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let args: Vec<String> = summary
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let env: Vec<(String, String)> = summary
            .get("env")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let expected_port = summary
            .get("expected_port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16);
        Self {
            run_id: snap.run_id.clone(),
            project_id: snap.project_id.clone(),
            project_key: project_key.to_string(),
            executable: get("executable"),
            args,
            cwd: get("cwd"),
            env,
            expected_port,
            preview_scheme: get("preview_scheme"),
            state: snap.state,
            exit_code: snap.exit_code,
            error_code: snap.error_code.clone(),
            error_message: snap.error_message.clone(),
            stop_reason: snap.stop_reason.clone(),
            started_at: snap.started_at.unwrap_or(0),
            exited_at: now_unix(),
        }
    }

    /// 转回运行快照（历史记录均为终态，PID 不可用）。
    pub fn to_snapshot(&self) -> RunSnapshot {
        let env_map: serde_json::Map<String, serde_json::Value> = self
            .env
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        let summary = serde_json::json!({
            "executable": self.executable,
            "args": self.args,
            "cwd": self.cwd,
            "env": env_map,
            "expected_port": self.expected_port,
            "preview_scheme": self.preview_scheme,
        });
        RunSnapshot {
            run_id: self.run_id.clone(),
            project_id: self.project_id.clone(),
            state: self.state,
            cwd: self.cwd.clone(),
            pid: None,
            started_at: Some(self.started_at),
            exit_code: self.exit_code,
            error_code: self.error_code.clone(),
            error_message: self.error_message.clone(),
            stop_reason: self.stop_reason.clone(),
            summary,
        }
    }
}

/// 运行历史存储抽象；SQLite 生产实现 + 内存测试实现。
pub trait RunHistoryStore: Send + Sync {
    /// 写入一条已退出记录并执行保留策略。
    fn record(&self, snap: &RunSnapshot, project_key: &str) -> Result<(), String>;
    /// 按启动时间倒序返回最近 `limit` 条。
    fn list(&self, limit: usize) -> Vec<HistoryRun>;
    /// 指定项目的最近 `limit` 条（按启动时间倒序）。
    fn by_project(&self, project_key: &str, limit: usize) -> Vec<HistoryRun>;
    /// 执行保留策略：每项目最多 `MAX_PER_PROJECT` 条、`MAX_DAYS` 天内。
    fn prune(&self);
}

fn env_to_json(env: &[(String, String)]) -> String {
    let map: serde_json::Map<String, serde_json::Value> = env
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

fn state_to_str(s: RunState) -> &'static str {
    match s {
        RunState::Starting => "starting",
        RunState::Running => "running",
        RunState::Stopping => "stopping",
        RunState::Exited => "exited",
        RunState::Failed => "failed",
    }
}

fn state_from_str(s: &str) -> RunState {
    match s {
        "starting" => RunState::Starting,
        "running" => RunState::Running,
        "stopping" => RunState::Stopping,
        "failed" => RunState::Failed,
        _ => RunState::Exited,
    }
}

/// SQLite 生产实现。连接由调用方注入（同一应用数据库文件）。
pub struct SqliteRunHistoryStore {
    conn: Mutex<Connection>,
}

impl SqliteRunHistoryStore {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }
}

impl RunHistoryStore for SqliteRunHistoryStore {
    fn record(&self, snap: &RunSnapshot, project_key: &str) -> Result<(), String> {
        let h = HistoryRun::from_snapshot(snap, project_key);
        let args_json = serde_json::to_string(&h.args).unwrap_or_else(|_| "[]".to_string());
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO project_run_history (
                    run_id, project_id, project_key, executable, args_json, cwd, env_json,
                    expected_port, preview_scheme, state, exit_code, error_code, error_message,
                    stop_reason, started_at, exited_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    h.run_id,
                    h.project_id,
                    h.project_key,
                    h.executable,
                    args_json,
                    h.cwd,
                    env_to_json(&h.env),
                    h.expected_port,
                    h.preview_scheme,
                    state_to_str(h.state),
                    h.exit_code,
                    h.error_code,
                    h.error_message,
                    h.stop_reason,
                    h.started_at,
                    h.exited_at,
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        self.prune();
        Ok(())
    }

    fn list(&self, limit: usize) -> Vec<HistoryRun> {
        let conn = self.conn.lock().unwrap();
        let Ok(mut stmt) = conn.prepare(
            "SELECT run_id, project_id, project_key, executable, args_json, cwd, env_json,
                    expected_port, preview_scheme, state, exit_code, error_code, error_message,
                    stop_reason, started_at, exited_at
             FROM project_run_history
             ORDER BY started_at DESC
             LIMIT ?1",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map([limit as i64], |row| {
            let args: String = row.get(4)?;
            let env: String = row.get(6)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                args,
                row.get::<_, String>(5)?,
                env,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, i64>(14)?,
                row.get::<_, i64>(15)?,
            ))
        }) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for row in rows.flatten() {
            let args: Vec<String> = serde_json::from_str(&row.4).unwrap_or_default();
            let env: Vec<(String, String)> = serde_json::from_str(&row.6).unwrap_or_else(|_| {
                serde_json::from_str::<serde_json::Value>(&row.6)
                    .ok()
                    .and_then(|v| v.as_object().cloned())
                    .map(|m| {
                        m.into_iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default()
            });
            let state: RunState = state_from_str(&row.9);
            out.push(HistoryRun {
                run_id: row.0,
                project_id: row.1,
                project_key: row.2,
                executable: row.3,
                args,
                cwd: row.5,
                env,
                expected_port: row.7.map(|p| p as u16),
                preview_scheme: row.8,
                state,
                exit_code: row.10.map(|c| c as u32),
                error_code: row.11,
                error_message: row.12,
                stop_reason: row.13,
                started_at: row.14,
                exited_at: row.15,
            });
        }
        out
    }

    fn by_project(&self, project_key: &str, limit: usize) -> Vec<HistoryRun> {
        let conn = self.conn.lock().unwrap();
        let Ok(mut stmt) = conn.prepare(
            "SELECT run_id, project_id, project_key, executable, args_json, cwd, env_json,
                    expected_port, preview_scheme, state, exit_code, error_code, error_message,
                    stop_reason, started_at, exited_at
             FROM project_run_history
             WHERE project_key = ?1
             ORDER BY started_at DESC
             LIMIT ?2",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map(params![project_key, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, i64>(14)?,
                row.get::<_, i64>(15)?,
            ))
        }) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for row in rows.flatten() {
            let args: Vec<String> = serde_json::from_str(&row.4).unwrap_or_default();
            let env: Vec<(String, String)> = serde_json::from_str(&row.6).unwrap_or_else(|_| {
                serde_json::from_str::<serde_json::Value>(&row.6)
                    .ok()
                    .and_then(|v| v.as_object().cloned())
                    .map(|m| {
                        m.into_iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default()
            });
            let state: RunState = state_from_str(&row.9);
            out.push(HistoryRun {
                run_id: row.0,
                project_id: row.1,
                project_key: row.2,
                executable: row.3,
                args,
                cwd: row.5,
                env,
                expected_port: row.7.map(|p| p as u16),
                preview_scheme: row.8,
                state,
                exit_code: row.10.map(|c| c as u32),
                error_code: row.11,
                error_message: row.12,
                stop_reason: row.13,
                started_at: row.14,
                exited_at: row.15,
            });
        }
        out
    }

    fn prune(&self) {
        let conn = self.conn.lock().unwrap();
        // 全局 7 天。
        let cutoff = now_unix() - MAX_DAYS * 86400;
        let _ = conn.execute(
            "DELETE FROM project_run_history WHERE exited_at < ?1",
            params![cutoff],
        );
        // 每项目保留最近 MAX_PER_PROJECT 条。
        let _ = conn.execute(
            "DELETE FROM project_run_history
             WHERE run_id IN (
                 SELECT run_id FROM (
                     SELECT run_id,
                            ROW_NUMBER() OVER (
                                PARTITION BY project_key ORDER BY started_at DESC
                            ) AS rn
                     FROM project_run_history
                 ) WHERE rn > ?1
             )",
            params![MAX_PER_PROJECT as i64],
        );
    }
}

/// 内存测试实现。
#[derive(Default)]
#[allow(dead_code)] // 测试模块与 backup_service 测试使用
pub struct InMemoryRunHistoryStore {
    pub runs: Mutex<Vec<HistoryRun>>,
}

impl InMemoryRunHistoryStore {
    #[allow(dead_code)] // 测试模块与 backup_service 测试使用
    pub fn new() -> Self {
        Self::default()
    }
}

impl RunHistoryStore for InMemoryRunHistoryStore {
    fn record(&self, snap: &RunSnapshot, project_key: &str) -> Result<(), String> {
        let h = HistoryRun::from_snapshot(snap, project_key);
        self.runs.lock().unwrap().push(h);
        self.prune();
        Ok(())
    }

    fn list(&self, limit: usize) -> Vec<HistoryRun> {
        let mut runs = self.runs.lock().unwrap().clone();
        runs.sort_by_key(|r| std::cmp::Reverse(r.started_at));
        runs.truncate(limit);
        runs
    }

    fn by_project(&self, project_key: &str, limit: usize) -> Vec<HistoryRun> {
        let mut runs: Vec<HistoryRun> = self
            .runs
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.project_key == project_key)
            .cloned()
            .collect();
        runs.sort_by_key(|r| std::cmp::Reverse(r.started_at));
        runs.truncate(limit);
        runs
    }

    fn prune(&self) {
        let mut runs = self.runs.lock().unwrap();
        // 全局 7 天。
        let cutoff = now_unix() - MAX_DAYS * 86400;
        runs.retain(|r| r.exited_at >= cutoff);
        // 每项目保留最近 MAX_PER_PROJECT 条。
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        runs.sort_by_key(|r| std::cmp::Reverse(r.started_at));
        runs.retain(|r| {
            let count = seen.entry(r.project_key.clone()).or_insert(0);
            *count += 1;
            *count <= MAX_PER_PROJECT
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn snapshot(run_id: &str, project_id: &str, started_at: i64) -> RunSnapshot {
        RunSnapshot {
            run_id: run_id.into(),
            project_id: project_id.into(),
            state: RunState::Exited,
            cwd: "C:\\proj".into(),
            pid: None,
            started_at: Some(started_at),
            exit_code: Some(0),
            error_code: None,
            error_message: None,
            stop_reason: None,
            summary: serde_json::json!({
                "executable": "node",
                "args": ["run", "dev"],
                "cwd": "C:\\proj",
                "env": { "PORT": "3000", "TOKEN": "****" },
                "expected_port": 3000,
                "preview_scheme": "http",
            }),
        }
    }

    fn sqlite_store() -> SqliteRunHistoryStore {
        let conn = Connection::open_in_memory().expect("open memory db");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS project_run_history (
                run_id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                project_key TEXT NOT NULL,
                executable TEXT NOT NULL,
                args_json TEXT NOT NULL,
                cwd TEXT NOT NULL,
                env_json TEXT NOT NULL DEFAULT '{}',
                expected_port INTEGER,
                preview_scheme TEXT NOT NULL DEFAULT 'http',
                state TEXT NOT NULL,
                exit_code INTEGER,
                error_code TEXT,
                error_message TEXT,
                stop_reason TEXT,
                started_at INTEGER NOT NULL,
                exited_at INTEGER NOT NULL
            );",
        )
        .expect("create table");
        SqliteRunHistoryStore::new(conn)
    }

    #[test]
    fn record_and_list_roundtrip() {
        let store = sqlite_store();
        store.record(&snapshot("r1", "p1", 100), "key-1").unwrap();
        let list = store.list(10);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].run_id, "r1");
        assert_eq!(list[0].executable, "node");
        assert_eq!(list[0].args, vec!["run", "dev"]);
        assert_eq!(list[0].expected_port, Some(3000));
        assert_eq!(list[0].preview_scheme, "http");
        assert_eq!(
            list[0].env,
            vec![
                ("PORT".to_string(), "3000".to_string()),
                ("TOKEN".to_string(), "****".to_string())
            ]
        );
        let snap = list[0].to_snapshot();
        assert_eq!(snap.state, RunState::Exited);
        assert_eq!(snap.pid, None);
        assert_eq!(snap.summary["expected_port"], 3000);
    }

    #[test]
    fn list_orders_by_started_at_desc() {
        let store = sqlite_store();
        store.record(&snapshot("r1", "p1", 100), "key-1").unwrap();
        store.record(&snapshot("r2", "p1", 200), "key-1").unwrap();
        let list = store.list(10);
        assert_eq!(list[0].run_id, "r2");
        assert_eq!(list[1].run_id, "r1");
    }

    #[test]
    fn by_project_filters() {
        let store = sqlite_store();
        store.record(&snapshot("r1", "p1", 100), "key-1").unwrap();
        store.record(&snapshot("r2", "p2", 200), "key-2").unwrap();
        let list = store.by_project("key-1", 10);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].run_id, "r1");
    }

    #[test]
    fn prune_keeps_max_per_project() {
        let store = sqlite_store();
        for i in 0..25 {
            store
                .record(&snapshot(&format!("r{i}"), "p1", i as i64), "key-1")
                .unwrap();
        }
        let list = store.list(100);
        assert_eq!(list.len(), MAX_PER_PROJECT);
        // 保留最新 20 条（r24..r5）。
        assert_eq!(list[0].run_id, "r24");
        assert!(list.iter().any(|r| r.run_id == "r5"));
        assert!(!list.iter().any(|r| r.run_id == "r4"));
    }

    #[test]
    fn prune_removes_old_records() {
        let store = InMemoryRunHistoryStore::new();
        store.record(&snapshot("r1", "p1", 1), "key-1").unwrap();
        assert_eq!(store.list(10).len(), 1);
        // 手动构造一条 8 天前的记录并 prune。
        store.runs.lock().unwrap().push(HistoryRun {
            exited_at: now_unix() - 8 * 86400,
            ..HistoryRun::from_snapshot(&snapshot("r-old", "p1", 1), "key-1")
        });
        store.prune();
        let list = store.list(10);
        assert!(!list.iter().any(|r| r.run_id == "r-old"));
    }

    #[test]
    fn in_memory_store_matches_policy() {
        let store = InMemoryRunHistoryStore::new();
        for i in 0..25 {
            store
                .record(&snapshot(&format!("r{i}"), "p1", i as i64), "key-1")
                .unwrap();
        }
        assert_eq!(store.list(100).len(), MAX_PER_PROJECT);
        assert_eq!(store.by_project("key-1", 100).len(), MAX_PER_PROJECT);
    }
}
