//! 运行配置校验、规范化与用户确认协议。
//!
//! 前端不自行计算确认哈希。后端把配置规范化为固定顺序的 UTF-8 JSON，
//! 用会话密钥签发 HMAC-SHA-256。确认票据单次使用、10 分钟有效，仅驻留内存。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::error::AppError;

/// 确认票据有效期。
pub const CONFIRMATION_TTL: Duration = Duration::from_secs(10 * 60);
/// canonical JSON 版本。
pub const CANONICAL_VERSION: u32 = 1;

/// 前端提交的运行配置（Tauri 命令参数）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfig {
    pub project_id: String,
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: String,
    /// 键值环境变量覆盖；值为 null 表示从子进程环境删除。
    pub env_overrides: HashMap<String, Option<String>>,
    pub expected_port: Option<u16>,
    pub preview_scheme: String,
}

/// 规范化后的运行配置：字段顺序固定、路径已解析、环境变量已排序。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NormalizedRunConfig {
    pub version: u32,
    pub project_id: String,
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env_overrides: Vec<(String, Option<String>)>,
    pub expected_port: Option<u16>,
    pub preview_scheme: String,
}

/// 确认协议错误，直接映射到 `AppError`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmationError {
    pub code: String,
    pub message: String,
}

impl ConfirmationError {
    fn invalid_config(msg: impl Into<String>) -> Self {
        Self { code: "invalid_config".into(), message: msg.into() }
    }
    fn invalid_cwd(msg: impl Into<String>) -> Self {
        Self { code: "invalid_working_directory".into(), message: msg.into() }
    }
    fn runtime_not_found(msg: impl Into<String>) -> Self {
        Self { code: "runtime_not_found".into(), message: msg.into() }
    }
    fn confirmation_required(msg: impl Into<String>) -> Self {
        Self { code: "confirmation_required".into(), message: msg.into() }
    }
}

impl From<ConfirmationError> for AppError {
    fn from(e: ConfirmationError) -> Self {
        AppError::new(e.code, e.message)
    }
}

/// 确认预览：脱敏后的运行摘要。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConfirmationPreview {
    pub confirmation_id: String,
    pub summary: serde_json::Value,
    pub expires_in_seconds: u64,
}

/// 确认兑换结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConfirmationGrant {
    pub confirmation_id: String,
    pub confirmation_hash: String,
}

/// 一次性确认票据。
struct ConfirmationTicket {
    canonical_json: Vec<u8>,
    expires_at: Instant,
}

/// 会话级确认管理：密钥驻留内存，票据单次使用。
pub struct ConfirmationSession {
    secret: [u8; 32],
    ttl: Duration,
    tickets: Mutex<HashMap<String, ConfirmationTicket>>,
    next_id: AtomicU64,
}

impl Default for ConfirmationSession {
    fn default() -> Self {
        Self::with_ttl(CONFIRMATION_TTL)
    }
}

impl ConfirmationSession {
    #[allow(dead_code)] // 供命令层按需构造自定义密钥的会话
    pub fn new(secret: [u8; 32]) -> Self {
        Self::with_secret_and_ttl(secret, CONFIRMATION_TTL)
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        let mut secret = [0u8; 32];
        let id = uuid::Uuid::new_v4();
        secret[..16].copy_from_slice(id.as_bytes());
        let id2 = uuid::Uuid::new_v4();
        secret[16..].copy_from_slice(id2.as_bytes());
        Self::with_secret_and_ttl(secret, ttl)
    }

