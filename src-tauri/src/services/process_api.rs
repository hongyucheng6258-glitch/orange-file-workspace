//! 可故障注入的 Windows 进程抽象。
//!
//! 生产实现把 Win32 Job Object、挂起进程、管道、恢复、终止和等待封装成
//! 最小原语；运行状态机只依赖 `Win32ProcessApi` trait，测试用注入式替身
//! 稳定触发创建失败、Assign 失败、Resume 失败、终止超时等竞态分支。
//!
//! 关键时序：Job Object 先创建并设置 KILL_ON_JOB_CLOSE；进程以
//! `CREATE_SUSPENDED` 创建；加入 Job 成功后才 `ResumeThread`。任何失败
//! 分支都不允许恢复未受控进程。

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

/// 创建子进程所需的全部参数（已由运行配置规范化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// 环境覆盖：值 None 表示从子进程环境删除。
    pub env_overrides: Vec<(String, Option<String>)>,
}

/// 进程 API 错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessApiError {
    pub code: String,
    pub message: String,
    pub win32_code: Option<u32>,
}

impl ProcessApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            win32_code: None,
        }
    }
}

/// 等待进程退出的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitResult {
    /// 进程已退出。
    Exited { exit_code: u32 },
    /// 等待超时，进程仍在运行。
    Timeout,
    /// 等待失败。
    Failed(ProcessApiError),
}

/// Job Object 句柄所有权包装。
pub struct JobHandle {
    #[cfg(windows)]
    inner: windows::Win32::Foundation::HANDLE,
}

impl std::fmt::Debug for JobHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobHandle").finish()
    }
}

/// 挂起中的子进程：进程句柄 + 主线程句柄 + stdout/stderr 读取端。
pub struct SuspendedProcess {
    #[cfg(windows)]
    process: windows::Win32::Foundation::HANDLE,
    #[cfg(windows)]
    thread: windows::Win32::Foundation::HANDLE,
    pub pid: u32,
    pub stdout: Option<Box<dyn Read + Send>>,
    pub stderr: Option<Box<dyn Read + Send>>,
}

impl std::fmt::Debug for SuspendedProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SuspendedProcess")
            .field("pid", &self.pid)
            .field("has_stdout", &self.stdout.is_some())
            .field("has_stderr", &self.stderr.is_some())
            .finish()
    }
}

/// 跨平台进程 API 抽象。
pub trait Win32ProcessApi: Send + Sync {
    fn create_job(&self) -> Result<JobHandle, ProcessApiError>;
    fn set_job_kill_on_close(&self, job: &JobHandle) -> Result<(), ProcessApiError>;
    fn create_process_suspended(
        &self,
        spec: &ProcessSpec,
    ) -> Result<SuspendedProcess, ProcessApiError>;
    fn assign_process_to_job(
        &self,
        job: &JobHandle,
        process: &SuspendedProcess,
    ) -> Result<(), ProcessApiError>;
    fn resume_thread(&self, process: &SuspendedProcess) -> Result<(), ProcessApiError>;
    fn terminate_job(&self, job: &JobHandle) -> Result<(), ProcessApiError>;
    /// 直接终止进程（用于进程尚未加入 Job 的失败分支）。
    fn terminate_process(&self, process: &SuspendedProcess) -> Result<(), ProcessApiError>;
    fn wait_process_exit(&self, process: &SuspendedProcess, timeout: Duration) -> WaitResult;
    /// 查询 Job Object 内活动进程数。
    fn query_job_process_count(&self, job: &JobHandle) -> Result<u32, ProcessApiError>;
}

/// 标准 argv 引号规则（CommandLineToArgvW 兼容）。
pub fn quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    if !arg
        .chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\\' || c == '\t')
    {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        if ch == '\\' {
            backslashes += 1;
            continue;
        }
        if ch == '"' {
            // 2n+1 个反斜杠 + 引号 → n 个字面反斜杠 + 字面引号。
            for _ in 0..backslashes * 2 + 1 {
                out.push('\\');
            }
            backslashes = 0;
            out.push('"');
            continue;
        }
        for _ in 0..backslashes {
            out.push('\\');
        }
        backslashes = 0;
        out.push(ch);
    }
    // 结尾反斜杠翻倍，保证闭合引号被正确解析。
    for _ in 0..backslashes * 2 {
        out.push('\\');
    }
    out.push('"');
    out
}

/// 由程序和参数构造 CreateProcessW 命令行。
pub fn build_command_line(executable: &str, args: &[String]) -> String {
    let mut parts = vec![quote_arg(executable)];
    for a in args {
        parts.push(quote_arg(a));
    }
    parts.join(" ")
}

