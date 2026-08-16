//! 项目运行状态机与日志管线。
//!
//! 管理项目级单实例、启动/停止/重启、stdout/stderr 实时事件、受限内存日志
//! 缓存和停止超时清理表。所有 Windows 调用通过 `Win32ProcessApi` 抽象，
//! 测试使用注入式替身覆盖失败分支与竞态。

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::error::AppError;
use crate::services::process_api::{
    JobHandle, ProcessApiError, ProcessSpec, SuspendedProcess, WaitResult, Win32ProcessApi,
};
use crate::services::run_confirmation::{
    canonical_key, is_sensitive_key, ConfirmationGrant, ConfirmationPreview, ConfirmationSession,
    NormalizedRunConfig, RunConfig,
};
use crate::services::run_history::RunHistoryStore;
use crate::services::web_preview_service::{PreviewContext, PreviewTarget};

/// 停止超时上限。
pub const STOP_TIMEOUT: Duration = Duration::from_secs(5);
/// 日志单事件上限。
pub const LOG_CHUNK_BYTES: usize = 32 * 1024;
/// 每个运行保留的日志总上限。
pub const LOG_RING_BYTES: usize = 2 * 1024 * 1024;

/// 运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Starting,
    Running,
    Stopping,
    Exited,
    Failed,
}

impl RunState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, RunState::Exited | RunState::Failed)
    }
}

/// 输出流类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// 运行快照（返回给前端）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSnapshot {
    pub run_id: String,
    pub project_id: String,
    pub state: RunState,
    pub cwd: String,
    pub pid: Option<u32>,
    pub started_at: Option<i64>,
    pub exit_code: Option<u32>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub stop_reason: Option<String>,
    pub summary: serde_json::Value,
}

/// 日志条目。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub seq: u64,
    pub stream: OutputStream,
    pub text: String,
    pub truncated: bool,
}

/// 日志分页。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPage {
    pub entries: Vec<LogEntry>,
    pub next_seq: u64,
}

/// 状态事件。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub run_id: String,
    pub project_id: String,
    pub state: RunState,
    pub pid: Option<u32>,
    pub exit_code: Option<u32>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

/// 输出事件。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputPayload {
    pub run_id: String,
    pub project_id: String,
    pub seq: u64,
    pub stream: OutputStream,
    pub text: String,
    pub truncated: bool,
}

/// 退出事件。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitedPayload {
    pub run_id: String,
    pub project_id: String,
    pub exit_code: u32,
    pub stop_reason: Option<String>,
}

/// 错误事件。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub run_id: String,
    pub project_id: String,
    pub error_code: String,
    pub error_message: String,
}

/// 事件输出接口；生产实现包装 AppHandle，测试实现收集事件。
pub trait RunEventSink: Send + Sync {
    fn emit_status(&self, p: &StatusPayload);
    fn emit_output(&self, p: &OutputPayload);
    fn emit_exited(&self, p: &ExitedPayload);
    fn emit_error(&self, p: &ErrorPayload);
    /// 预览就绪事件（端口归属校验完成）。
    fn emit_preview(&self, p: &crate::services::web_preview_service::PreviewTarget);
}

/// 空事件输出（测试与无 UI 场景）。
#[allow(dead_code)] // 备份等模块的测试构造 AppState 时使用
pub struct NullRunEventSink;

impl RunEventSink for NullRunEventSink {
    fn emit_status(&self, _p: &StatusPayload) {}
    fn emit_output(&self, _p: &OutputPayload) {}
    fn emit_exited(&self, _p: &ExitedPayload) {}
    fn emit_error(&self, _p: &ErrorPayload) {}
    fn emit_preview(&self, _p: &crate::services::web_preview_service::PreviewTarget) {}
}

/// 运行错误，映射到 `AppError`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub code: String,
    pub message: String,
}

impl RuntimeError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    fn from_api(code: &str, e: &ProcessApiError) -> Self {
        Self {
            code: code.into(),
            message: e.message.clone(),
        }
    }
}

impl From<RuntimeError> for AppError {
    fn from(e: RuntimeError) -> Self {
        AppError::new(e.code, e.message)
    }
}

/// 每个运行的日志总线：环形缓存 + 独立序号。
#[derive(Clone)]
struct LogBus {
    ring: Arc<LogRing>,
    seq: Arc<AtomicU64>,
}

