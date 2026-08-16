//! Web 端口预览：目标解析、端口监听检测与 Job 归属校验。
//!
//! 预览目标按优先级确定：用户配置的 `expected_port` + `preview_scheme` →
//! 结构化命令参数中的明确端口 → 日志中的本地 `http(s)://` 地址及路径。
//! 打开前必须确认端口正在监听；监听 PID 无法确认属于当前运行时时不自动打开。

use std::sync::Arc;

use serde::Serialize;

use crate::error::AppError;
use crate::services::process_api::JobHandle;
use crate::services::project_runtime::RuntimeManager;

/// 预览目标来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewSource {
    /// 用户配置的 expected_port + preview_scheme。
    Config,
    /// 结构化命令参数中的明确端口。
    Args,
    /// 日志中识别出的本地 URL。
    Log,
}

/// 端口归属状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PortOwnership {
    /// 监听 PID 属于当前运行所在的 Job Object。
    Confirmed,
    /// 无法确认归属（无 Job 句柄、运行已结束或 PID 不在 Job 内）。
    Unconfirmed,
}

/// 解析出的预览目标（含来源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlCandidate {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub source: PreviewSource,
}

/// 打开预览命令的返回结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTarget {
    pub run_id: String,
    pub project_id: String,
    pub url: String,
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub source: PreviewSource,
    pub ownership: PortOwnership,
}

/// 预览错误，映射到 `AppError`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewError {
    pub code: String,
    pub message: String,
}

impl PreviewError {
    fn unavailable(msg: impl Into<String>) -> Self {
        Self {
            code: "preview_unavailable".into(),
            message: msg.into(),
        }
    }
    fn run_not_found(msg: impl Into<String>) -> Self {
        Self {
            code: "run_not_found".into(),
            message: msg.into(),
        }
    }
}

impl From<PreviewError> for AppError {
    fn from(e: PreviewError) -> Self {
        AppError::new(e.code, e.message)
    }
}

/// 端口监听探测抽象；Windows 生产实现基于 `GetExtendedTcpTable`。
pub trait PortProbe: Send + Sync {
    /// 返回指定端口上处于 LISTEN 状态的进程 PID；未监听返回 None。
    fn listening_pid(&self, port: u16) -> Option<u32>;
}

/// 从命令行参数提取明确端口：`--port=3000`、`--port 3000`、`-p 3000`。
pub fn port_from_args(args: &[String]) -> Option<u16> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let value = if let Some(v) = arg.strip_prefix("--port=") {
            Some(v.to_string())
        } else if arg == "--port" || arg == "-p" {
            iter.next().map(|v| v.to_string())
        } else {
            None
        };
        if let Some(v) = value {
            if let Ok(port) = v.parse::<u16>() {
                if port > 0 {
                    return Some(port);
                }
            }
        }
    }
    None
}

/// 扫描文本中的 URL（http:// 或 https://，大小写不敏感，空白/引号/<> 终止）。
/// 全程按字节处理，避免多字节 UTF-8 字符导致字符串切片 panic。
fn scan_urls(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut urls = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let scheme_len =
            if bytes[i..].len() >= 8 && bytes[i..i + 8].eq_ignore_ascii_case(b"https://") {
                8
            } else if bytes[i..].len() >= 7 && bytes[i..i + 7].eq_ignore_ascii_case(b"http://") {
                7
            } else {
                0
            };
        if scheme_len > 0 {
            let start = i;
            let mut end = start + scheme_len;
            while end < bytes.len() {
                let c = bytes[end];
                if c.is_ascii_whitespace() || c == b'"' || c == b'\'' || c == b'<' || c == b'>' {
                    break;
                }
                end += 1;
            }
            if end > start + scheme_len {
                // URL 由 ASCII 组成，按字节切片恢复字符串是安全的。
                urls.push(String::from_utf8_lossy(&bytes[start..end]).to_string());
            }
            i = end;
        } else {
            i += 1;
        }
    }
    urls
}

fn is_local_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    let lower = h.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "localhost" | "127.0.0.1" | "127.0.0.2" | "0.0.0.0" | "::1" | "::"
    )
}

