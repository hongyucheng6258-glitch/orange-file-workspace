//! 运行状态机测试：注入式进程 API + 事件收集 sink。

use super::*;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::AtomicU32;
use std::sync::mpsc;
use std::sync::Arc;

use crate::services::process_api::ProcessApiError;

/// 注入式进程 API：按标志注入失败、按通道注入屏障、记录调用日志。
struct FakeApi {
    fail_create_job: AtomicBool,
    fail_kill_on_close: AtomicBool,
    fail_spawn: AtomicBool,
    fail_assign: AtomicBool,
    fail_resume: AtomicBool,
    fail_terminate: AtomicBool,
    /// terminate 是否使进程退出（默认 true；设为 false 模拟停止超时）。
    terminate_marks_exited: AtomicBool,
    exited: AtomicBool,
    exit_code: AtomicU32,
    /// 阻塞在 assign 内的屏障：Some(receiver) 时 assign 阻塞直到收到信号。
    assign_barrier: Mutex<Option<mpsc::Receiver<()>>>,
    stdout_data: Mutex<Vec<u8>>,
    stderr_data: Mutex<Vec<u8>>,
    calls: Mutex<Vec<String>>,
}

impl FakeApi {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            fail_create_job: AtomicBool::new(false),
            fail_kill_on_close: AtomicBool::new(false),
            fail_spawn: AtomicBool::new(false),
            fail_assign: AtomicBool::new(false),
            fail_resume: AtomicBool::new(false),
            fail_terminate: AtomicBool::new(false),
            terminate_marks_exited: AtomicBool::new(true),
            exited: AtomicBool::new(false),
            exit_code: AtomicU32::new(0),
            assign_barrier: Mutex::new(None),
            stdout_data: Mutex::new(Vec::new()),
            stderr_data: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn log(&self, call: &str) {
        self.calls.lock().unwrap().push(call.to_string());
    }

    fn called(&self, call: &str) -> bool {
        self.calls.lock().unwrap().iter().any(|c| c == call)
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn set_natural_exit(&self, code: u32) {
        self.exited.store(true, Ordering::SeqCst);
        self.exit_code.store(code, Ordering::SeqCst);
    }

    fn set_exited(&self, code: u32) {
        self.exited.store(true, Ordering::SeqCst);
        self.exit_code.store(code, Ordering::SeqCst);
    }

    fn spawn_stdout(&self, data: &[u8]) {
        *self.stdout_data.lock().unwrap() = data.to_vec();
    }

    fn spawn_stderr(&self, data: &[u8]) {
        *self.stderr_data.lock().unwrap() = data.to_vec();
    }
}

impl Win32ProcessApi for FakeApi {
    fn create_job(&self) -> Result<JobHandle, ProcessApiError> {
        self.log("create_job");
        if self.fail_create_job.load(Ordering::SeqCst) {
            return Err(ProcessApiError::new("win32_error", "create_job failed"));
        }
        Ok(JobHandle::test_new())
    }

    fn set_job_kill_on_close(&self, _job: &JobHandle) -> Result<(), ProcessApiError> {
        self.log("set_job_kill_on_close");
        if self.fail_kill_on_close.load(Ordering::SeqCst) {
            return Err(ProcessApiError::new("win32_error", "set_kill failed"));
        }
        Ok(())
    }

    fn create_process_suspended(
        &self,
        _spec: &ProcessSpec,
    ) -> Result<SuspendedProcess, ProcessApiError> {
        self.log("create_process_suspended");
        if self.fail_spawn.load(Ordering::SeqCst) {
            return Err(ProcessApiError::new("win32_error", "spawn failed"));
        }
        let stdout = self.stdout_data.lock().unwrap().clone();
        let stderr = self.stderr_data.lock().unwrap().clone();
        Ok(SuspendedProcess::test_new(
            4242,
            Some(Box::new(Cursor::new(stdout))),
            Some(Box::new(Cursor::new(stderr))),
        ))
    }

    fn assign_process_to_job(
        &self,
        _job: &JobHandle,
        _process: &SuspendedProcess,
    ) -> Result<(), ProcessApiError> {
        self.log("assign_process_to_job");
        {
            let barrier = self.assign_barrier.lock().unwrap();
            if let Some(rx) = &*barrier {
                let _ = rx.recv();
            }
        }
        if self.fail_assign.load(Ordering::SeqCst) {
            return Err(ProcessApiError::new("win32_error", "assign failed"));
        }
        Ok(())
    }

