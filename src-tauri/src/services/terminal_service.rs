//! 内置终端服务：基于 portable-pty（Windows ConPTY）管理 Shell 会话。
//!
//! 设计要点：
//! - 每个会话 = 一个 PTY + 一个子进程（PowerShell / cmd）+ 一个读取线程。
//! - 输出经 Tauri Channel 推送（原始字节），输入经命令写入 master。
//! - 读取线程 EOF 后回收会话并发送退出事件，杜绝孤儿进程。
//! - Shell 白名单 + cwd 存在性校验，避免任意命令注入。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
use tauri::ipc::Channel;
use tauri::Manager;

use crate::error::AppError;

/// 后端 → 前端事件流（Channel 内传递，不占用全局事件名）。
/// 注意：tag 必须小写（snake_case），前端按 ev.type === "output"/"exit" 匹配。
#[derive(serde::Serialize, Clone)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TerminalEvent {
    /// 终端输出原始字节（可能非 UTF-8，前端按字节写入 xterm）。
    Output(Vec<u8>),
    /// 进程退出（退出码，未知时为 null）。
    Exit(Option<i32>),
}

/// 会话信息（供 terminal_list 返回）。
#[derive(serde::Serialize, Clone)]
pub struct TerminalSessionInfo {
    pub session_id: u64,
    pub cwd: String,
    pub shell: String,
}

/// 单个终端会话：master（resize）、writer（输入）、child（kill/wait）。
pub struct TerminalSession {
    pub id: u64,
    pub cwd: PathBuf,
    pub shell: String,
    pub master: Mutex<Box<dyn MasterPty + Send>>,
    pub writer: Mutex<Box<dyn Write + Send>>,
    pub child: Mutex<Option<Box<dyn Child + Send + Sync>>>,
}

/// 终端运行时：会话表 + 自增 ID。
pub struct TerminalRuntime {
    pub sessions: Mutex<HashMap<u64, TerminalSession>>,
    pub next_id: AtomicU64,
}

impl Default for TerminalRuntime {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

/// 单次输入最大字节数，防止超大 payload。
const MAX_WRITE_BYTES: usize = 64 * 1024;
/// 读取缓冲大小。
const READ_BUF_SIZE: usize = 64 * 1024;

/// Shell 白名单：kind -> 可执行程序 + 固定参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSpec {
    pub program: String,
    pub args: Vec<String>,
}

pub fn resolve_shell(kind: &str) -> Result<ShellSpec, AppError> {
    match kind {
        "powershell" => Ok(ShellSpec {
            program: "powershell.exe".to_string(),
            args: vec![],
        }),
        "cmd" => Ok(ShellSpec {
            program: "cmd.exe".to_string(),
            args: vec![],
        }),
        "gitbash" => {
            let bash = detect_git_bash().ok_or_else(|| {
                AppError::new(
                    "shell_unavailable",
                    "未检测到 Git Bash，请安装 Git for Windows",
                )
            })?;
            Ok(ShellSpec {
                program: bash,
                // 交互式登录 shell，加载用户 profile（.bash_profile）。
                args: vec!["--login".to_string(), "-i".to_string()],
            })
        }
        "wsl" => {
            if !detect_wsl() {
                return Err(AppError::new(
                    "shell_unavailable",
                    "未检测到 WSL，请安装 WSL 并启用至少一个发行版",
                ));
            }
            Ok(ShellSpec {
                program: "wsl.exe".to_string(),
                args: vec![],
            })
        }
        _ => Err(AppError::new(
            "invalid_shell",
            format!("不支持的 Shell: {kind}"),
        )),
    }
}