/// 归一本地 host 为可访问地址。
fn normalize_host(host: &str) -> String {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    let lower = h.to_ascii_lowercase();
    if matches!(lower.as_str(), "0.0.0.0" | "::1" | "::") {
        "127.0.0.1".to_string()
    } else {
        lower
    }
}

/// 解析单个 URL 字符串为本地候选；非本地地址返回 None。
fn parse_url_candidate(url: &str, source: PreviewSource) -> Option<UrlCandidate> {
    let lower = url.to_ascii_lowercase();
    let scheme = if lower.starts_with("https://") {
        "https"
    } else if lower.starts_with("http://") {
        "http"
    } else {
        return None;
    };
    let raw_rest = &url[scheme.len() + 3..];
    let (authority, path) = match raw_rest.find('/') {
        Some(idx) => (&raw_rest[..idx], &raw_rest[idx..]),
        None => (raw_rest, ""),
    };
    let (host_raw, port) = if let Some(idx) = authority.rfind(':') {
        let maybe_port = &authority[idx + 1..];
        if maybe_port.is_empty() {
            (authority, None)
        } else if let Ok(p) = maybe_port.parse::<u16>() {
            (&authority[..idx], Some(p))
        } else {
            (authority, None)
        }
    } else {
        (authority, None)
    };
    if !is_local_host(host_raw) {
        return None;
    }
    let host = normalize_host(host_raw);
    let port = port.unwrap_or_else(|| if scheme == "https" { 443 } else { 80 });
    Some(UrlCandidate {
        scheme: scheme.to_string(),
        host,
        port,
        path: if path.is_empty() {
            String::new()
        } else {
            path.to_string()
        },
        source,
    })
}

/// 从日志文本识别本地 http(s) 地址；多条时取第一条。
pub fn local_url_from_log(text: &str) -> Option<UrlCandidate> {
    for url in scan_urls(text) {
        if let Some(candidate) = parse_url_candidate(&url, PreviewSource::Log) {
            return Some(candidate);
        }
    }
    None
}

/// 按 config → args → log 优先级解析预览目标。
pub fn resolve_target(
    expected_port: Option<u16>,
    preview_scheme: &str,
    args: &[String],
    log_text: &str,
) -> Option<UrlCandidate> {
    let scheme = if preview_scheme.eq_ignore_ascii_case("https") {
        "https"
    } else {
        "http"
    };
    if let Some(port) = expected_port.filter(|p| *p > 0) {
        return Some(UrlCandidate {
            scheme: scheme.to_string(),
            host: "127.0.0.1".to_string(),
            port,
            path: String::new(),
            source: PreviewSource::Config,
        });
    }
    if let Some(port) = port_from_args(args) {
        return Some(UrlCandidate {
            scheme: scheme.to_string(),
            host: "127.0.0.1".to_string(),
            port,
            path: String::new(),
            source: PreviewSource::Args,
        });
    }
    local_url_from_log(log_text)
}

/// 格式化完整 URL。
pub fn format_url(candidate: &UrlCandidate) -> String {
    format!(
        "{}://{}:{}{}",
        candidate.scheme, candidate.host, candidate.port, candidate.path
    )
}

/// Windows TCP 监听表探测实现（`GetExtendedTcpTable`）。
#[cfg(windows)]
pub struct Win32PortProbe;

#[cfg(windows)]
impl PortProbe for Win32PortProbe {
    fn listening_pid(&self, port: u16) -> Option<u32> {
        win32_tcp_listener_pid(port)
    }
}