    fn resume_thread(&self, _process: &SuspendedProcess) -> Result<(), ProcessApiError> {
        self.log("resume_thread");
        if self.fail_resume.load(Ordering::SeqCst) {
            return Err(ProcessApiError::new("win32_error", "resume failed"));
        }
        Ok(())
    }

    fn terminate_job(&self, _job: &JobHandle) -> Result<(), ProcessApiError> {
        self.log("terminate_job");
        if self.fail_terminate.load(Ordering::SeqCst) {
            return Err(ProcessApiError::new("win32_error", "terminate failed"));
        }
        if self.terminate_marks_exited.load(Ordering::SeqCst) {
            self.exited.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    fn terminate_process(&self, _process: &SuspendedProcess) -> Result<(), ProcessApiError> {
        self.log("terminate_process");
        if self.fail_terminate.load(Ordering::SeqCst) {
            return Err(ProcessApiError::new("win32_error", "terminate failed"));
        }
        self.exited.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn wait_process_exit(&self, _process: &SuspendedProcess, _timeout: Duration) -> WaitResult {
        self.log("wait_process_exit");
        if self.exited.load(Ordering::SeqCst) {
            WaitResult::Exited {
                exit_code: self.exit_code.load(Ordering::SeqCst),
            }
        } else {
            WaitResult::Timeout
        }
    }

    fn query_job_process_count(&self, _job: &JobHandle) -> Result<u32, ProcessApiError> {
        self.log("query_job_process_count");
        if self.exited.load(Ordering::SeqCst) {
            Ok(0)
        } else {
            Ok(1)
        }
    }
}

/// 收集事件的 sink。
#[derive(Default)]
struct TestSink {
    statuses: Mutex<Vec<StatusPayload>>,
    outputs: Mutex<Vec<OutputPayload>>,
    exited: Mutex<Vec<ExitedPayload>>,
    errors: Mutex<Vec<ErrorPayload>>,
}

impl TestSink {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    fn outputs(&self) -> Vec<OutputPayload> {
        self.outputs.lock().unwrap().clone()
    }
    fn exited(&self) -> Vec<ExitedPayload> {
        self.exited.lock().unwrap().clone()
    }
    fn wait_outputs(&self, expected: usize, timeout: Duration) -> Vec<OutputPayload> {
        let deadline = Instant::now() + timeout;
        loop {
            let n = self.outputs.lock().unwrap().len();
            if n >= expected {
                return self.outputs();
            }
            if Instant::now() >= deadline {
                return self.outputs();
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl RunEventSink for TestSink {
    fn emit_status(&self, p: &StatusPayload) {
        self.statuses.lock().unwrap().push(p.clone());
    }
    fn emit_output(&self, p: &OutputPayload) {
        self.outputs.lock().unwrap().push(p.clone());
    }
    fn emit_exited(&self, p: &ExitedPayload) {
        self.exited.lock().unwrap().push(p.clone());
    }
    fn emit_error(&self, p: &ErrorPayload) {
        self.errors.lock().unwrap().push(p.clone());
    }
}

fn tmp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("nexus-run-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::write(root.join("bin").join("tool.exe"), "x").unwrap();
    root
}

fn base_config(root: &Path) -> RunConfig {
    RunConfig {
        project_id: "p1".into(),
        executable: root
            .join("bin")
            .join("tool.exe")
            .to_string_lossy()
            .to_string(),
        args: vec!["--serve".into()],
        cwd: root.to_string_lossy().to_string(),
        env_overrides: HashMap::new(),
        expected_port: Some(3000),
        preview_scheme: "http".into(),
    }
}

fn project_key(root: &Path) -> String {
    canonical_key(root).unwrap().to_string_lossy().to_string()
}

fn start_run(manager: &Arc<RuntimeManager>, root: &Path, config: &RunConfig) -> RunSnapshot {
    let preview = manager.prepare_confirmation(config, root).unwrap();
    let grant = manager.confirm_config(&preview.confirmation_id).unwrap();
    manager
        .start(config, root, &grant.confirmation_hash)
        .unwrap()
}

fn make_manager(api: Arc<FakeApi>, sink: Arc<TestSink>) -> Arc<RuntimeManager> {
    Arc::new(RuntimeManager::new(api, sink))
}

fn wait_until<F: Fn() -> bool>(f: F, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if f() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

// ---- 基础生命周期 ----

#[test]
fn start_natural_exit_emits_single_terminal() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("nat");
    api.set_natural_exit(0);
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);
    assert_eq!(snap.state, RunState::Running);
    assert_eq!(snap.pid, Some(4242));
    assert!(wait_until(
        || {
            manager
                .get_run(&snap.run_id)
                .map(|s| s.state == RunState::Exited)
                .unwrap_or(false)
        },
        Duration::from_secs(3)
    ));
    let final_snap = manager.get_run(&snap.run_id).unwrap();
    assert_eq!(final_snap.state, RunState::Exited);
    assert_eq!(final_snap.exit_code, Some(0));
    // 终态后项目占位释放。
    assert!(manager
        .get_run_by_project_key(&project_key(&root))
        .is_some());
    assert!(manager
        .get_run_by_project_key(&project_key(&root))
        .unwrap()
        .state
        .is_terminal());
    // 仅一个退出事件。
    let exited = sink.exited();
    assert_eq!(exited.len(), 1);
    assert_eq!(exited[0].exit_code, 0);
    assert!(exited[0].stop_reason.is_none());
}

#[test]
fn nonzero_exit_reports_error_code() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("nonzero");
    api.set_natural_exit(7);
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);
    assert!(wait_until(
        || {
            manager
                .get_run(&snap.run_id)
                .map(|s| s.state == RunState::Exited)
                .unwrap_or(false)
        },
        Duration::from_secs(3)
    ));
    let final_snap = manager.get_run(&snap.run_id).unwrap();
    assert_eq!(final_snap.exit_code, Some(7));
    assert_eq!(
        final_snap.error_code.as_deref(),
        Some("process_non_zero_exit")
    );
}

#[test]
fn stop_marks_user_and_terminates() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("stop");
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);
    assert_eq!(snap.state, RunState::Running);
    let stopped = manager.stop(&snap.run_id).unwrap();
    assert_eq!(stopped.state, RunState::Stopping);
    // terminate_job 默认使进程退出。
    assert!(wait_until(
        || {
            manager
                .get_run(&snap.run_id)
                .map(|s| s.state == RunState::Exited)
                .unwrap_or(false)
        },
        Duration::from_secs(3)
    ));
    let final_snap = manager.get_run(&snap.run_id).unwrap();
    assert_eq!(final_snap.state, RunState::Exited);
    assert_eq!(final_snap.stop_reason.as_deref(), Some("user"));
    assert!(api.called("terminate_job"));
    // 重复停止幂等。
    assert!(manager.stop(&snap.run_id).is_ok());
    // 仅一个退出事件。
    assert_eq!(sink.exited().len(), 1);
}

#[test]
fn stop_timeout_retains_project_until_cleanup() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("stoptimeout");
    api.terminate_marks_exited.store(false, Ordering::SeqCst);
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);
    assert_eq!(snap.state, RunState::Running);
    let _ = manager.stop(&snap.run_id).unwrap();
    // 等待 5 秒停止超时。
    assert!(wait_until(
        || {
            manager
                .get_run(&snap.run_id)
                .map(|s| s.state == RunState::Failed)
                .unwrap_or(false)
        },
        Duration::from_secs(7)
    ));
    let failed = manager.get_run(&snap.run_id).unwrap();
    assert_eq!(failed.error_code.as_deref(), Some("process_stop_timeout"));
    // 项目占位保留 → 新启动被拒绝。
    let config2 = base_config(&root);
    let preview = manager.prepare_confirmation(&config2, &root).unwrap();
    let grant = manager.confirm_config(&preview.confirmation_id).unwrap();
    let err = manager
        .start(&config2, &root, &grant.confirmation_hash)
        .unwrap_err();
    assert_eq!(err.code, "project_already_running");
    // 进程退出后 cleanup 释放占位。
    api.set_exited(0);
    manager.cleanup_tick();
    let preview = manager.prepare_confirmation(&config2, &root).unwrap();
    let grant = manager.confirm_config(&preview.confirmation_id).unwrap();
    let snap2 = manager
        .start(&config2, &root, &grant.confirmation_hash)
        .unwrap();
    assert_eq!(snap2.state, RunState::Running);
    // 清理完成前旧 run 仍可查询。
    assert!(manager.get_run(&snap.run_id).is_some());
}

#[test]
fn concurrent_start_single_instance() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("concurrent");
    let config = base_config(&root);
    let preview = manager.prepare_confirmation(&config, &root).unwrap();
    let grant = manager.confirm_config(&preview.confirmation_id).unwrap();
    let hash = grant.confirmation_hash.clone();
    let m1 = manager.clone();
    let m2 = manager.clone();
    let c1 = config.clone();
    let c2 = config.clone();
    let r1 = root.clone();
    let r2 = root.clone();
    let h1 = hash.clone();
    let h2 = hash.clone();
    let t1 = std::thread::spawn(move || m1.start(&c1, &r1, &h1).is_ok());
    let t2 = std::thread::spawn(move || m2.start(&c2, &r2, &h2).is_ok());
    let ok_count = [t1.join().unwrap(), t2.join().unwrap()]
        .iter()
        .filter(|ok| **ok)
        .count();
    assert_eq!(ok_count, 1);
}

