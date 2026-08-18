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
use std::path::{Path, PathBuf};
use std::sync::Arc;
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

/// Job Object 句柄所有权包装（引用计数共享）。
///
/// 运行状态机与预览服务可能同时持有同一 Job 的句柄（端口归属校验），
/// 因此按引用计数共享底层 HANDLE，引用归零才 `CloseHandle`。
pub struct JobHandle {
    inner: Arc<JobInner>,
}

impl Clone for JobHandle {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

struct JobInner {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
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
    /// 查询 Job Object 内全部进程 PID（用于端口归属校验）。
    fn query_job_process_ids(&self, job: &JobHandle) -> Result<Vec<u32>, ProcessApiError>;
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
    let is_cmd = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("cmd.exe"))
        .unwrap_or(false);
    let command_index = is_cmd
        .then(|| args.iter().position(|arg| arg.eq_ignore_ascii_case("/c")))
        .flatten()
        .and_then(|index| (index + 1 < args.len()).then_some(index + 1));

    for (index, arg) in args.iter().enumerate() {
        if Some(index) == command_index {
            // /c 后的命令由 cmd.exe 自行解析；C 运行时转义会把内嵌引号
            // 变成字面反斜杠，导致带引号的 .cmd 路径无法执行。
            parts.push(format!("\"{arg}\""));
        } else {
            parts.push(quote_arg(arg));
        }
    }
    parts.join(" ")
}

/// 限制 Job Object PID 列表的读取数量，避免内核计数变化导致越界。
fn safe_process_id_count(assigned: usize, in_list: usize, capacity: usize) -> usize {
    assigned.min(in_list).min(capacity)
}