impl LogBus {
    fn new() -> Self {
        Self {
            ring: Arc::new(LogRing::new()),
            seq: Arc::new(AtomicU64::new(1)),
        }
    }
    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }

    /// 追加一条日志：截断时先写入独立取号的 marker，再写入真实文本。
    /// 返回真实文本对应的 seq 与文本，供事件发布使用，保证所有 seq 唯一。
    /// 取号与插入在同一临界区，双流（stdout/stderr）并发时序号仍与插入顺序一致。
    fn append(&self, stream: OutputStream, text: String, truncated: bool) -> (u64, String) {
        const MARKER: &str = "[日志已截断，仅保留最近 2 MiB]";
        let len = text.len();
        let mut entries = self.ring.entries.lock().unwrap();
        let mut bytes = self.ring.bytes.lock().unwrap();
        let mut trimmed = false;
        while *bytes + len > LOG_RING_BYTES && !entries.is_empty() {
            if let Some(old) = entries.pop_front() {
                *bytes = bytes.saturating_sub(old.text.len());
            }
            trimmed = true;
        }
        if trimmed {
            let marker_seq = self.next_seq();
            entries.push_back(LogEntry {
                seq: marker_seq,
                stream,
                text: MARKER.to_string(),
                truncated: true,
            });
            *bytes += MARKER.len();
            let text_seq = self.next_seq();
            entries.push_back(LogEntry {
                seq: text_seq,
                stream,
                text: text.clone(),
                truncated,
            });
            *bytes += len;
            (text_seq, text)
        } else {
            let seq = self.next_seq();
            entries.push_back(LogEntry {
                seq,
                stream,
                text: text.clone(),
                truncated,
            });
            *bytes += len;
            (seq, text)
        }
    }
}

struct LogRing {
    entries: Mutex<VecDeque<LogEntry>>,
    bytes: Mutex<usize>,
}

impl LogRing {
    fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            bytes: Mutex::new(0),
        }
    }

    fn page(&self, after_seq: u64) -> LogPage {
        let entries = self.entries.lock().unwrap();
        let filtered: Vec<LogEntry> = entries
            .iter()
            .filter(|e| e.seq > after_seq)
            .cloned()
            .collect();
        let next_seq = filtered.last().map(|e| e.seq + 1).unwrap_or(after_seq + 1);
        LogPage {
            entries: filtered,
            next_seq,
        }
    }
}

/// 运行资源（Job + 进程），由协调线程独占。
struct RunResources {
    job: JobHandle,
    process: SuspendedProcess,
}

/// 最近终态记录：保留到应用退出。
struct RecentRun {
    run_id: String,
    snapshot: RunSnapshot,
    bus: LogBus,
    config: NormalizedRunConfig,
}

/// 清理表条目：停止超时后仍需重试终止。
struct CleanupEntry {
    run_id: String,
    project_key: String,
    snapshot: RunSnapshot,
    bus: LogBus,
    config: NormalizedRunConfig,
    resources: Option<RunResources>,
}

struct ActiveRun {
    run_id: String,
    project_key: String,
    project_id: String,
    config: NormalizedRunConfig,
    state: RunState,
    pid: Option<u32>,
    started_at: Option<i64>,
    exit_code: Option<u32>,
    error_code: Option<String>,
    error_message: Option<String>,
    stop_reason: Option<String>,
    stop_requested: Arc<AtomicBool>,
    bus: LogBus,
    /// Job 句柄共享引用：供端口归属校验在运行期间查询进程列表。
    job: Option<JobHandle>,
}

impl ActiveRun {
    fn snapshot(&self) -> RunSnapshot {
        RunSnapshot {
            run_id: self.run_id.clone(),
            project_id: self.project_id.clone(),
            state: self.state,
            cwd: self.config.cwd.clone(),
            pid: self.pid,
            started_at: self.started_at,
            exit_code: self.exit_code,
            error_code: self.error_code.clone(),
            error_message: self.error_message.clone(),
            stop_reason: self.stop_reason.clone(),
            summary: redacted_summary(&self.config),
        }
    }
}

struct RuntimeInner {
    runs: HashMap<String, ActiveRun>,
    project_keys: HashMap<String, String>,
    recent: HashMap<String, RecentRun>,
    cleanup: Vec<CleanupEntry>,
}

/// 运行管理器：项目级单实例 + 生命周期 + 日志 + 运行历史。
pub struct RuntimeManager {
    api: Arc<dyn Win32ProcessApi>,
    sink: Arc<dyn RunEventSink>,
    history: Arc<dyn RunHistoryStore>,
    session: ConfirmationSession,
    start_lock: Mutex<()>,
    inner: Mutex<RuntimeInner>,
}

impl RuntimeManager {
    pub fn new(
        api: Arc<dyn Win32ProcessApi>,
        sink: Arc<dyn RunEventSink>,
        history: Arc<dyn RunHistoryStore>,
    ) -> Self {
        Self {
            api,
            sink,
            history,
            session: ConfirmationSession::default(),
            start_lock: Mutex::new(()),
            inner: Mutex::new(RuntimeInner {
                runs: HashMap::new(),
                project_keys: HashMap::new(),
                recent: HashMap::new(),
                cleanup: Vec::new(),
            }),
        }
    }

    /// 启动时加载已退出历史到内存（仅终态，不误报为运行中）。
    pub fn load_history(&self) {
        let entries = self.history.list(500);
        let mut inner = self.inner.lock().unwrap();
        for entry in entries {
            if inner.runs.contains_key(&entry.run_id)
                || inner.cleanup.iter().any(|c| c.run_id == entry.run_id)
                || inner.recent.contains_key(&entry.project_key)
            {
                continue;
            }
            let snap = entry.to_snapshot();
            inner.recent.insert(
                entry.project_key.clone(),
                RecentRun {
                    run_id: entry.run_id,
                    snapshot: snap,
                    bus: LogBus::new(),
                    config: NormalizedRunConfig {
                        version: 1,
                        project_id: String::new(),
                        executable: String::new(),
                        args: Vec::new(),
                        cwd: String::new(),
                        env_overrides: Vec::new(),
                        expected_port: None,
                        preview_scheme: "http".to_string(),
                    },
                },
            );
        }
    }