    pub fn with_secret_and_ttl(secret: [u8; 32], ttl: Duration) -> Self {
        Self {
            secret,
            ttl,
            tickets: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// 生成一次性确认票据并返回脱敏预览。
    pub fn prepare(
        &self,
        config: &RunConfig,
        project_root: &Path,
    ) -> Result<ConfirmationPreview, ConfirmationError> {
        let normalized = normalize(config, project_root)?;
        let canonical_json = canonical_json(&normalized);
        let id = format!("c{}", self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
        let expires_at = Instant::now() + self.ttl;
        self.tickets
            .lock()
            .unwrap()
            .insert(id.clone(), ConfirmationTicket { canonical_json, expires_at });
        Ok(ConfirmationPreview {
            confirmation_id: id,
            summary: redacted_summary(&normalized),
            expires_in_seconds: self.ttl.as_secs(),
        })
    }

    /// 原子兑换一次性票据并签发确认哈希。
    pub fn confirm(&self, confirmation_id: &str) -> Result<ConfirmationGrant, ConfirmationError> {
        let mut tickets = self.tickets.lock().unwrap();
        let Some(ticket) = tickets.remove(confirmation_id) else {
            return Err(ConfirmationError::confirmation_required(
                "确认票据不存在、已使用或已过期，请重新确认",
            ));
        };
        if ticket.expires_at <= Instant::now() {
            return Err(ConfirmationError::confirmation_required(
                "确认票据已过期，请重新确认",
            ));
        }
        let hash = compute_hash(&self.secret, &ticket.canonical_json);
        Ok(ConfirmationGrant {
            confirmation_id: confirmation_id.to_string(),
            confirmation_hash: hash,
        })
    }

    /// 校验配置并验证确认哈希；返回后端重新规范化的快照用于启动。
    pub fn verify(
        &self,
        config: &RunConfig,
        project_root: &Path,
        confirmation_hash: &str,
    ) -> Result<NormalizedRunConfig, ConfirmationError> {
        let normalized = normalize(config, project_root)?;
        let canonical_json = canonical_json(&normalized);
        let expected = compute_hash(&self.secret, &canonical_json);
        let ok = bool::from(
            expected
                .as_bytes()
                .ct_eq(confirmation_hash.as_bytes()),
        );
        if !ok {
            return Err(ConfirmationError::confirmation_required(
                "配置已变化或确认已失效，请重新确认",
            ));
        }
        Ok(normalized)
    }

    /// 清理过期票据。
    pub fn sweep_expired(&self) {
        let now = Instant::now();
        self.tickets
            .lock()
            .unwrap()
            .retain(|_, t| t.expires_at > now);
    }

    /// 当前有效票据数量（测试与诊断用）。
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.tickets.lock().unwrap().len()
    }
}

/// 基于 sha2 的 HMAC-SHA-256（SHA-256 块大小 64 字节）。
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        let h = Sha256::digest(key);
        k[..h.len()].copy_from_slice(&h);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0u8; BLOCK];
    let mut opad = [0u8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] = k[i] ^ 0x36;
        opad[i] = k[i] ^ 0x5c;
    }
    let inner = Sha256::new().chain_update(ipad).chain_update(data).finalize();
    let outer = Sha256::new().chain_update(opad).chain_update(inner).finalize();
    outer.into()
}

/// base64url 无填充编码。
fn base64url(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

fn compute_hash(secret: &[u8; 32], canonical_json: &[u8]) -> String {
    base64url(&hmac_sha256(secret, canonical_json))
}

/// 固定字段顺序的 canonical JSON 字节。
fn canonical_json(normalized: &NormalizedRunConfig) -> Vec<u8> {
    serde_json::to_vec(normalized).expect("normalized config serializes")
}

/// 环境变量名是否匹配敏感模式。
fn is_sensitive_key(key: &str) -> bool {
    let u = key.to_ascii_uppercase();
    ["TOKEN", "SECRET", "PASSWORD", "KEY", "CREDENTIAL", "AUTH"]
        .iter()
        .any(|k| u.contains(k))
}

/// 生成脱敏摘要，敏感环境变量只显示脱敏值。
fn redacted_summary(normalized: &NormalizedRunConfig) -> serde_json::Value {
    let env: serde_json::Value = normalized
        .env_overrides
        .iter()
        .map(|(k, v)| {
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
        "project_id": normalized.project_id,
        "executable": normalized.executable,
        "args": normalized.args,
        "cwd": normalized.cwd,
        "env": env,
        "expected_port": normalized.expected_port,
        "preview_scheme": normalized.preview_scheme,
    })
}

/// 去除 Windows 规范化路径的 `\\?\` 前缀并折叠盘符/UNC 主机名大小写。
fn strip_verbatim(s: &str) -> String {
    let mut s = s.to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        s = format!("\\\\{rest}");
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        s = rest.to_string();
    }
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_uppercase() {
        s = format!("{}{}", (bytes[0] as char).to_ascii_lowercase(), &s[1..]);
    }
    if let Some(rest) = s.strip_prefix(r"\\") {
        let mut parts = rest.splitn(3, '\\');
        if let (Some(host), Some(share)) = (parts.next(), parts.next()) {
            let tail = parts.next().unwrap_or("");
            s = format!(
                "\\\\{}\\{}{}",
                host.to_lowercase(),
                share.to_lowercase(),
                if tail.is_empty() {
                    String::new()
                } else {
                    format!("\\{tail}")
                }
            );
        }
    }
    s
}