#[cfg(windows)]
fn win32_tcp_listener_pid(port: u16) -> Option<u32> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
        TCP_TABLE_OWNER_PID_LISTENER,
    };
    const NO_ERROR: u32 = 0;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    const MIB_TCP_STATE_LISTEN: u32 = 2;

    // 第一次调用获取所需缓冲区大小。
    let mut size: u32 = 0;
    let rc =
        unsafe { GetExtendedTcpTable(None, &mut size, false, 0, TCP_TABLE_OWNER_PID_LISTENER, 0) };
    if rc == NO_ERROR && size == 0 {
        return None;
    }
    if rc != NO_ERROR && rc != ERROR_INSUFFICIENT_BUFFER {
        return None;
    }
    let mut buf: Vec<u8> = vec![0u8; size as usize];
    let rc = unsafe {
        GetExtendedTcpTable(
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            &mut size,
            false,
            0,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if rc != NO_ERROR {
        return None;
    }
    let table_ptr = buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID;
    let entries = unsafe { (*table_ptr).dwNumEntries } as usize;
    let rows = unsafe { &(*table_ptr).table[0] } as *const MIB_TCPROW_OWNER_PID;
    for i in 0..entries {
        let row = unsafe { &*rows.add(i) };
        if row.dwState != MIB_TCP_STATE_LISTEN {
            continue;
        }
        // dwLocalPort 为网络字节序。
        let local_port = u16::from_be(row.dwLocalPort as u16);
        if local_port == port {
            return Some(row.dwOwningPid);
        }
    }
    None
}

/// 空端口探测（非 Windows 平台或测试构造用）。
#[allow(dead_code)] // 非 Windows 平台与测试模块使用
pub struct UnsupportedPortProbe;

impl PortProbe for UnsupportedPortProbe {
    fn listening_pid(&self, _port: u16) -> Option<u32> {
        None
    }
}

/// 运行上下文（由 RuntimeManager::preview_context 提供）。
pub struct PreviewContext {
    pub run_id: String,
    pub project_id: String,
    pub expected_port: Option<u16>,
    pub preview_scheme: String,
    pub args: Vec<String>,
    pub log_text: String,
    pub job: Option<JobHandle>,
}

/// 预览服务：解析目标 → 探测端口 → 校验归属 → 发布事件。
pub struct PreviewService {
    runtime: Arc<RuntimeManager>,
    probe: Arc<dyn PortProbe>,
}

impl PreviewService {
    pub fn new(runtime: Arc<RuntimeManager>, probe: Arc<dyn PortProbe>) -> Self {
        Self { runtime, probe }
    }

    /// 打开预览：返回经过验证的预览目标；端口未监听或无目标时返回
    /// `preview_unavailable`。
    pub fn open_preview(&self, run_id: &str) -> Result<PreviewTarget, PreviewError> {
        let ctx = self
            .runtime
            .preview_context(run_id)
            .ok_or_else(|| PreviewError::run_not_found("运行实例不存在或已结束"))?;
        let candidate = resolve_target(
            ctx.expected_port,
            &ctx.preview_scheme,
            &ctx.args,
            &ctx.log_text,
        )
        .ok_or_else(|| {
            PreviewError::unavailable("未发现可验证的预览目标（配置端口、参数或日志均无可用地址）")
        })?;
        // 端口必须正在监听。
        let pid = self.probe.listening_pid(candidate.port).ok_or_else(|| {
            PreviewError::unavailable(format!("端口 {} 当前未监听", candidate.port))
        })?;
        // 归属校验：监听 PID 应属于当前运行所在的 Job Object。
        let ownership = match &ctx.job {
            Some(job) => {
                let pids = self.runtime.job_process_ids(job).unwrap_or_default();
                if pids.contains(&pid) {
                    PortOwnership::Confirmed
                } else {
                    PortOwnership::Unconfirmed
                }
            }
            None => PortOwnership::Unconfirmed,
        };
        let target = PreviewTarget {
            run_id: ctx.run_id.clone(),
            project_id: ctx.project_id.clone(),
            url: format_url(&candidate),
            scheme: candidate.scheme,
            host: candidate.host,
            port: candidate.port,
            path: candidate.path,
            source: candidate.source,
            ownership,
        };
        // 发布就绪事件，前端据此更新预览按钮。
        self.runtime.emit_preview_ready(&target);
        Ok(target)
    }
}

/// 共享预览依赖：为 lib.rs 初始化提供统一入口。
pub fn build_preview_service(runtime: Arc<RuntimeManager>) -> Arc<PreviewService> {
    #[cfg(windows)]
    let probe: Arc<dyn PortProbe> = Arc::new(Win32PortProbe);
    #[cfg(not(windows))]
    let probe: Arc<dyn PortProbe> = Arc::new(UnsupportedPortProbe);
    Arc::new(PreviewService::new(runtime, probe))
}

#[cfg(test)]
mod tests;