/// 在 PATH 中查找可执行文件。
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// 探测 Git Bash 的 bash.exe：优先 PATH 命中，其次 Git 常见安装路径。
pub fn detect_git_bash() -> Option<String> {
    if let Some(p) = find_on_path("bash.exe") {
        return Some(p.to_string_lossy().to_string());
    }
    let roots = [
        std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string()),
        std::env::var("ProgramFiles(x86)")
            .unwrap_or_else(|_| r"C:\Program Files (x86)".to_string()),
    ];
    for root in roots {
        for sub in ["Git\\bin\\bash.exe", "Git\\usr\\bin\\bash.exe"] {
            let p = PathBuf::from(&root).join(sub);
            if p.is_file() {
                return Some(p.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// 探测 WSL：wsl.exe 存在且至少一个发行版已安装。
pub fn detect_wsl() -> bool {
    let wsl = PathBuf::from(r"C:\Windows\System32\wsl.exe");
    if !wsl.is_file() {
        return false;
    }
    match std::process::Command::new(&wsl)
        .args(["--list", "--quiet"])
        .output()
    {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Shell 信息（终端页 Shell 下拉的数据源）。
#[derive(serde::Serialize, Clone)]
pub struct ShellInfo {
    pub id: String,
    pub label: String,
    pub available: bool,
}

/// 可用 Shell 探测列表：内置两个恒可用，Git Bash / WSL 按本机检测结果。
pub fn list_shells() -> Vec<ShellInfo> {
    vec![
        ShellInfo {
            id: "powershell".into(),
            label: "PowerShell".into(),
            available: true,
        },
        ShellInfo {
            id: "cmd".into(),
            label: "CMD".into(),
            available: true,
        },
        ShellInfo {
            id: "gitbash".into(),
            label: "Git Bash".into(),
            available: detect_git_bash().is_some(),
        },
        ShellInfo {
            id: "wsl".into(),
            label: "WSL".into(),
            available: detect_wsl(),
        },
    ]
}

/// 解析启动目录：必须为已存在目录，否则回退用户主目录。
fn resolve_cwd(cwd: Option<PathBuf>) -> Result<PathBuf, AppError> {
    if let Some(p) = cwd {
        if p.exists() && p.is_dir() {
            return Ok(p);
        }
    }
    let home = std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    Ok(home)
}

/// 打开 PTY 并启动 Shell（不含会话表与读取线程，便于测试）。
fn open_session_inner(
    shell_kind: &str,
    cwd: Option<PathBuf>,
    cols: u16,
    rows: u16,
) -> Result<(TerminalSession, Box<dyn Read + Send>), AppError> {
    let spec = resolve_shell(shell_kind)?;
    let cwd = resolve_cwd(cwd)?;

    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::new("pty_open", e.to_string()))?;

    let mut cmd = CommandBuilder::new(&spec.program);
    for a in &spec.args {
        cmd.arg(a);
    }
    if shell_kind == "wsl" {
        // WSL 默认进入 Linux 主目录，显式用 --cd 让 Windows 侧 cwd 生效。
        cmd.arg("--cd");
        cmd.arg(cwd.to_string_lossy().to_string());
    }
    cmd.cwd(&cwd);
    cmd.env("TERM", "xterm-256color");
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| AppError::new("pty_spawn", e.to_string()))?;
    // slave 必须在 spawn 后立即释放，否则 ConPTY 句柄泄漏。
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| AppError::new("pty_reader", e.to_string()))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| AppError::new("pty_writer", e.to_string()))?;

    Ok((
        TerminalSession {
            id: 0, // 由调用方分配
            cwd,
            shell: spec.program,
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(Some(child)),
        },
        reader,
    ))
}

/// 启动一个新终端会话：创建 PTY、启动 Shell、挂载读取线程。
/// 输出与退出事件经 `channel` 推送。
pub fn spawn_session(
    app: &tauri::AppHandle,
    shell_kind: &str,
    cwd: Option<PathBuf>,
    cols: u16,
    rows: u16,
    channel: Channel<TerminalEvent>,
) -> Result<TerminalSessionInfo, AppError> {
    let (mut session, reader) = open_session_inner(shell_kind, cwd, cols, rows)?;

    let id = {
        let runtime = &app.state::<crate::AppState>().terminal;
        let id = runtime.next_id.fetch_add(1, Ordering::SeqCst);
        session.id = id;
        runtime
            .sessions
            .lock()
            .expect("terminal sessions lock")
            .insert(id, session);
        id
    };
    let info = {
        let st = app.state::<crate::AppState>();
        let sessions = st.terminal.sessions.lock().expect("terminal sessions lock");
        let s = sessions.get(&id).expect("just inserted terminal session");
        TerminalSessionInfo {
            session_id: id,
            cwd: s.cwd.to_string_lossy().to_string(),
            shell: s.shell.clone(),
        }
    };

    // 读取线程：输出推送（EOF 或失联时退出）。
    let app2 = app.clone();
    let channel2 = channel.clone();
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = vec![0u8; READ_BUF_SIZE];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if channel2
                        .send(TerminalEvent::Output(buf[..n].to_vec()))
                        .is_err()
                    {
                        // 前端已断开，结束读取。
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("[terminal] session {id} read error: {e}");
                    break;
                }
            }
        }
        // EOF：尽力回收会话（若尚未被监视线程清理）。
        let exit_code = {
            let st = app2.state::<crate::AppState>();
            let removed = st
                .terminal
                .sessions
                .lock()
                .expect("terminal sessions lock")
                .remove(&id);
            if let Some(sess) = removed {
                if let Some(mut child) = sess.child.lock().expect("child lock").take() {
                    child.wait().ok().map(|s| s.exit_code() as i32)
                } else {
                    None
                }
            } else {
                None
            }
        };
        let _ = channel2.send(TerminalEvent::Exit(exit_code));
    });

    // 监视线程：轮询子进程退出。ConPTY 的 conhost 在 shell 退出后仍保持输出
    // 管道打开，读取线程会一直阻塞；必须主动检测退出并回收会话（drop master
    // 触发 ClosePseudoConsole），否则 conhost 成为孤儿进程。
    let app3 = app.clone();
    let channel3 = channel.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let st = app3.state::<crate::AppState>();
            let mut sessions = st.terminal.sessions.lock().expect("terminal sessions lock");
            let Some(sess) = sessions.get(&id) else {
                // 会话已被读取线程或 close_session 清理。
                return;
            };
            let exited = {
                let mut child_guard = sess.child.lock().expect("child lock");
                match child_guard.as_mut() {
                    Some(c) => c.try_wait().ok().flatten().is_some(),
                    None => true,
                }
            };
            if exited {
                let removed = sessions.remove(&id);
                drop(sessions);
                if let Some(sess) = removed {
                    let exit_code =
                        if let Some(mut child) = sess.child.lock().expect("child lock").take() {
                            child.wait().ok().map(|s| s.exit_code() as i32)
                        } else {
                            None
                        };
                    let _ = channel3.send(TerminalEvent::Exit(exit_code));
                }
                return;
            }
        }
    });

    Ok(info)
}