    /// 运行列表：活动实例 + 清理中 + （可选）已退出记录（内存 recent + 落库历史）。
    pub fn list_runs(&self, include_exited: bool) -> Vec<RunSnapshot> {
        let mut out: Vec<RunSnapshot> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        {
            let inner = self.inner.lock().unwrap();
            for run in inner.runs.values() {
                let snap = run.snapshot();
                seen.insert(snap.run_id.clone());
                out.push(snap);
            }
            for c in &inner.cleanup {
                if !seen.contains(&c.run_id) {
                    seen.insert(c.run_id.clone());
                    out.push(c.snapshot.clone());
                }
            }
            if include_exited {
                for r in inner.recent.values() {
                    if !seen.contains(&r.run_id) {
                        seen.insert(r.run_id.clone());
                        out.push(r.snapshot.clone());
                    }
                }
            }
        }
        if include_exited {
            for h in self.history.list(500) {
                if !seen.contains(&h.run_id) {
                    seen.insert(h.run_id.clone());
                    out.push(h.to_snapshot());
                }
            }
        }
        out.sort_by(|a, b| {
            let sa = a.started_at.unwrap_or(0);
            let sb = b.started_at.unwrap_or(0);
            sb.cmp(&sa)
        });
        out
    }

    /// 记录已退出运行到历史（忽略存储错误，不阻塞运行状态机）。
    fn record_history(&self, snap: &RunSnapshot, project_key: &str) {
        let _ = self.history.record(snap, project_key);
    }

    // ---- 确认协议委托 ----

    pub fn prepare_confirmation(
        &self,
        config: &RunConfig,
        project_root: &Path,
    ) -> Result<ConfirmationPreview, RuntimeError> {
        self.session
            .prepare(config, project_root)
            .map_err(|e| RuntimeError::new(e.code, e.message))
    }

    pub fn confirm_config(&self, id: &str) -> Result<ConfirmationGrant, RuntimeError> {
        self.session
            .confirm(id)
            .map_err(|e| RuntimeError::new(e.code, e.message))
    }

    pub fn sweep_expired(&self) {
        self.session.sweep_expired();
    }

    // ---- 启动 ----

    pub fn start(
        self: &Arc<Self>,
        config: &RunConfig,
        project_root: &Path,
        confirmation_hash: &str,
    ) -> Result<RunSnapshot, RuntimeError> {
        let normalized = self
            .session
            .verify(config, project_root, confirmation_hash)
            .map_err(|e| RuntimeError::new(e.code, e.message))?;
        self.start_normalized(normalized)
    }

    /// 用已保存的规范化快照启动（重启路径，无需重新确认）。
    fn start_normalized(
        self: &Arc<Self>,
        normalized: NormalizedRunConfig,
    ) -> Result<RunSnapshot, RuntimeError> {
        let project_key = canonical_key(Path::new(&normalized.cwd))
            .map(|p| p.to_string_lossy().to_string())
            .ok_or_else(|| RuntimeError::new("invalid_working_directory", "项目根目录无法解析"))?;

        let run_id = new_run_id();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let bus = LogBus::new();
        {
            let _guard = self.start_lock.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            if inner.project_keys.contains_key(&project_key) {
                return Err(RuntimeError::new(
                    "project_already_running",
                    "该项目已有运行实例或正在清理，无法重复启动",
                ));
            }
            inner.runs.insert(
                run_id.clone(),
                ActiveRun {
                    run_id: run_id.clone(),
                    project_key: project_key.clone(),
                    project_id: normalized.project_id.clone(),
                    config: normalized.clone(),
                    state: RunState::Starting,
                    pid: None,
                    started_at: None,
                    exit_code: None,
                    error_code: None,
                    error_message: None,
                    stop_reason: None,
                    stop_requested: stop_flag.clone(),
                    bus: bus.clone(),
                    job: None,
                },
            );
            inner.project_keys.insert(project_key, run_id.clone());
        }

        let started_at = now_unix();
        let spawned = self.spawn_process(&normalized, &stop_flag);
        match spawned {
            Ok(resources) => {
                if stop_flag.load(Ordering::SeqCst) {
                    // 恢复前再次检查：挂起期间请求停止 → 直接终止，不恢复主线程。
                    let _ = self.api.terminate_job(&resources.job);
                    let _ = self.api.terminate_process(&resources.process);
                    let _ = self.api.wait_process_exit(&resources.process, STOP_TIMEOUT);
                    return Ok(self.finalize_stopped_during_start(&run_id));
                }
                let mut inner = self.inner.lock().unwrap();
                if let Some(run) = inner.runs.get_mut(&run_id) {
                    run.state = RunState::Running;
                    run.pid = Some(resources.process.pid);
                    run.started_at = Some(started_at);
                }
                drop(inner);
                self.emit_status_for(&run_id);
                self.spawn_coordinator(run_id.clone(), resources);
                Ok(self.get_run(&run_id).unwrap())
            }
            Err(e) if e.code == "stopped_during_start" => {
                Ok(self.finalize_stopped_during_start(&run_id))
            }
            Err(e) => {
                let mut inner = self.inner.lock().unwrap();
                let run = inner.runs.remove(&run_id);
                if let Some(run) = run {
                    inner.project_keys.remove(&run.project_key);
                    let mut snap = run.snapshot();
                    snap.state = RunState::Failed;
                    snap.error_code = Some(e.code.clone());
                    snap.error_message = Some(e.message.clone());
                    self.emit_error_payload(&snap);
                    self.sink.emit_status(&StatusPayload {
                        run_id: snap.run_id.clone(),
                        project_id: snap.project_id.clone(),
                        state: RunState::Failed,
                        pid: snap.pid,
                        exit_code: snap.exit_code,
                        error_code: snap.error_code.clone(),
                        error_message: snap.error_message.clone(),
                    });
                    inner.recent.insert(
                        run.project_key.clone(),
                        RecentRun {
                            run_id: run.run_id.clone(),
                            snapshot: snap.clone(),
                            bus: run.bus.clone(),
                            config: run.config.clone(),
                        },
                    );
                    self.record_history(&snap, &run.project_key);
                }
                Err(e)
            }
        }
    }