#[test]
fn restart_after_terminal_creates_new_run_id() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("restart");
    api.set_natural_exit(0);
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);
    assert!(wait_until(
        || {
            manager
                .get_run(&snap.run_id)
                .map(|s| s.state == RunState::Exited)
                .unwrap_or(false)
        },
        Duration::from_secs(3)
    ));
    // 重置进程状态，避免新运行立即退出。
    api.exited.store(false, Ordering::SeqCst);
    let restarted = manager.restart(&snap.run_id).unwrap();
    assert_ne!(restarted.run_id, snap.run_id);
    assert_eq!(restarted.state, RunState::Running);
    let _ = sink;
}

#[test]
fn restart_waits_for_stop_then_starts() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("restart-stop");
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);
    assert_eq!(snap.state, RunState::Running);
    // 停止请求后进程 300ms 内退出（默认 terminate_marks_exited=true）。
    let t = std::thread::spawn({
        let manager = manager.clone();
        let rid = snap.run_id.clone();
        move || manager.restart(&rid)
    });
    let result = t.join().unwrap().unwrap();
    assert_ne!(result.run_id, snap.run_id);
    assert_eq!(result.state, RunState::Running);
}

// ---- 启动期间停止 ----

#[test]
fn stop_during_start_never_resumes() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("stop-start");
    let config = base_config(&root);
    let preview = manager.prepare_confirmation(&config, &root).unwrap();
    let grant = manager.confirm_config(&preview.confirmation_id).unwrap();
    let hash = grant.confirmation_hash.clone();

    let (tx, rx) = mpsc::channel();
    *api.assign_barrier.lock().unwrap() = Some(rx);

    let manager2 = manager.clone();
    let c2 = config.clone();
    let r2 = root.clone();
    let h2 = hash.clone();
    let handle = std::thread::spawn(move || manager2.start(&c2, &r2, &h2));

    // 等待 run 注册为 Starting。
    let key = project_key(&root);
    assert!(wait_until(
        || manager.get_run_by_project_key(&key).is_some(),
        Duration::from_secs(3)
    ));
    let starting = manager.get_run_by_project_key(&key).unwrap();
    assert_eq!(starting.state, RunState::Starting);
    let run_id = starting.run_id.clone();
    let _ = manager.stop(&run_id).unwrap();
    tx.send(()).unwrap();
    let result = handle.join().unwrap().unwrap();
    assert_eq!(result.state, RunState::Exited);
    assert_eq!(result.stop_reason.as_deref(), Some("user"));
    // 从未恢复主线程。
    assert!(!api.called("resume_thread"));
    assert!(api.called("terminate_job"));
    // 项目占位已释放。
    assert!(manager.get_run_by_project_key(&key).is_some());
    assert!(manager
        .get_run_by_project_key(&key)
        .unwrap()
        .state
        .is_terminal());
}