/// 向会话写入输入（UTF-8 字节流）。
pub fn write_session(runtime: &TerminalRuntime, id: u64, data: &str) -> Result<(), AppError> {
    if data.as_bytes().len() > MAX_WRITE_BYTES {
        return Err(AppError::new("input_too_large", "单次输入超过 64KB 限制"));
    }
    let sessions = runtime.sessions.lock().expect("terminal sessions lock");
    let sess = sessions
        .get(&id)
        .ok_or_else(|| AppError::new("session_not_found", "终端会话不存在或已关闭"))?;
    let mut w = sess.writer.lock().expect("writer lock");
    w.write_all(data.as_bytes())
        .map_err(|e| AppError::new("pty_write", e.to_string()))?;
    w.flush()
        .map_err(|e| AppError::new("pty_write", e.to_string()))?;
    Ok(())
}

/// 调整会话窗口大小（列/行）。
pub fn resize_session(
    runtime: &TerminalRuntime,
    id: u64,
    cols: u16,
    rows: u16,
) -> Result<(), AppError> {
    let sessions = runtime.sessions.lock().expect("terminal sessions lock");
    let sess = sessions
        .get(&id)
        .ok_or_else(|| AppError::new("session_not_found", "终端会话不存在或已关闭"))?;
    let master = sess.master.lock().expect("master lock");
    master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::new("pty_resize", e.to_string()))?;
    Ok(())
}