    fn finalize_stopped_during_start(&self, run_id: &str) -> RunSnapshot {
        let mut inner = self.inner.lock().unwrap();
        let run = inner.runs.remove(run_id);
        if let Some(run) = run {
            inner.project_keys.remove(&run.project_key);
            let mut snap = run.snapshot();
            snap.state = RunState::Exited;
            snap.exit_code = Some(0);
            snap.stop_reason = Some("user".into());
            let payload = ExitedPayload {
                run_id: snap.run_id.clone(),
                project_id: snap.project_id.clone(),
                exit_code: 0,
                stop_reason: Some("user".into()),
            };
            self.sink.emit_exited(&payload);
            inner.recent.insert(
                run.project_key.clone(),
                RecentRun {
                    run_id: run.run_id.clone(),
                    snapshot: snap.clone(),
                    bus: run.bus.clone(),
                    config: run.config.clone(),
                },
            );
            self.record_history(&snap, &run.project_key);
            snap
        } else {
            RunSnapshot {
                run_id: run_id.to_string(),
                project_id: String::new(),
                state: RunState::Exited,
                cwd: String::new(),
                pid: None,
                started_at: None,
                exit_code: Some(0),
                error_code: None,
                error_message: None,
                stop_reason: Some("user".into()),
                summary: serde_json::json!({}),
            }
        }
    }

    /// 创建 Job、挂起进程、加入 Job、恢复线程；任何失败分支都不恢复未受控进程。
    fn spawn_process(
        &self,
        normalized: &NormalizedRunConfig,
        stop_flag: &AtomicBool,
    ) -> Result<RunResources, RuntimeError> {
        let job = self
            .api
            .create_job()
            .map_err(|e| RuntimeError::from_api("process_containment_failed", &e))?;
        if let Err(e) = self.api.set_job_kill_on_close(&job) {
            return Err(RuntimeError::from_api("process_containment_failed", &e));
        }
        let spec = ProcessSpec {
            executable: normalized.executable.clone(),
            args: normalized.args.clone(),
            cwd: Some(PathBuf::from(&normalized.cwd)),
            env_overrides: normalized.env_overrides.clone(),
        };
        let process = match self.api.create_process_suspended(&spec) {
            Ok(p) => p,
            Err(e) => return Err(RuntimeError::from_api("process_spawn_failed", &e)),
        };
        if stop_flag.load(Ordering::SeqCst) {
            let _ = self.api.terminate_process(&process);
            let _ = self.api.wait_process_exit(&process, STOP_TIMEOUT);
            return Err(RuntimeError::new(
                "stopped_during_start",
                "启动过程中已请求停止",
            ));
        }
        if let Err(e) = self.api.assign_process_to_job(&job, &process) {
            let _ = self.api.terminate_process(&process);
            let _ = self.api.wait_process_exit(&process, STOP_TIMEOUT);
            return Err(RuntimeError::from_api("process_containment_failed", &e));
        }
        if stop_flag.load(Ordering::SeqCst) {
            // 加入 Job 后、恢复前再次检查。
            let _ = self.api.terminate_job(&job);
            let _ = self.api.wait_process_exit(&process, STOP_TIMEOUT);
            return Err(RuntimeError::new(
                "stopped_during_start",
                "启动过程中已请求停止",
            ));
        }
        if let Err(e) = self.api.resume_thread(&process) {
            let _ = self.api.terminate_job(&job);
            let _ = self.api.wait_process_exit(&process, STOP_TIMEOUT);
            return Err(RuntimeError::from_api("process_spawn_failed", &e));
        }
        Ok(RunResources { job, process })
    }