/// 规范路径键：真实路径 + 统一分隔符 + 盘符/UNC 大小写折叠。
pub fn canonical_key(path: &Path) -> Option<PathBuf> {
    let canon = std::fs::canonicalize(path).ok()?;
    let s = canon.to_string_lossy().to_string();
    let s = strip_verbatim(&s).replace('/', "\\");
    Some(PathBuf::from(s))
}

/// 是否为 Windows 批处理文件。
fn is_batch(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("cmd" | "bat")
    )
}

/// 为 cmd.exe 转义单个参数。
fn escape_cmd_arg(arg: &str) -> String {
    let has_special = arg
        .chars()
        .any(|c| c.is_whitespace() || "&|<>^()\"%!".contains(c));
    if !has_special {
        return arg.to_string();
    }
    format!("\"{}\"", arg.replace('"', "\"\""))
}

/// 批处理文件转换为 `cmd.exe /d /s /c "<script> <args...>"` 形式。
fn batch_command(script: &Path, args: &[String]) -> (String, Vec<String>) {
    let sysroot = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    let cmd_exe = sysroot.join("System32").join("cmd.exe");
    let mut cmd = escape_cmd_arg(&script.to_string_lossy());
    for a in args {
        cmd.push(' ');
        cmd.push_str(&escape_cmd_arg(a));
    }
    (
        cmd_exe.to_string_lossy().to_string(),
        vec!["/d".to_string(), "/s".to_string(), "/c".to_string(), cmd],
    )
}