/// 结束会话：终止子进程并移除会话（幂等）。
pub fn close_session(runtime: &TerminalRuntime, id: u64) -> Result<(), AppError> {
    let sess = runtime
        .sessions
        .lock()
        .expect("terminal sessions lock")
        .remove(&id);
    if let Some(sess) = sess {
        if let Some(mut child) = sess.child.lock().expect("child lock").take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    Ok(())
}

/// 列出存活会话。
pub fn list_sessions(runtime: &TerminalRuntime) -> Vec<TerminalSessionInfo> {
    runtime
        .sessions
        .lock()
        .expect("terminal sessions lock")
        .iter()
        .map(|(id, s)| TerminalSessionInfo {
            session_id: *id,
            cwd: s.cwd.to_string_lossy().to_string(),
            shell: s.shell.clone(),
        })
        .collect()
}

/// 应用退出兜底：终止所有会话，避免孤儿进程。
pub fn shutdown_all(runtime: &TerminalRuntime) {
    let ids: Vec<u64> = runtime
        .sessions
        .lock()
        .expect("terminal sessions lock")
        .keys()
        .copied()
        .collect();
    for id in ids {
        let _ = close_session(runtime, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Channel 事件序列化的 type 字段必须是小写（与前端 ev.type === "output"/"exit" 匹配）。
    #[test]
    fn terminal_event_type_is_lowercase() {
        let out = serde_json::to_value(TerminalEvent::Output(vec![1, 2, 3])).unwrap();
        assert_eq!(out["type"], "output");
        assert_eq!(out["data"], serde_json::json!([1, 2, 3]));

        let exit = serde_json::to_value(TerminalEvent::Exit(Some(0))).unwrap();
        assert_eq!(exit["type"], "exit");
        assert_eq!(exit["data"], 0);
    }

    #[test]
    fn shell_whitelist() {
        assert_eq!(
            resolve_shell("powershell").unwrap().program,
            "powershell.exe"
        );
        assert_eq!(resolve_shell("cmd").unwrap().program, "cmd.exe");
        assert!(resolve_shell("bash").is_err());
        assert!(resolve_shell("").is_err());
        // Git Bash：已安装返回 bash 路径，未安装返回 shell_unavailable。
        match resolve_shell("gitbash") {
            Ok(s) => assert!(s.program.ends_with("bash.exe")),
            Err(e) => assert_eq!(e.code, "shell_unavailable"),
        }
        // WSL：可用返回 wsl.exe，不可用返回 shell_unavailable。
        match resolve_shell("wsl") {
            Ok(s) => assert_eq!(s.program, "wsl.exe"),
            Err(e) => assert_eq!(e.code, "shell_unavailable"),
        }
        // 探测列表：内置两个恒可用。
        let shells = list_shells();
        assert!(shells.iter().any(|s| s.id == "powershell" && s.available));
        assert!(shells.iter().any(|s| s.id == "cmd" && s.available));
    }

    #[test]
    fn cwd_falls_back_to_existing() {
        // 不存在的目录回退到用户主目录
        let resolved = resolve_cwd(Some(PathBuf::from("Z:\\no\\such\\dir-xyz-123"))).unwrap();
        assert!(resolved.is_dir());
        // 传入合法目录则保留
        let tmp = std::env::temp_dir();
        let resolved2 = resolve_cwd(Some(tmp.clone())).unwrap();
        assert_eq!(resolved2, tmp);
    }

    /// 真实 PTY 回环测试：spawn cmd，写 echo 命令，读到回显后 kill。
    #[test]
    #[cfg(windows)]
    fn pty_echo_roundtrip() {
        let (session, mut reader) = open_session_inner("cmd", None, 24, 80).unwrap();

        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx
                            .send(String::from_utf8_lossy(&chunk[..n]).to_string())
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        {
            let mut w = session.writer.lock().unwrap();
            writeln!(w, "echo PTY_OK\r").unwrap();
            w.flush().unwrap();
        }

        let mut buf = String::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !buf.contains("PTY_OK") && std::time::Instant::now() < deadline {
            match rx.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(s) => buf.push_str(&s),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            }
        }

        if let Some(mut child) = session.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        assert!(buf.contains("PTY_OK"), "未收到 PTY 回显, 收到: {buf}");
    }
}