// ---- 失败注入 ----

#[test]
fn spawn_failures_release_key_and_report_codes() {
    let cases: &[(&str, &dyn Fn(&FakeApi))] = &[
        ("create_job", &|api| {
            api.fail_create_job.store(true, Ordering::SeqCst)
        }),
        ("kill_on_close", &|api| {
            api.fail_kill_on_close.store(true, Ordering::SeqCst)
        }),
        ("spawn", &|api| api.fail_spawn.store(true, Ordering::SeqCst)),
        ("assign", &|api| {
            api.fail_assign.store(true, Ordering::SeqCst)
        }),
        ("resume", &|api| {
            api.fail_resume.store(true, Ordering::SeqCst)
        }),
    ];
    for (name, inject) in cases {
        let api = FakeApi::new();
        let sink = TestSink::new();
        let manager = make_manager(api.clone(), sink.clone());
        let root = tmp_root(name);
        inject(&api);
        let config = base_config(&root);
        let preview = manager.prepare_confirmation(&config, &root).unwrap();
        let grant = manager.confirm_config(&preview.confirmation_id).unwrap();
        let err = manager
            .start(&config, &root, &grant.confirmation_hash)
            .unwrap_err();
        assert!(
            matches!(
                err.code.as_str(),
                "process_containment_failed" | "process_spawn_failed"
            ),
            "{name}: unexpected code {}",
            err.code
        );
        // 失败后项目占位释放。
        let after = manager.get_run_by_project_key(&project_key(&root));
        assert!(after.is_some(), "{name}: 最近记录缺失");
        assert!(
            after.unwrap().state == RunState::Failed,
            "{name}: 应为 Failed"
        );
        // assign 失败时不恢复主线程。
        if *name == "assign" {
            assert!(!api.called("resume_thread"), "assign 失败后不应恢复主线程");
            assert!(api.called("terminate_process"));
        }
        if *name == "resume" {
            assert!(api.called("terminate_job"));
        }
    }
}