/// 由基础环境 + 覆盖构造子进程环境块（UTF-16 双 NUL 结尾）。
/// 值 None 表示删除对应变量。
pub fn build_env_block(
    base: impl Iterator<Item = (String, String)>,
    overrides: &[(String, Option<String>)],
) -> Vec<u16> {
    let mut map: Vec<(String, String)> = base.collect();
    for (key, value) in overrides {
        let upper = key.to_uppercase();
        if let Some(value) = value {
            if let Some(existing) = map.iter_mut().find(|(k, _)| k.to_uppercase() == upper) {
                existing.1 = value.clone();
            } else {
                map.push((key.clone(), value.clone()));
            }
        } else {
            map.retain(|(k, _)| k.to_uppercase() != upper);
        }
    }
    let mut block = Vec::new();
    let mut sorted = map;
    sorted.sort_by(|a, b| a.0.to_uppercase().cmp(&b.0.to_uppercase()));
    for (key, value) in sorted {
        for unit in format!("{key}={value}").encode_utf16() {
            block.push(unit);
        }
        block.push(0);
    }
    block.push(0);
    block
}

#[cfg(windows)]
mod win32 {
    use super::*;
    use std::os::windows::io::FromRawHandle;
    use windows::Win32::Foundation::{
        CloseHandle, GetLastError, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0,
        WAIT_TIMEOUT, GENERIC_READ,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
        QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows::Win32::System::Pipes::CreatePipe;
    use windows::Win32::System::Threading::{
        CreateProcessW, GetExitCodeProcess, ResumeThread, TerminateProcess, WaitForSingleObject,
        PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW, CREATE_NO_WINDOW,
        CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    };

    fn last_win32_error() -> u32 {
        unsafe { GetLastError().0 }
    }

    fn api_error(context: &str) -> ProcessApiError {
        let code = last_win32_error();
        ProcessApiError {
            code: "win32_error".into(),
            message: format!("{context} 失败（Win32 错误 {code}）"),
            win32_code: Some(code),
        }
    }

    impl Drop for JobHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.inner);
            }
        }
    }

    impl Drop for SuspendedProcess {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.process);
                let _ = CloseHandle(self.thread);
            }
        }
    }

    /// 打开 NUL 设备作为子进程 stdin。
    fn open_nul_read_handle() -> Result<HANDLE, ProcessApiError> {
        let wide: Vec<u16> = "NUL".encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            CreateFileW(
                windows::core::PCWSTR(wide.as_ptr()),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .map_err(|_| api_error("打开 NUL 设备"))?;
        if handle.is_invalid() {
            return Err(api_error("打开 NUL 设备"));
        }
        Ok(handle)
    }

    fn make_inheritable(handle: HANDLE) -> Result<(), ProcessApiError> {
        unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
            .map_err(|_| api_error("设置句柄继承"))
    }

    /// 真实 Win32 实现。
    pub struct Win32ProcessApiImpl;

    impl Win32ProcessApi for Win32ProcessApiImpl {
        fn create_job(&self) -> Result<JobHandle, ProcessApiError> {
            let handle =
                unsafe { CreateJobObjectW(None, None) }.map_err(|_| api_error("创建 Job Object"))?;
            if handle.is_invalid() {
                return Err(api_error("创建 Job Object"));
            }
            Ok(JobHandle { inner: handle })
        }

        fn set_job_kill_on_close(&self, job: &JobHandle) -> Result<(), ProcessApiError> {
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            unsafe {
                SetInformationJobObject(
                    job.inner,
                    windows::Win32::System::JobObjects::JobObjectExtendedLimitInformation,
                    &info as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            }
            .map_err(|_| api_error("设置 KILL_ON_JOB_CLOSE"))
        }

        fn create_process_suspended(
            &self,
            spec: &ProcessSpec,
        ) -> Result<SuspendedProcess, ProcessApiError> {
            // 输出管道：子进程继承写端，父进程持有读端。
            let mut out_read: HANDLE = Default::default();
            let mut out_write: HANDLE = Default::default();
            unsafe { CreatePipe(&mut out_read, &mut out_write, None, 0) }
                .map_err(|_| api_error("创建 stdout 管道"))?;
            make_inheritable(out_write)?;
            let mut err_read: HANDLE = Default::default();
            let mut err_write: HANDLE = Default::default();
            unsafe { CreatePipe(&mut err_read, &mut err_write, None, 0) }
                .map_err(|_| api_error("创建 stderr 管道"))?;
            make_inheritable(err_write)?;

            let nul_in = open_nul_read_handle()?;

            let cmd_line = build_command_line(&spec.executable, &spec.args);
            let mut cmd_wide: Vec<u16> =
                cmd_line.encode_utf16().chain(std::iter::once(0)).collect();
            let env_block = build_env_block(std::env::vars(), &spec.env_overrides);

            let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            startup.dwFlags = STARTF_USESTDHANDLES;
            startup.hStdInput = nul_in;
            startup.hStdOutput = out_write;
            startup.hStdError = err_write;

            let cwd_wide: Vec<u16> = spec
                .cwd
                .as_deref()
                .map(|p| p.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect())
                .unwrap_or_default();
            let cwd_ptr = if cwd_wide.is_empty() {
                windows::core::PCWSTR::null()
            } else {
                windows::core::PCWSTR(cwd_wide.as_ptr())
            };

            let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
            let flags = CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW;
            let created = unsafe {
                CreateProcessW(
                    windows::core::PCWSTR::null(),
                    Some(windows::core::PWSTR(cmd_wide.as_mut_ptr())),
                    None,
                    None,
                    true,
                    flags,
                    Some(env_block.as_ptr() as *const core::ffi::c_void),
                    cwd_ptr,
                    &startup,
                    &mut pi,
                )
            }
            .is_ok();

            // 父进程侧关闭不再需要的写端和 stdin。
            unsafe {
                let _ = CloseHandle(out_write);
                let _ = CloseHandle(err_write);
                let _ = CloseHandle(nul_in);
            }
            if !created {
                unsafe {
                    let _ = CloseHandle(out_read);
                    let _ = CloseHandle(err_read);
                }
                return Err(api_error("创建子进程"));
            }

            let stdout: Option<Box<dyn Read + Send>> = unsafe {
                Some(Box::new(std::fs::File::from_raw_handle(out_read.0 as _)))
            };
            let stderr: Option<Box<dyn Read + Send>> = unsafe {
                Some(Box::new(std::fs::File::from_raw_handle(err_read.0 as _)))
            };
            let pid = pi.dwProcessId;

            Ok(SuspendedProcess {
                process: pi.hProcess,
                thread: pi.hThread,
                pid,
                stdout,
                stderr,
            })
        }

        fn assign_process_to_job(
            &self,
            job: &JobHandle,
            process: &SuspendedProcess,
        ) -> Result<(), ProcessApiError> {
            unsafe { AssignProcessToJobObject(job.inner, process.process) }
                .map_err(|_| api_error("AssignProcessToJobObject"))
        }

        fn resume_thread(&self, process: &SuspendedProcess) -> Result<(), ProcessApiError> {
            let previous = unsafe { ResumeThread(process.thread) };
            if previous == u32::MAX {
                return Err(api_error("ResumeThread"));
            }
            Ok(())
        }

        fn terminate_job(&self, job: &JobHandle) -> Result<(), ProcessApiError> {
            unsafe { TerminateJobObject(job.inner, 1) }.map_err(|_| api_error("TerminateJobObject"))
        }

        fn terminate_process(&self, process: &SuspendedProcess) -> Result<(), ProcessApiError> {
            unsafe { TerminateProcess(process.process, 1) }
                .map_err(|_| api_error("TerminateProcess"))
        }

        fn wait_process_exit(&self, process: &SuspendedProcess, timeout: Duration) -> WaitResult {
            let ms = timeout.as_millis().min(u32::MAX as u128) as u32;
            let result = unsafe { WaitForSingleObject(process.process, ms) };
            match result {
                WAIT_OBJECT_0 => {
                    let mut code: u32 = 0;
                    let ok = unsafe { GetExitCodeProcess(process.process, &mut code) }.is_ok();
                    if !ok {
                        return WaitResult::Failed(ProcessApiError::new(
                            "win32_error",
                            "GetExitCodeProcess 失败",
                        ));
                    }
                    WaitResult::Exited { exit_code: code }
                }
                WAIT_TIMEOUT => WaitResult::Timeout,
                _ => WaitResult::Failed(ProcessApiError::new(
                    "win32_error",
                    "WaitForSingleObject 失败",
                )),
            }
        }

        fn query_job_process_count(&self, job: &JobHandle) -> Result<u32, ProcessApiError> {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
            unsafe {
                QueryInformationJobObject(
                    Some(job.inner),
                    JobObjectBasicAccountingInformation,
                    &mut info as *mut _ as *mut core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    None,
                )
            }
            .map_err(|_| api_error("QueryInformationJobObject"))?;
            Ok(info.ActiveProcesses)
        }
    }
}

#[cfg(windows)]
#[allow(unused_imports)] // 步骤 4 运行状态机将引用该实现
pub use win32::Win32ProcessApiImpl;

#[cfg(not(windows))]
pub struct Win32ProcessApiImpl;

#[cfg(not(windows))]
impl Win32ProcessApi for Win32ProcessApiImpl {
    fn create_job(&self) -> Result<JobHandle, ProcessApiError> {
        Err(ProcessApiError::new("unsupported", "当前平台不支持 Job Object"))
    }
    fn set_job_kill_on_close(&self, _job: &JobHandle) -> Result<(), ProcessApiError> {
        Err(ProcessApiError::new("unsupported", "当前平台不支持 Job Object"))
    }
    fn create_process_suspended(
        &self,
        _spec: &ProcessSpec,
    ) -> Result<SuspendedProcess, ProcessApiError> {
        Err(ProcessApiError::new("unsupported", "当前平台不支持挂起进程"))
    }
    fn assign_process_to_job(
        &self,
        _job: &JobHandle,
        _process: &SuspendedProcess,
    ) -> Result<(), ProcessApiError> {
        Err(ProcessApiError::new("unsupported", "当前平台不支持 Job Object"))
    }
    fn resume_thread(&self, _process: &SuspendedProcess) -> Result<(), ProcessApiError> {
        Err(ProcessApiError::new("unsupported", "当前平台不支持恢复线程"))
    }
    fn terminate_job(&self, _job: &JobHandle) -> Result<(), ProcessApiError> {
        Err(ProcessApiError::new("unsupported", "当前平台不支持终止 Job"))
    }
    fn terminate_process(&self, _process: &SuspendedProcess) -> Result<(), ProcessApiError> {
        Err(ProcessApiError::new("unsupported", "当前平台不支持终止进程"))
    }
    fn wait_process_exit(
        &self,
        _process: &SuspendedProcess,
        _timeout: Duration,
    ) -> WaitResult {
        WaitResult::Failed(ProcessApiError::new("unsupported", "当前平台不支持等待进程"))
    }
    fn query_job_process_count(&self, _job: &JobHandle) -> Result<u32, ProcessApiError> {
        Err(ProcessApiError::new("unsupported", "当前平台不支持查询 Job"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_arg_plain() {
        assert_eq!(quote_arg("npm"), "npm");
        assert_eq!(quote_arg("server.js"), "server.js");
    }

    #[test]
    fn quote_arg_with_space() {
        assert_eq!(
            quote_arg("C:\\Program Files\\node.exe"),
            "\"C:\\Program Files\\node.exe\""
        );
    }

    #[test]
    fn quote_arg_with_embedded_quotes_and_backslashes() {
        assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote_arg("a\\"), "\"a\\\\\"");
        assert_eq!(quote_arg(""), "\"\"");
    }

    #[test]
    fn build_command_line_joins_and_quotes() {
        let line = build_command_line(
            "C:\\Program Files\\node.exe",
            &["server.js".into(), "a b".into()],
        );
        assert_eq!(line, "\"C:\\Program Files\\node.exe\" server.js \"a b\"");
    }

    #[test]
    fn env_block_applies_overrides_and_removals() {
        let base = vec![
            ("PATH".to_string(), "C:\\base".to_string()),
            ("KEEP".to_string(), "1".to_string()),
        ];
        let overrides = vec![
            ("PATH".to_string(), Some("C:\\new".to_string())),
            ("DELETE_ME".to_string(), None),
        ];
        let block = build_env_block(base.into_iter(), &overrides);
        let text: String = block
            .split(|&u| u == 0)
            .filter(|s| !s.is_empty())
            .map(|u| String::from_utf16_lossy(u))
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(text, "KEEP=1|PATH=C:\\new");
        assert!(block.ends_with(&[0, 0]));
    }

    #[test]
    fn env_block_case_insensitive_override() {
        let base = vec![("Path".to_string(), "C:\\old".to_string())];
        let overrides = vec![("PATH".to_string(), Some("C:\\new".to_string()))];
        let block = build_env_block(base.into_iter(), &overrides);
        let text: String = block
            .split(|&u| u == 0)
            .filter(|s| !s.is_empty())
            .map(|u| String::from_utf16_lossy(u))
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(text, "Path=C:\\new");
    }
}