/// 扩容 Job PID 缓冲区容量：翻倍增长直至上限；已达上限返回 None。
fn next_pid_capacity(current: usize, max: usize) -> Option<usize> {
    if current >= max {
        None
    } else {
        Some((current * 2).min(max))
    }
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
    sorted.sort_by_key(|(k, _)| k.to_uppercase());
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
        CloseHandle, GetLastError, SetHandleInformation, GENERIC_READ, HANDLE, HANDLE_FLAG_INHERIT,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
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
        CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
        InitializeProcThreadAttributeList, ResumeThread, TerminateProcess,
        UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED,
        CREATE_UNICODE_ENVIRONMENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
        PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW,
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

    impl Drop for JobInner {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.handle);
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

    /// RAII 守卫：Drop 时自动关闭原始句柄；`into_raw` 放弃所有权并返回句柄。
    /// 用于进程创建流程：中间步骤失败时自动清理已创建的句柄，防止泄漏。
    struct RawHandleGuard(HANDLE);

    impl RawHandleGuard {
        fn into_raw(self) -> HANDLE {
            let h = self.0;
            std::mem::forget(self);
            h
        }
    }

    impl Drop for RawHandleGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    /// 进程线程属性列表守卫：Drop 时调用 DeleteProcThreadAttributeList。
    struct ProcAttributeGuard(LPPROC_THREAD_ATTRIBUTE_LIST);

    impl Drop for ProcAttributeGuard {
        fn drop(&mut self) {
            if !self.0 .0.is_null() {
                unsafe { DeleteProcThreadAttributeList(self.0) };
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
            let handle = unsafe { CreateJobObjectW(None, None) }
                .map_err(|_| api_error("创建 Job Object"))?;
            if handle.is_invalid() {
                return Err(api_error("创建 Job Object"));
            }
            Ok(JobHandle {
                inner: Arc::new(JobInner { handle }),
            })
        }

        fn set_job_kill_on_close(&self, job: &JobHandle) -> Result<(), ProcessApiError> {
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            unsafe {
                SetInformationJobObject(
                    job.inner.handle,
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
            // 每个句柄创建后立即由 RAII 守卫接管；任何中间步骤失败时守卫自动关闭，
            // 避免多次启动失败累积泄漏系统句柄。全部成功后才移交所有权。
            let mut out_read: HANDLE = Default::default();
            let mut out_write: HANDLE = Default::default();
            unsafe { CreatePipe(&mut out_read, &mut out_write, None, 0) }
                .map_err(|_| api_error("创建 stdout 管道"))?;
            let out_read_g = RawHandleGuard(out_read);
            let _out_write_g = RawHandleGuard(out_write);
            make_inheritable(out_write)?;

            let mut err_read: HANDLE = Default::default();
            let mut err_write: HANDLE = Default::default();
            unsafe { CreatePipe(&mut err_read, &mut err_write, None, 0) }
                .map_err(|_| api_error("创建 stderr 管道"))?;
            let err_read_g = RawHandleGuard(err_read);
            let _err_write_g = RawHandleGuard(err_write);
            make_inheritable(err_write)?;

            let nul_in = open_nul_read_handle()?;
            let _nul_g = RawHandleGuard(nul_in);

            // 显式句柄继承白名单：仅 stdin/stdout/stderr，避免应用内其他可继承
            // 句柄（文件、管道、同步对象）被意外传给项目代码。
            let inherit_handles = [nul_in, out_write, err_write];
            let mut attr_size: usize = 0;
            // 第一次调用仅查询所需大小（必然返回缓冲区不足，忽略结果）。
            let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut attr_size) };
            if attr_size == 0 {
                return Err(api_error("初始化句柄属性列表"));
            }
            let mut attr_buf: Vec<u8> = vec![0u8; attr_size];
            let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut _);
            unsafe {
                InitializeProcThreadAttributeList(Some(attr_list), 1, None, &mut attr_size)
                    .map_err(|_| api_error("初始化句柄属性列表"))?;
            }
            let _attr_guard = ProcAttributeGuard(attr_list);
            unsafe {
                UpdateProcThreadAttribute(
                    attr_list,
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    Some(inherit_handles.as_ptr() as *const core::ffi::c_void),
                    std::mem::size_of_val(&inherit_handles),
                    None,
                    None,
                )
                .map_err(|_| api_error("设置句柄继承列表"))?;
            }

            let cmd_line = build_command_line(&spec.executable, &spec.args);
            let mut cmd_wide: Vec<u16> =
                cmd_line.encode_utf16().chain(std::iter::once(0)).collect();
            let env_block = build_env_block(std::env::vars(), &spec.env_overrides);

            let mut startup_ex: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
            startup_ex.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            startup_ex.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup_ex.StartupInfo.hStdInput = nul_in;
            startup_ex.StartupInfo.hStdOutput = out_write;
            startup_ex.StartupInfo.hStdError = err_write;
            startup_ex.lpAttributeList = attr_list;

            let cwd_wide: Vec<u16> = spec
                .cwd
                .as_deref()
                .map(|p| {
                    p.to_string_lossy()
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect()
                })
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
                    &startup_ex as *const STARTUPINFOEXW as *const STARTUPINFOW,
                    &mut pi,
                )
            }
            .is_ok();

            if !created {
                return Err(api_error("创建子进程"));
            }
            // 成功：读端所有权移交给 File；写端与 NUL 句柄随守卫在函数返回时关闭。
            let stdout: Option<Box<dyn Read + Send>> = unsafe {
                Some(Box::new(std::fs::File::from_raw_handle(
                    out_read_g.into_raw().0 as _,
                )))
            };
            let stderr: Option<Box<dyn Read + Send>> = unsafe {
                Some(Box::new(std::fs::File::from_raw_handle(
                    err_read_g.into_raw().0 as _,
                )))
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
            unsafe { AssignProcessToJobObject(job.inner.handle, process.process) }
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
            unsafe { TerminateJobObject(job.inner.handle, 1) }
                .map_err(|_| api_error("TerminateJobObject"))
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
                    Some(job.inner.handle),
                    JobObjectBasicAccountingInformation,
                    &mut info as *mut _ as *mut core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    None,
                )
            }
            .map_err(|_| api_error("QueryInformationJobObject"))?;
            Ok(info.ActiveProcesses)
        }

        fn query_job_process_ids(&self, job: &JobHandle) -> Result<Vec<u32>, ProcessApiError> {
            use windows::Win32::System::JobObjects::{
                JobObjectBasicProcessIdList, JOBOBJECT_BASIC_PROCESS_ID_LIST,
            };
            const MAX_PIDS: usize = 4096;
            let header_bytes = std::mem::size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>()
                - std::mem::size_of::<usize>();
            // 从 1 个 PID 槽位开始，缓冲区不足（ERROR_MORE_DATA）时翻倍扩容。
            let mut pid_capacity = 1usize;
            loop {
                let total = header_bytes + std::mem::size_of::<usize>() * pid_capacity;
                let mut buf: Vec<u8> = vec![0u8; total];
                let queried = unsafe {
                    QueryInformationJobObject(
                        Some(job.inner.handle),
                        JobObjectBasicProcessIdList,
                        buf.as_mut_ptr() as *mut core::ffi::c_void,
                        total as u32,
                        None,
                    )
                };
                match queried {
                    Ok(()) => {
                        let list = buf.as_ptr() as *const JOBOBJECT_BASIC_PROCESS_ID_LIST;
                        let assigned = unsafe { (*list).NumberOfAssignedProcesses } as usize;
                        let in_list = unsafe { (*list).NumberOfProcessIdsInList } as usize;
                        let capacity = (total - header_bytes) / std::mem::size_of::<usize>();
                        let readable = safe_process_id_count(assigned, in_list, capacity);
                        let pids_ptr = unsafe { &(*list).ProcessIdList[0] } as *const usize;
                        let mut pids = Vec::with_capacity(readable);
                        for i in 0..readable {
                            pids.push(unsafe { *pids_ptr.add(i) } as u32);
                        }
                        return Ok(pids);
                    }
                    Err(_) => match next_pid_capacity(pid_capacity, MAX_PIDS) {
                        Some(next) => pid_capacity = next,
                        None => return Err(api_error("QueryInformationJobObject")),
                    },
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn raw_handle_guard_into_raw_keeps_handle_alive() {
            // into_raw 必须放弃所有权并返回原句柄，供成功路径接管。
            let mut r: HANDLE = Default::default();
            let mut w: HANDLE = Default::default();
            unsafe { CreatePipe(&mut r, &mut w, None, 0) }.unwrap();
            let guard = RawHandleGuard(r);
            let raw = guard.into_raw();
            assert_eq!(raw, r);
            unsafe {
                let _ = CloseHandle(raw);
                let _ = CloseHandle(w);
            }
        }
    }
}

#[cfg(windows)]
#[allow(unused_imports)] // 步骤 4 运行状态机将引用该实现
pub use win32::Win32ProcessApiImpl;

#[cfg(test)]
impl JobHandle {
    pub(crate) fn test_new() -> Self {
        Self {
            inner: Arc::new(JobInner::test_new()),
        }
    }
}

impl JobInner {
    #[cfg(test)]
    fn test_new() -> Self {
        #[cfg(windows)]
        {
            Self {
                handle: windows::Win32::Foundation::HANDLE(std::ptr::null_mut()),
            }
        }
        #[cfg(not(windows))]
        {
            Self {}
        }
    }
}

// Windows 内核句柄只是指针，可在线程间移动；Drop 从任意线程 CloseHandle 均安全。
// 运行管理器会把句柄所有权传入协调线程，因此必须标记 Send/Sync。
unsafe impl Send for JobInner {}
unsafe impl Sync for JobInner {}
unsafe impl Send for SuspendedProcess {}

#[cfg(test)]
impl SuspendedProcess {
    pub(crate) fn test_new(
        pid: u32,
        stdout: Option<Box<dyn Read + Send>>,
        stderr: Option<Box<dyn Read + Send>>,
    ) -> Self {
        #[cfg(windows)]
        {
            Self {
                process: windows::Win32::Foundation::HANDLE::default(),
                thread: windows::Win32::Foundation::HANDLE::default(),
                pid,
                stdout,
                stderr,
            }
        }
        #[cfg(not(windows))]
        {
            Self {
                pid,
                stdout,
                stderr,
            }
        }
    }
}

#[cfg(not(windows))]
pub struct Win32ProcessApiImpl;

#[cfg(not(windows))]
impl Win32ProcessApi for Win32ProcessApiImpl {
    fn create_job(&self) -> Result<JobHandle, ProcessApiError> {
        Err(ProcessApiError::new(
            "unsupported",
            "当前平台不支持 Job Object",
        ))
    }
    fn set_job_kill_on_close(&self, _job: &JobHandle) -> Result<(), ProcessApiError> {
        Err(ProcessApiError::new(
            "unsupported",
            "当前平台不支持 Job Object",
        ))
    }
    fn create_process_suspended(
        &self,
        _spec: &ProcessSpec,
    ) -> Result<SuspendedProcess, ProcessApiError> {
        Err(ProcessApiError::new(
            "unsupported",
            "当前平台不支持挂起进程",
        ))
    }
    fn assign_process_to_job(
        &self,
        _job: &JobHandle,
        _process: &SuspendedProcess,
    ) -> Result<(), ProcessApiError> {
        Err(ProcessApiError::new(
            "unsupported",
            "当前平台不支持 Job Object",
        ))
    }
    fn resume_thread(&self, _process: &SuspendedProcess) -> Result<(), ProcessApiError> {
        Err(ProcessApiError::new(
            "unsupported",
            "当前平台不支持恢复线程",
        ))
    }
    fn terminate_job(&self, _job: &JobHandle) -> Result<(), ProcessApiError> {
        Err(ProcessApiError::new(
            "unsupported",
            "当前平台不支持终止 Job",
        ))
    }
    fn terminate_process(&self, _process: &SuspendedProcess) -> Result<(), ProcessApiError> {
        Err(ProcessApiError::new(
            "unsupported",
            "当前平台不支持终止进程",
        ))
    }
    fn wait_process_exit(&self, _process: &SuspendedProcess, _timeout: Duration) -> WaitResult {
        WaitResult::Failed(ProcessApiError::new(
            "unsupported",
            "当前平台不支持等待进程",
        ))
    }
    fn query_job_process_count(&self, _job: &JobHandle) -> Result<u32, ProcessApiError> {
        Err(ProcessApiError::new(
            "unsupported",
            "当前平台不支持查询 Job",
        ))
    }
    fn query_job_process_ids(&self, _job: &JobHandle) -> Result<Vec<u32>, ProcessApiError> {
        Err(ProcessApiError::new(
            "unsupported",
            "当前平台不支持查询 Job",
        ))
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
    fn build_command_line_preserves_cmd_c_command_quotes() {
        let line = build_command_line(
            r"C:\Windows\System32\cmd.exe",
            &[
                "/d".into(),
                "/s".into(),
                "/c".into(),
                r#""C:\Users\User Name\tools\npm.CMD" run dev"#.into(),
            ],
        );
        assert_eq!(
            line,
            r#""C:\Windows\System32\cmd.exe" /d /s /c ""C:\Users\User Name\tools\npm.CMD" run dev""#
        );
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
            .map(String::from_utf16_lossy)
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(text, "KEEP=1|PATH=C:\\new");
        assert!(block.ends_with(&[0, 0]));
    }

    #[test]
    fn process_id_count_never_exceeds_list_or_buffer_capacity() {
        assert_eq!(safe_process_id_count(3, 1, 4), 1);
        assert_eq!(safe_process_id_count(3, 3, 2), 2);
        assert_eq!(safe_process_id_count(2, 2, 4), 2);
    }

    #[test]
    fn pid_capacity_grows_exponentially_up_to_max() {
        assert_eq!(next_pid_capacity(1, 4096), Some(2));
        assert_eq!(next_pid_capacity(2, 4096), Some(4));
        assert_eq!(next_pid_capacity(1024, 4096), Some(2048));
        assert_eq!(next_pid_capacity(2048, 4096), Some(4096));
        assert_eq!(next_pid_capacity(4096, 4096), None);
        assert_eq!(next_pid_capacity(4096, 256), None);
    }

    #[test]
    fn env_block_case_insensitive_override() {
        let base = vec![("Path".to_string(), "C:\\old".to_string())];
        let overrides = vec![("PATH".to_string(), Some("C:\\new".to_string()))];
        let block = build_env_block(base.into_iter(), &overrides);
        let text: String = block
            .split(|&u| u == 0)
            .filter(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(text, "Path=C:\\new");
    }
}