#[test]
fn assign_failure_never_resumes_thread() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("assign-fail");
    api.fail_assign.store(true, Ordering::SeqCst);
    let config = base_config(&root);
    let preview = manager.prepare_confirmation(&config, &root).unwrap();
    let grant = manager.confirm_config(&preview.confirmation_id).unwrap();
    let _ = manager
        .start(&config, &root, &grant.confirmation_hash)
        .unwrap_err();
    assert!(!api.called("resume_thread"));
    assert!(api.called("terminate_process"));
}

// ---- 日志 ----

#[test]
fn output_events_seq_monotonic_and_query_no_duplicates() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("logs");
    api.spawn_stdout(b"hello\nworld\n");
    api.spawn_stderr(b"warn: x\n");
    api.set_natural_exit(0);
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);
    let outputs = sink.wait_outputs(2, Duration::from_secs(3));
    assert!(outputs.len() >= 2, "输出事件不足: {}", outputs.len());
    let seqs: Vec<u64> = outputs.iter().map(|o| o.seq).collect();
    let mut sorted = seqs.clone();
    sorted.sort_unstable();
    assert_eq!(seqs, sorted, "seq 应单调递增");
    let page1 = manager.get_logs(&snap.run_id, 0).unwrap();
    let max_seq = page1.next_seq - 1;
    // 补发查询不重复。
    let page2 = manager.get_logs(&snap.run_id, max_seq).unwrap();
    assert!(page2.entries.is_empty());
    let streams: Vec<OutputStream> = page1.entries.iter().map(|e| e.stream).collect();
    assert!(streams.contains(&OutputStream::Stdout));
    assert!(streams.contains(&OutputStream::Stderr));
}

#[test]
fn log_ring_trims_oldest_with_marker() {
    let ring = LogRing::new();
    let mut seq = 1u64;
    // 每个事件 32 KiB，超过 2 MiB 上限。
    let chunk = "x".repeat(LOG_CHUNK_BYTES);
    for _ in 0..70 {
        ring.push(seq, OutputStream::Stdout, chunk.clone(), false);
        seq += 1;
    }
    let page = ring.page(0);
    assert!(page.entries.len() < 70, "应淘汰最旧内容");
    assert!(page
        .entries
        .iter()
        .any(|e| e.truncated && e.text.contains("截断")));
}

// ---- 事件唯一性 ----

#[test]
fn exactly_one_terminal_event_per_run() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("unique");
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);
    let _ = manager.stop(&snap.run_id).unwrap();
    assert!(wait_until(
        || {
            manager
                .get_run(&snap.run_id)
                .map(|s| s.state == RunState::Exited)
                .unwrap_or(false)
        },
        Duration::from_secs(3)
    ));
    assert_eq!(sink.exited().len(), 1);
}

// ---- 关闭 ----

#[test]
fn shutdown_all_terminates_running() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("shutdown");
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);
    assert_eq!(snap.state, RunState::Running);
    manager.shutdown_all();
    let after = manager.get_run(&snap.run_id).unwrap();
    assert!(after.state.is_terminal());
}