    /// 启动输出读取线程与退出协调线程。
    fn spawn_coordinator(self: &Arc<Self>, run_id: String, resources: RunResources) {
        let (bus, project_id) = {
            let inner = self.inner.lock().unwrap();
            let run = inner.runs.get(&run_id);
            (
                run.map(|r| r.bus.clone()),
                run.map(|r| r.project_id.clone()).unwrap_or_default(),
            )
        };
        let Some(bus) = bus else { return };
        let stop_flag = {
            let inner = self.inner.lock().unwrap();
            inner.runs.get(&run_id).map(|r| r.stop_requested.clone())
        };
        let Some(stop_flag) = stop_flag else { return };

        let mut process = resources.process;
        // 保留 Job 句柄共享引用供预览服务校验端口归属。
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(run) = inner.runs.get_mut(&run_id) {
                run.job = Some(resources.job.clone());
            }
        }
        // 收集输出读取线程句柄：终态前必须排空，避免最后一批日志晚于 exited 事件。
        let mut reader_handles: Vec<std::thread::JoinHandle<()>> = Vec::new();
        if let Some(reader) = process.stdout.take() {
            let bus = bus.clone();
            let sink = self.sink.clone();
            let rid = run_id.clone();
            let pid = project_id.clone();
            reader_handles.push(std::thread::spawn(move || {
                read_stream(reader, OutputStream::Stdout, rid, pid, bus, sink);
            }));
        }
        if let Some(reader) = process.stderr.take() {
            let bus = bus.clone();
            let sink = self.sink.clone();
            let rid = run_id.clone();
            let pid = project_id.clone();
            reader_handles.push(std::thread::spawn(move || {
                read_stream(reader, OutputStream::Stderr, rid, pid, bus, sink);
            }));
        }
        let readers = Arc::new(Mutex::new(reader_handles));