/// 在 PATH 目录中解析可执行文件（PATHEXT 扩展）。
fn find_on_path(name: &Path, dirs: &[PathBuf]) -> Option<PathBuf> {
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let exts: Vec<String> = pathext
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    for dir in dirs {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        for ext in &exts {
            let cand = dir.join(format!("{}{}", name.to_string_lossy(), ext));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// 合并 PATH：配置覆盖优先，否则使用当前进程 PATH。
fn merged_path(config: &RunConfig) -> Vec<PathBuf> {
    let path = config
        .env_overrides
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
        .and_then(|(_, v)| v.clone())
        .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
    std::env::split_paths(&path).collect()
}

/// 校验并规范化运行配置。
pub fn normalize(
    config: &RunConfig,
    project_root: &Path,
) -> Result<NormalizedRunConfig, ConfirmationError> {
    let executable = config.executable.trim();
    if executable.is_empty() {
        return Err(ConfirmationError::invalid_config("可执行程序不能为空"));
    }
    if config.args.iter().any(|a| a.contains('\0')) {
        return Err(ConfirmationError::invalid_config("参数包含 NUL 字符"));
    }

    // 环境变量校验：名称/值不含 NUL，名称不含 '='，大小写不敏感去重。
    let mut env_pairs: Vec<(String, Option<String>)> = Vec::with_capacity(config.env_overrides.len());
    for (key, value) in &config.env_overrides {
        if key.is_empty() || key.contains('\0') || key.contains('=') {
            return Err(ConfirmationError::invalid_config(format!(
                "环境变量名非法: {key:?}"
            )));
        }
        if let Some(v) = value {
            if v.contains('\0') {
                return Err(ConfirmationError::invalid_config(format!(
                    "环境变量 {key} 的值包含 NUL 字符"
                )));
            }
        }
        let upper = key.to_uppercase();
        if env_pairs.iter().any(|(k, _)| k == &upper) {
            return Err(ConfirmationError::invalid_config(format!(
                "环境变量 {key} 与已有变量大小写冲突"
            )));
        }
        env_pairs.push((upper, value.clone()));
    }
    env_pairs.sort_by(|a, b| utf16_order(&a.0, &b.0));

    // 端口与预览协议。
    if config.expected_port == Some(0) {
        return Err(ConfirmationError::invalid_config("端口必须为 1-65535"));
    }
    let preview_scheme = if config.preview_scheme.is_empty() {
        "http".to_string()
    } else {
        let s = config.preview_scheme.to_ascii_lowercase();
        if s != "http" && s != "https" {
            return Err(ConfirmationError::invalid_config(format!(
                "预览协议必须为 http 或 https，收到: {s}"
            )));
        }
        s
    };

    // 项目根规范化。
    let canon_root = canonical_key(project_root)
        .ok_or_else(|| ConfirmationError::invalid_cwd("项目根目录无法访问"))?;
    if !std::fs::metadata(&canon_root).map(|m| m.is_dir()).unwrap_or(false) {
        return Err(ConfirmationError::invalid_cwd("项目根目录不是有效目录"));
    }

    // 可执行程序解析。
    let resolved = resolve_executable(config, &canon_root)?;

    // 工作目录解析。
    let cwd_input = if config.cwd.trim().is_empty() {
        canon_root.clone()
    } else {
        let p = PathBuf::from(&config.cwd);
        if p.is_absolute() {
            p
        } else {
            canon_root.join(p)
        }
    };
    let canon_cwd = canonical_key(&cwd_input)
        .ok_or_else(|| ConfirmationError::invalid_cwd("工作目录不存在或无法访问"))?;
    if !std::fs::metadata(&canon_cwd).map(|m| m.is_dir()).unwrap_or(false) {
        return Err(ConfirmationError::invalid_cwd("工作目录不是有效目录"));
    }
    if canon_cwd.strip_prefix(&canon_root).is_err() {
        return Err(ConfirmationError::invalid_cwd("工作目录越出项目根目录"));
    }

    Ok(NormalizedRunConfig {
        version: CANONICAL_VERSION,
        project_id: config.project_id.clone(),
        executable: resolved.0,
        args: resolved.1,
        cwd: canon_cwd.to_string_lossy().to_string(),
        env_overrides: env_pairs,
        expected_port: config.expected_port,
        preview_scheme,
    })
}

/// UTF-16 码元序比较（用于环境变量键排序）。
fn utf16_order(a: &str, b: &str) -> std::cmp::Ordering {
    let a16: Vec<u16> = a.encode_utf16().collect();
    let b16: Vec<u16> = b.encode_utf16().collect();
    a16.cmp(&b16)
}

/// 解析可执行程序：绝对路径 / 项目内相对路径 / PATH 名称。
fn resolve_executable(
    config: &RunConfig,
    canon_root: &Path,
) -> Result<(String, Vec<String>), ConfirmationError> {
    let executable = config.executable.trim();
    let p = Path::new(executable);
    let has_sep = executable.contains('/') || executable.contains('\\');

    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else if has_sep {
        let full = canon_root.join(p);
        let norm = canonical_key(&full).unwrap_or(full);
        if norm.strip_prefix(canon_root).is_err() {
            return Err(ConfirmationError::invalid_config(format!(
                "可执行程序路径越出项目根目录: {executable}"
            )));
        }
        norm
    } else {
        let dirs = merged_path(config);
        find_on_path(p, &dirs).ok_or_else(|| {
            ConfirmationError::runtime_not_found(format!(
                "在 PATH 中找不到可执行程序: {executable}"
            ))
        })?
    };

    if !resolved.is_file() {
        return Err(ConfirmationError::runtime_not_found(format!(
            "可执行程序不存在: {}",
            resolved.display()
        )));
    }

    if is_batch(&resolved) {
        Ok(batch_command(&resolved, &config.args))
    } else {
        Ok((resolved.to_string_lossy().to_string(), config.args.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nexus-conf-{tag}-{}", uuid::Uuid::new_v4()))
    }

    fn make_root(tag: &str) -> PathBuf {
        let root = tmp_dir(tag);
        std::fs::create_dir_all(&root).expect("mkdir root");
        root
    }

    fn base_config(root: &Path) -> RunConfig {
        // 用系统自带 cmd.exe 作为可执行程序，避免测试依赖本机 PATH（如 node）。
        let exe = std::env::var_os("ComSpec")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\cmd.exe"));
        RunConfig {
            project_id: "p1".into(),
            executable: exe.to_string_lossy().to_string(),
            args: vec!["server.js".into()],
            cwd: root.to_string_lossy().to_string(),
            env_overrides: HashMap::new(),
            expected_port: None,
            preview_scheme: "http".into(),
        }
    }

    #[test]
    fn empty_executable_rejected() {
        let root = make_root("empty-exe");
        let mut c = base_config(&root);
        c.executable = "   ".into();
        let err = normalize(&c, &root).unwrap_err();
        assert_eq!(err.code, "invalid_config");
    }

    #[test]
    fn nul_arg_rejected() {
        let root = make_root("nul-arg");
        let mut c = base_config(&root);
        c.args.push("bad\0arg".into());
        let err = normalize(&c, &root).unwrap_err();
        assert_eq!(err.code, "invalid_config");
    }

    #[test]
    fn env_key_conflict_case_insensitive_rejected() {
        let root = make_root("env-case");
        let mut c = base_config(&root);
        c.env_overrides.insert("Path".into(), Some("a".into()));
        c.env_overrides.insert("PATH".into(), Some("b".into()));
        let err = normalize(&c, &root).unwrap_err();
        assert_eq!(err.code, "invalid_config");
    }

    #[test]
    fn env_key_with_equals_rejected() {
        let root = make_root("env-eq");
        let mut c = base_config(&root);
        c.env_overrides.insert("A=B".into(), Some("1".into()));
        assert_eq!(normalize(&c, &root).unwrap_err().code, "invalid_config");
    }

    #[test]
    fn bad_port_and_scheme_rejected() {
        let root = make_root("bad-port");
        let mut c = base_config(&root);
        c.expected_port = Some(0);
        assert_eq!(normalize(&c, &root).unwrap_err().code, "invalid_config");
        c.expected_port = None;
        c.preview_scheme = "ftp".into();
        assert_eq!(normalize(&c, &root).unwrap_err().code, "invalid_config");
    }

    #[test]
    fn cwd_outside_root_rejected() {
        let root = make_root("cwd-outside-root");
        let other = make_root("cwd-outside-other");
        let mut c = base_config(&root);
        c.cwd = other.to_string_lossy().to_string();
        let err = normalize(&c, &root).unwrap_err();
        assert_eq!(err.code, "invalid_working_directory");
    }

    #[test]
    fn missing_cwd_rejected() {
        let root = make_root("cwd-missing");
        let mut c = base_config(&root);
        c.cwd = root.join("nope").to_string_lossy().to_string();
        assert_eq!(normalize(&c, &root).unwrap_err().code, "invalid_working_directory");
    }

    #[test]
    fn empty_cwd_defaults_to_root() {
        let root = make_root("cwd-default");
        let mut c = base_config(&root);
        c.cwd = "".into();
        let n = normalize(&c, &root).unwrap();
        assert_eq!(Path::new(&n.cwd), canonical_key(&root).unwrap());
    }

    #[test]
    fn project_relative_executable_resolved_within_root() {
        let root = make_root("rel-exe");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin").join("run.exe"), "x").unwrap();
        let mut c = base_config(&root);
        c.executable = "bin\\run.exe".into();
        let n = normalize(&c, &root).unwrap();
        assert_eq!(n.executable, canonical_key(&root.join("bin").join("run.exe")).unwrap().to_string_lossy());
    }

    #[test]
    fn project_relative_executable_escaping_rejected() {
        let root = make_root("rel-exe-escape");
        let other = make_root("rel-exe-escape-other");
        std::fs::write(other.join("evil.exe"), "x").unwrap();
        let mut c = base_config(&root);
        c.executable = format!("..\\{}\\evil.exe", other.file_name().unwrap().to_string_lossy());
        assert_eq!(normalize(&c, &root).unwrap_err().code, "invalid_config");
    }

    #[test]
    fn env_sorted_and_case_normalized() {
        let root = make_root("env-sort");
        let mut c = base_config(&root);
        c.env_overrides.insert("Zeta".into(), Some("1".into()));
        c.env_overrides.insert("alpha".into(), Some("2".into()));
        c.env_overrides.insert("BETA".into(), None);
        let n = normalize(&c, &root).unwrap();
        let keys: Vec<&str> = n.env_overrides.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["ALPHA", "BETA", "ZETA"]);
        assert_eq!(n.env_overrides[1].1, None);
    }

    #[test]
    fn canonical_json_stable_across_runs() {
        let root = make_root("canonical-stable");
        std::fs::write(root.join("server.js"), "x").unwrap();
        let tools = make_root("canonical-stable-tools");
        std::fs::write(tools.join("node.exe"), "x").unwrap();
        let mut c = base_config(&root);
        c.executable = "node".into();
        c.env_overrides
            .insert("PATH".into(), Some(tools.to_string_lossy().to_string()));
        let a = normalize(&c, &root).unwrap();
        let b = normalize(&c, &root).unwrap();
        assert_eq!(canonical_json(&a), canonical_json(&b));
    }

    #[test]
    fn strip_verbatim_normalizes_drive_and_unc() {
        assert_eq!(strip_verbatim(r"\\?\C:\Foo\Bar"), r"c:\Foo\Bar");
        assert_eq!(strip_verbatim(r"\\?\UNC\HOST\Share\Dir"), r"\\host\share\Dir");
    }

    #[test]
    fn confirm_grant_and_verify_success() {
        let root = make_root("verify-ok");
        std::fs::write(root.join("server.js"), "x").unwrap();
        let session = ConfirmationSession::new([7u8; 32]);
        let c = base_config(&root);
        let preview = session.prepare(&c, &root).unwrap();
        let grant = session.confirm(&preview.confirmation_id).unwrap();
        assert!(!grant.confirmation_hash.is_empty());
        assert_eq!(session.pending_count(), 0);
        let verified = session.verify(&c, &root, &grant.confirmation_hash).unwrap();
        assert_eq!(verified.project_id, "p1");
    }

    #[test]
    fn confirm_ticket_single_use() {
        let root = make_root("ticket-once");
        std::fs::write(root.join("server.js"), "x").unwrap();
        let session = ConfirmationSession::new([1u8; 32]);
        let c = base_config(&root);
        let preview = session.prepare(&c, &root).unwrap();
        assert!(session.confirm(&preview.confirmation_id).is_ok());
        let err = session.confirm(&preview.confirmation_id).unwrap_err();
        assert_eq!(err.code, "confirmation_required");
    }

    #[test]
    fn ticket_expires_after_ttl() {
        let root = make_root("ticket-ttl");
        std::fs::write(root.join("server.js"), "x").unwrap();
        let session = ConfirmationSession::with_ttl(Duration::from_millis(20));
        let c = base_config(&root);
        let preview = session.prepare(&c, &root).unwrap();
        std::thread::sleep(Duration::from_millis(60));
        let err = session.confirm(&preview.confirmation_id).unwrap_err();
        assert_eq!(err.code, "confirmation_required");
    }

    #[test]
    fn concurrent_confirm_only_one_succeeds() {
        let root = make_root("ticket-concurrent");
        std::fs::write(root.join("server.js"), "x").unwrap();
        let session = std::sync::Arc::new(ConfirmationSession::new([2u8; 32]));
        let c = base_config(&root);
        let preview = session.prepare(&c, &root).unwrap();
        let id = preview.confirmation_id.clone();
        let mut handles = Vec::new();
        for _ in 0..8 {
            let s = session.clone();
            let id = id.clone();
            handles.push(std::thread::spawn(move || s.confirm(&id).is_ok()));
        }
        let mut ok_count = 0;
        for h in handles {
            if h.join().unwrap() {
                ok_count += 1;
            }
        }
        assert_eq!(ok_count, 1);
    }

    #[test]
    fn config_change_invalidates_hash() {
        let root = make_root("hash-config-change");
        std::fs::write(root.join("server.js"), "x").unwrap();
        let session = ConfirmationSession::new([3u8; 32]);
        let mut c1 = base_config(&root);
        let preview = session.prepare(&c1, &root).unwrap();
        let grant = session.confirm(&preview.confirmation_id).unwrap();
        c1.args = vec!["other.js".into()];
        let err = session.verify(&c1, &root, &grant.confirmation_hash).unwrap_err();
        assert_eq!(err.code, "confirmation_required");
    }

    #[test]
    fn path_resolution_change_invalidates_hash() {
        let root = make_root("hash-path-change");
        std::fs::write(root.join("server.js"), "x").unwrap();
        let tools_a = make_root("hash-path-change-a");
        let tools_b = make_root("hash-path-change-b");
        std::fs::write(tools_a.join("node.exe"), "x").unwrap();
        std::fs::write(tools_b.join("node.exe"), "x").unwrap();
        let session = ConfirmationSession::new([4u8; 32]);
        let mut c = base_config(&root);
        c.env_overrides
            .insert("PATH".into(), Some(tools_a.to_string_lossy().to_string()));
        let preview = session.prepare(&c, &root).unwrap();
        let grant = session.confirm(&preview.confirmation_id).unwrap();
        // 用户改动 PATH 覆盖，其他字段不变 → 哈希必须失效。
        let mut c2 = c.clone();
        c2.env_overrides
            .insert("PATH".into(), Some(tools_b.to_string_lossy().to_string()));
        assert_eq!(
            session
                .verify(&c2, &root, &grant.confirmation_hash)
                .unwrap_err()
                .code,
            "confirmation_required"
        );
    }

    #[test]
    fn hash_not_reusable_across_sessions() {
        let root = make_root("hash-cross-session");
        std::fs::write(root.join("server.js"), "x").unwrap();
        let s1 = ConfirmationSession::new([5u8; 32]);
        let s2 = ConfirmationSession::new([6u8; 32]);
        let c = base_config(&root);
        let preview = s1.prepare(&c, &root).unwrap();
        let grant = s1.confirm(&preview.confirmation_id).unwrap();
        assert_eq!(s2.verify(&c, &root, &grant.confirmation_hash).unwrap_err().code, "confirmation_required");
    }

    #[test]
    fn tampered_hash_rejected() {
        let root = make_root("hash-tamper");
        std::fs::write(root.join("server.js"), "x").unwrap();
        let session = ConfirmationSession::new([8u8; 32]);
        let c = base_config(&root);
        let preview = session.prepare(&c, &root).unwrap();
        let grant = session.confirm(&preview.confirmation_id).unwrap();
        let mut bad = grant.confirmation_hash.clone();
        let first = bad.as_bytes()[0];
        let replacement = if first == b'A' { b'B' } else { b'A' };
        bad.replace_range(0..1, &(replacement as char).to_string());
        assert_eq!(session.verify(&c, &root, &bad).unwrap_err().code, "confirmation_required");
    }

    #[test]
    fn redacted_summary_hides_sensitive_env() {
        let root = make_root("redact");
        std::fs::write(root.join("server.js"), "x").unwrap();
        let mut c = base_config(&root);
        c.env_overrides.insert("API_TOKEN".into(), Some("super-secret-value".into()));
        c.env_overrides.insert("PORT".into(), Some("3000".into()));
        let preview = ConfirmationSession::new([9u8; 32]).prepare(&c, &root).unwrap();
        let summary = preview.summary.to_string();
        assert!(!summary.contains("super-secret-value"));
        assert!(summary.contains("字符"));
        assert!(summary.contains("3000"));
    }

    #[test]
    fn hmac_sha256_known_vector() {
        // RFC 4231 test case 1: key=0x0b*20, data="Hi There"
        let key = [0x0bu8; 20];
        let data = b"Hi There";
        let got = hmac_sha256(&key, data);
        let expected = [
            0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b,
            0xf1, 0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c,
            0x2e, 0x32, 0xcf, 0xf7,
        ];
        assert_eq!(got, expected);
    }
}
