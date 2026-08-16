//! 运行与预览服务的共享测试支持（仅测试构建）。
//!
//! 提供注入式 `Win32ProcessApi` 替身、事件收集 sink 与常用构造辅助，
//! 供 `project_runtime` 与 `web_preview_service` 的测试模块复用。

#![cfg(test)]

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::services::process_api::{
    JobHandle, ProcessApiError, ProcessSpec, SuspendedProcess, WaitResult, Win32ProcessApi,
};
use crate::services::project_runtime::{
    ErrorPayload, ExitedPayload, OutputPayload, RunEventSink, RunSnapshot, RuntimeManager,
    StatusPayload,
};
use crate::services::run_confirmation::{canonical_key, RunConfig};
use crate::services::web_preview_service::PreviewTarget;

/// 注入式进程 API：按标志注入失败、按通道注入屏障、记录调用日志。
pub struct FakeApi {
    pub fail_create_job: AtomicBool,
    pub fail_kill_on_close: AtomicBool,
    pub fail_spawn: AtomicBool,
    pub fail_assign: AtomicBool,
    pub fail_resume: AtomicBool,
    pub fail_terminate: AtomicBool,
    /// terminate 是否使进程退出（默认 true；设为 false 模拟停止超时）。
    pub terminate_marks_exited: AtomicBool,
    pub exited: AtomicBool,
    pub exit_code: AtomicU32,
    /// 阻塞在 assign 内的屏障：Some(receiver) 时 assign 阻塞直到收到信号。
    pub assign_barrier: Mutex<Option<mpsc::Receiver<()>>>,
    pub stdout_data: Mutex<Vec<u8>>,
    pub stderr_data: Mutex<Vec<u8>>,
    pub calls: Mutex<Vec<String>>,
    /// Job 内进程 PID 列表（供 query_job_process_ids 返回）。
    pub job_pids: Mutex<Vec<u32>>,
}

impl FakeApi {
    pub fn new() -> Arc<Self> {
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
            job_pids: Mutex::new(vec![4242]),
        })
    }

    pub fn log(&self, call: &str) {
        self.calls.lock().unwrap().push(call.to_string());
    }

    pub fn called(&self, call: &str) -> bool {
        self.calls.lock().unwrap().iter().any(|c| c == call)
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    pub fn set_natural_exit(&self, code: u32) {
        self.exited.store(true, Ordering::SeqCst);
        self.exit_code.store(code, Ordering::SeqCst);
    }

    pub fn set_exited(&self, code: u32) {
        self.exited.store(true, Ordering::SeqCst);
        self.exit_code.store(code, Ordering::SeqCst);
    }

    pub fn spawn_stdout(&self, data: &[u8]) {
        *self.stdout_data.lock().unwrap() = data.to_vec();
    }

    pub fn spawn_stderr(&self, data: &[u8]) {
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

    fn query_job_process_ids(&self, _job: &JobHandle) -> Result<Vec<u32>, ProcessApiError> {
        self.log("query_job_process_ids");
        Ok(self.job_pids.lock().unwrap().clone())
    }
}

/// 收集事件的 sink。
#[derive(Default)]
pub struct TestSink {
    pub statuses: Mutex<Vec<StatusPayload>>,
    pub outputs: Mutex<Vec<OutputPayload>>,
    pub exited: Mutex<Vec<ExitedPayload>>,
    pub errors: Mutex<Vec<ErrorPayload>>,
    pub previews: Mutex<Vec<PreviewTarget>>,
}

impl TestSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub fn outputs(&self) -> Vec<OutputPayload> {
        self.outputs.lock().unwrap().clone()
    }
    pub fn exited(&self) -> Vec<ExitedPayload> {
        self.exited.lock().unwrap().clone()
    }
    pub fn previews(&self) -> Vec<PreviewTarget> {
        self.previews.lock().unwrap().clone()
    }
    pub fn wait_outputs(&self, expected: usize, timeout: Duration) -> Vec<OutputPayload> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let n = self.outputs.lock().unwrap().len();
            if n >= expected {
                return self.outputs();
            }
            if std::time::Instant::now() >= deadline {
                return self.outputs();
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    pub fn wait_previews(&self, timeout: Duration) -> Vec<PreviewTarget> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let n = self.previews.lock().unwrap().len();
            if n > 0 {
                return self.previews();
            }
            if std::time::Instant::now() >= deadline {
                return self.previews();
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
    fn emit_preview(&self, p: &PreviewTarget) {
        self.previews.lock().unwrap().push(p.clone());
    }
}

/// 创建临时项目根目录（含 bin/tool.exe）。
pub fn tmp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("nexus-run-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::write(root.join("bin").join("tool.exe"), "x").unwrap();
    root
}

pub fn base_config(root: &Path) -> RunConfig {
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

pub fn project_key(root: &Path) -> String {
    canonical_key(root).unwrap().to_string_lossy().to_string()
}

pub fn start_run(manager: &Arc<RuntimeManager>, root: &Path, config: &RunConfig) -> RunSnapshot {
    let preview = manager.prepare_confirmation(config, root).unwrap();
    let grant = manager.confirm_config(&preview.confirmation_id).unwrap();
    manager
        .start(config, root, &grant.confirmation_hash)
        .unwrap()
}

pub fn make_manager(api: Arc<FakeApi>, sink: Arc<TestSink>) -> Arc<RuntimeManager> {
    Arc::new(RuntimeManager::new(api, sink))
}

pub fn wait_until<F: Fn() -> bool>(f: F, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if f() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