        let manager = self.clone();
        let job = resources.job;
        std::thread::spawn(move || {
            manager.coordinate(run_id, job, process, stop_flag, bus, readers);
        });
    }

    /// 等待输出读取线程排空。仅进程树已退出后调用：此时管道写端已关闭，
    /// read 会立即返回，join 不会阻塞。
    fn drain_readers(&self, readers: &Arc<Mutex<Vec<std::thread::JoinHandle<()>>>>) {
        let handles = std::mem::take(&mut *readers.lock().unwrap());
        for handle in handles {
            let _ = handle.join();
        }
    }

    /// 协调线程主循环：等待退出；收到停止请求后终止 Job 并等待，最多 5 秒。
    ///
    /// 主进程退出不等于运行结束：Job 内可能仍有活动子进程（如 npm → node），
    /// 必须等整个进程树空闲后才进入终态，避免误报退出并触发 KILL_ON_JOB_CLOSE。
    fn coordinate(
        self: &Arc<Self>,
        run_id: String,
        job: JobHandle,
        process: SuspendedProcess,
        stop_flag: Arc<AtomicBool>,
        bus: LogBus,
        readers: Arc<Mutex<Vec<std::thread::JoinHandle<()>>>>,
    ) {
        let mut stop_started: Option<Instant> = None;
        // 主进程退出码；None 表示主进程尚未退出。
        let mut main_exit: Option<u32> = None;
        // Job 活动进程数查询连续失败计数（防御性上限，避免死循环）。
        let mut query_failures: u32 = 0;
        loop {
            if stop_started.is_none() && stop_flag.load(Ordering::SeqCst) {
                stop_started = Some(Instant::now());
                let _ = self.api.terminate_job(&job);
            }
            // 第一层：等待主进程退出。
            if main_exit.is_none() {
                match self
                    .api
                    .wait_process_exit(&process, Duration::from_millis(200))
                {
                    WaitResult::Exited { exit_code } => {
                        main_exit = Some(exit_code);
                    }
                    WaitResult::Timeout => {
                        if let Some(started) = stop_started {
                            if started.elapsed() >= STOP_TIMEOUT {
                                self.finalize_stop_timeout(run_id, job, process, bus);
                                return;
                            }
                        }
                        continue;
                    }
                    WaitResult::Failed(e) => {
                        self.finalize_error(run_id, &e, job, process, bus);
                        return;
                    }
                }
            }
            // 第二层：主进程已退出，等待整个进程树空闲。
            match self.api.query_job_process_count(&job) {
                Ok(0) => {
                    let exit_code = main_exit.unwrap_or(0);
                    let stop_reason = if stop_flag.load(Ordering::SeqCst) {
                        Some("user".to_string())
                    } else {
                        None
                    };
                    // 先排空输出线程，再发布终态，保证最后一批日志先于 exited 事件。
                    self.drain_readers(&readers);
                    self.finalize_exited(run_id, exit_code, stop_reason, job, process, bus);
                    return;
                }
                Ok(_) => {
                    query_failures = 0;
                }
                Err(_) => {
                    query_failures += 1;
                    // 持续无法查询 Job（句柄失效等）时保守按空闲处理，避免永久挂起。
                    if query_failures >= 25 {
                        let exit_code = main_exit.unwrap_or(0);
                        let stop_reason = if stop_flag.load(Ordering::SeqCst) {
                            Some("user".to_string())
                        } else {
                            None
                        };
                        self.drain_readers(&readers);
                        self.finalize_exited(run_id, exit_code, stop_reason, job, process, bus);
                        return;
                    }
                }
            }
            if let Some(started) = stop_started {
                if started.elapsed() >= STOP_TIMEOUT {
                    self.finalize_stop_timeout(run_id, job, process, bus);
                    return;
                }
            }
        }
    }

    fn finalize_exited(
        &self,
        run_id: String,
        exit_code: u32,
        stop_reason: Option<String>,
        job: JobHandle,
        process: SuspendedProcess,
        bus: LogBus,
    ) {
        let mut inner = self.inner.lock().unwrap();
        let run = inner.runs.remove(&run_id);
        if let Some(mut run) = run {
            run.state = RunState::Exited;
            run.exit_code = Some(exit_code);
            run.stop_reason = stop_reason.clone();
            if exit_code != 0 {
                run.error_code = Some("process_non_zero_exit".into());
                run.error_message = Some(format!("进程以退出码 {exit_code} 结束"));
            }
            inner.project_keys.remove(&run.project_key);
            let snap = run.snapshot();
            drop(job);
            drop(process);
            inner.recent.insert(
                run.project_key.clone(),
                RecentRun {
                    run_id,
                    snapshot: snap.clone(),
                    bus,
                    config: run.config.clone(),
                },
            );
            self.record_history(&snap, &run.project_key);
            self.emit_status_payload(&snap);
            self.sink.emit_exited(&ExitedPayload {
                run_id: snap.run_id.clone(),
                project_id: snap.project_id.clone(),
                exit_code,
                stop_reason,
            });
        } else {
            drop(job);
            drop(process);
        }
    }

    fn finalize_stop_timeout(
        &self,
        run_id: String,
        job: JobHandle,
        process: SuspendedProcess,
        bus: LogBus,
    ) {
        let mut inner = self.inner.lock().unwrap();
        let run = inner.runs.remove(&run_id);
        if let Some(mut run) = run {
            run.state = RunState::Failed;
            run.error_code = Some("process_stop_timeout".into());
            run.error_message = Some("进程树未能在 5 秒内退出，已保留清理任务".into());
            let snap = run.snapshot();
            self.emit_error_payload(&snap);
            self.sink.emit_status(&StatusPayload {
                run_id: snap.run_id.clone(),
                project_id: snap.project_id.clone(),
                state: RunState::Failed,
                pid: snap.pid,
                exit_code: snap.exit_code,
                error_code: snap.error_code.clone(),
                error_message: snap.error_message.clone(),
            });
            // 项目占位保留，禁止新的启动/重启。
            inner.cleanup.push(CleanupEntry {
                run_id,
                project_key: run.project_key.clone(),
                snapshot: snap,
                bus,
                config: run.config.clone(),
                resources: Some(RunResources { job, process }),
            });
        } else {
            drop(job);
            drop(process);
        }
    }

    fn finalize_error(
        &self,
        run_id: String,
        e: &ProcessApiError,
        job: JobHandle,
        process: SuspendedProcess,
        bus: LogBus,
    ) {
        let mut inner = self.inner.lock().unwrap();
        let run = inner.runs.remove(&run_id);
        if let Some(mut run) = run {
            run.state = RunState::Failed;
            run.error_code = Some("process_containment_failed".into());
            run.error_message = Some(e.message.clone());
            inner.project_keys.remove(&run.project_key);
            let snap = run.snapshot();
            drop(job);
            drop(process);
            self.emit_error_payload(&snap);
            self.sink.emit_status(&StatusPayload {
                run_id: snap.run_id.clone(),
                project_id: snap.project_id.clone(),
                state: RunState::Failed,
                pid: snap.pid,
                exit_code: snap.exit_code,
                error_code: snap.error_code.clone(),
                error_message: snap.error_message.clone(),
            });
            inner.recent.insert(
                run.project_key.clone(),
                RecentRun {
                    run_id,
                    snapshot: snap.clone(),
                    bus,
                    config: run.config.clone(),
                },
            );
            self.record_history(&snap, &run.project_key);
        } else {
            drop(job);
            drop(process);
        }
    }

    // ---- 停止 ----

    pub fn stop(&self, run_id: &str) -> Result<RunSnapshot, RuntimeError> {
        let snap;
        {
            let mut inner = self.inner.lock().unwrap();
            let Some(run) = inner.runs.get_mut(run_id) else {
                if let Some(r) = inner.recent.values().find(|r| r.run_id == run_id) {
                    return Ok(r.snapshot.clone());
                }
                return Err(RuntimeError::new("run_not_found", "运行实例不存在或已结束"));
            };
            if run.state == RunState::Stopping || run.state.is_terminal() {
                return Ok(run.snapshot());
            }
            run.state = RunState::Stopping;
            run.stop_requested.store(true, Ordering::SeqCst);
            snap = run.snapshot();
        }
        self.emit_status_payload(&snap);
        Ok(snap)
    }

    // ---- 重启 ----

    pub fn restart(self: &Arc<Self>, run_id: &str) -> Result<RunSnapshot, RuntimeError> {
        let config = {
            let inner = self.inner.lock().unwrap();
            if let Some(run) = inner.runs.get(run_id) {
                if run.state.is_terminal() {
                    None
                } else {
                    Some(run.config.clone())
                }
            } else if let Some(c) = inner.cleanup.iter().find(|c| c.run_id == run_id) {
                let _ = c;
                return Err(RuntimeError::new(
                    "project_already_running",
                    "旧进程仍在清理中，无法重启",
                ));
            } else if let Some(r) = inner.recent.values().find(|r| r.run_id == run_id) {
                Some(r.config.clone())
            } else {
                return Err(RuntimeError::new("run_not_found", "运行实例不存在"));
            }
        };
        let Some(config) = config else {
            return Err(RuntimeError::new("run_not_found", "运行实例不存在或已结束"));
        };
        // 旧实例若仍在运行：请求停止并等待终态。
        if let Some(snap) = self.get_run(run_id) {
            if !snap.state.is_terminal() {
                let _ = self.stop(run_id);
                let deadline = Instant::now() + STOP_TIMEOUT + Duration::from_secs(1);
                loop {
                    let current = self.get_run(run_id);
                    let terminal = current.map(|s| s.state.is_terminal()).unwrap_or(true);
                    if terminal || Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                let after = self.get_run(run_id);
                if let Some(after) = after {
                    if !after.state.is_terminal() {
                        return Err(RuntimeError::new(
                            "process_stop_timeout",
                            "旧进程未能终止，重启取消",
                        ));
                    }
                }
            }
        }
        self.start_normalized(config)
    }

    // ---- 查询 ----

    pub fn get_run(&self, run_id: &str) -> Option<RunSnapshot> {
        let inner = self.inner.lock().unwrap();
        if let Some(run) = inner.runs.get(run_id) {
            return Some(run.snapshot());
        }
        if let Some(c) = inner.cleanup.iter().find(|c| c.run_id == run_id) {
            return Some(c.snapshot.clone());
        }
        inner
            .recent
            .values()
            .find(|r| r.run_id == run_id)
            .map(|r| r.snapshot.clone())
    }

    pub fn get_run_by_project_key(&self, project_key: &str) -> Option<RunSnapshot> {
        let inner = self.inner.lock().unwrap();
        if let Some(run_id) = inner.project_keys.get(project_key) {
            if let Some(run) = inner.runs.get(run_id) {
                return Some(run.snapshot());
            }
        }
        inner.recent.get(project_key).map(|r| r.snapshot.clone())
    }

    /// 按 project_id 查询最近运行（活动优先，含子项目运行；无活动时回退最近终态）。
    /// 多个活动运行（如前后端子项目并行）取启动时间最近的一个。
    pub fn get_run_by_project_id(&self, project_id: &str) -> Option<RunSnapshot> {
        let mut out = self.list_runs(true);
        out.retain(|s| s.project_id == project_id);
        out.first().cloned()
    }

    pub fn get_logs(&self, run_id: &str, after_seq: u64) -> Result<LogPage, RuntimeError> {
        let inner = self.inner.lock().unwrap();
        let bus = if let Some(run) = inner.runs.get(run_id) {
            Some(run.bus.clone())
        } else if let Some(c) = inner.cleanup.iter().find(|c| c.run_id == run_id) {
            Some(c.bus.clone())
        } else {
            inner
                .recent
                .values()
                .find(|r| r.run_id == run_id)
                .map(|r| r.bus.clone())
        };
        let Some(bus) = bus else {
            return Err(RuntimeError::new("run_not_found", "运行实例不存在或已淘汰"));
        };
        Ok(bus.ring.page(after_seq))
    }

    // ---- 预览支持 ----

    /// 返回活动运行的预览上下文：快照字段、拼接日志、Job 句柄。
    /// 仅活动运行（starting/running）可预览；终态返回 None。
    pub fn preview_context(&self, run_id: &str) -> Option<PreviewContext> {
        let inner = self.inner.lock().unwrap();
        let run = inner.runs.get(run_id)?;
        if run.state.is_terminal() {
            return None;
        }
        let log_text = {
            let page = run.bus.ring.page(0);
            page.entries
                .iter()
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>()
                .join("")
        };
        Some(PreviewContext {
            run_id: run.run_id.clone(),
            project_id: run.project_id.clone(),
            expected_port: run.config.expected_port,
            preview_scheme: run.config.preview_scheme.clone(),
            args: run.config.args.clone(),
            log_text,
            job: run.job.clone(),
        })
    }

    /// 查询 Job Object 内进程 PID（端口归属校验）。
    pub fn job_process_ids(&self, job: &JobHandle) -> Result<Vec<u32>, ProcessApiError> {
        self.api.query_job_process_ids(job)
    }

    /// 发布预览就绪事件。
    pub fn emit_preview_ready(&self, target: &PreviewTarget) {
        self.sink.emit_preview(target);
    }

    // ---- 清理 ----

    /// 重试终止停止超时的运行；确认退出后释放项目占位。
    pub fn cleanup_tick(&self) {
        let mut done: Vec<(String, String, RunSnapshot, LogBus, NormalizedRunConfig)> = Vec::new();
        {
            let mut inner = self.inner.lock().unwrap();
            let mut remaining: Vec<CleanupEntry> = Vec::new();
            for entry in inner.cleanup.drain(..) {
                let exited = match entry.resources.as_ref() {
                    Some(r) => match self
                        .api
                        .wait_process_exit(&r.process, Duration::from_millis(50))
                    {
                        WaitResult::Exited { .. } => true,
                        WaitResult::Timeout => {
                            let _ = self.api.terminate_job(&r.job);
                            let _ = self
                                .api
                                .wait_process_exit(&r.process, Duration::from_millis(50));
                            false
                        }
                        WaitResult::Failed(_) => false,
                    },
                    None => true,
                };
                if exited {
                    let c = entry;
                    done.push((c.project_key, c.run_id, c.snapshot, c.bus, c.config));
                } else {
                    remaining.push(entry);
                }
            }
            inner.cleanup = remaining;
        }
        for (project_key, run_id, mut snapshot, bus, config) in done {
            if snapshot.exit_code.is_none() {
                snapshot.exit_code = Some(1);
            }
            snapshot.state = RunState::Exited;
            let mut inner = self.inner.lock().unwrap();
            inner.project_keys.remove(&project_key);
            inner.recent.insert(
                project_key.clone(),
                RecentRun {
                    run_id,
                    snapshot: snapshot.clone(),
                    bus,
                    config,
                },
            );
            drop(inner);
            self.record_history(&snapshot, &project_key);
            self.emit_status_payload(&snapshot);
            self.sink.emit_exited(&ExitedPayload {
                run_id: snapshot.run_id.clone(),
                project_id: snapshot.project_id.clone(),
                exit_code: snapshot.exit_code.unwrap_or(1),
                stop_reason: Some("user".into()),
            });
        }
    }

    /// 应用正常退出：终止全部活动运行，总等待上限 5 秒。
    pub fn shutdown_all(&self) {
        let started = Instant::now();
        let run_ids: Vec<String> = {
            let inner = self.inner.lock().unwrap();
            inner.runs.keys().cloned().collect()
        };
        for run_id in run_ids {
            let _ = self.stop(&run_id);
        }
        let deadline = started + STOP_TIMEOUT;
        loop {
            let all_done = {
                let inner = self.inner.lock().unwrap();
                inner.runs.is_empty() && inner.cleanup.is_empty()
            };
            if all_done || Instant::now() >= deadline {
                break;
            }
            self.cleanup_tick();
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    // ---- 事件 ----

    fn emit_status_for(&self, run_id: &str) {
        if let Some(snap) = self.get_run(run_id) {
            self.emit_status_payload(&snap);
        }
    }

    fn emit_status_payload(&self, snap: &RunSnapshot) {
        self.sink.emit_status(&StatusPayload {
            run_id: snap.run_id.clone(),
            project_id: snap.project_id.clone(),
            state: snap.state,
            pid: snap.pid,
            exit_code: snap.exit_code,
            error_code: snap.error_code.clone(),
            error_message: snap.error_message.clone(),
        });
    }

    fn emit_error_payload(&self, snap: &RunSnapshot) {
        if let (Some(code), Some(message)) = (&snap.error_code, &snap.error_message) {
            self.sink.emit_error(&ErrorPayload {
                run_id: snap.run_id.clone(),
                project_id: snap.project_id.clone(),
                error_code: code.clone(),
                error_message: message.clone(),
            });
        }
    }
}

fn read_stream(
    mut reader: Box<dyn Read + Send>,
    stream: OutputStream,
    run_id: String,
    project_id: String,
    bus: LogBus,
    sink: Arc<dyn RunEventSink>,
) {
    let mut buf = vec![0u8; LOG_CHUNK_BYTES];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        let text = String::from_utf8_lossy(&buf[..n]).into_owned();
        let truncated = n == LOG_CHUNK_BYTES;
        let (seq, text) = bus.append(stream, text, truncated);
        sink.emit_output(&OutputPayload {
            run_id: run_id.clone(),
            project_id: project_id.clone(),
            seq,
            stream,
            text,
            truncated,
        });
    }
}

fn redacted_summary(config: &NormalizedRunConfig) -> serde_json::Value {
    let env: serde_json::Value = config
        .env_overrides
        .iter()
        .map(|(k, v)| {
            // 与确认摘要共用同一套敏感规则（含 CREDENTIAL/AUTH），避免脱敏不一致。
            let display = if is_sensitive_key(k) {
                v.as_ref().map(|value| {
                    if value.is_empty() {
                        "••••••".to_string()
                    } else {
                        format!("••••••({} 字符)", value.len())
                    }
                })
            } else {
                v.clone()
            };
            (k.clone(), display)
        })
        .collect();
    serde_json::json!({
        "executable": config.executable,
        "args": config.args,
        "cwd": config.cwd,
        "env": env,
        "expected_port": config.expected_port,
        "preview_scheme": config.preview_scheme,
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn new_run_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests;
