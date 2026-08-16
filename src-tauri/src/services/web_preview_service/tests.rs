//! 预览目标解析与 PreviewService 集成测试。

use super::*;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::services::project_runtime::RunState;
use crate::services::test_support::*;

// ---- 目标解析纯函数 ----

#[test]
fn port_from_args_forms() {
    assert_eq!(port_from_args(&["--port=3000".into()]), Some(3000));
    assert_eq!(port_from_args(&["--port".into(), "3000".into()]), Some(3000));
    assert_eq!(port_from_args(&["-p".into(), "8080".into()]), Some(8080));
    assert_eq!(port_from_args(&["run".into(), "dev".into()]), None);
    assert_eq!(port_from_args(&["--port".into(), "abc".into()]), None);
    assert_eq!(port_from_args(&["--port=0".into()]), None);
    assert_eq!(port_from_args(&["--port".into(), "70000".into()]), None);
}

#[test]
fn local_url_from_log_preserves_https_and_path() {
    let log = "Server ready at https://localhost:8443/secure/dashboard?tab=1";
    let c = local_url_from_log(log).expect("url");
    assert_eq!(c.scheme, "https");
    assert_eq!(c.host, "localhost");
    assert_eq!(c.port, 8443);
    assert_eq!(c.path, "/secure/dashboard?tab=1");
    assert_eq!(c.source, PreviewSource::Log);
}

#[test]
fn local_url_from_log_normalizes_wildcard_hosts() {
    let c = local_url_from_log("Listening on http://0.0.0.0:8080/").expect("url");
    assert_eq!(c.host, "127.0.0.1");
    assert_eq!(c.port, 8080);
    let c = local_url_from_log("Vite dev server: http://[::1]:5173/").expect("url");
    assert_eq!(c.host, "127.0.0.1");
    assert_eq!(c.port, 5173);
}

#[test]
fn local_url_from_log_defaults_port_by_scheme() {
    let c = local_url_from_log("http://localhost/").expect("url");
    assert_eq!(c.port, 80);
    let c = local_url_from_log("https://127.0.0.1/health").expect("url");
    assert_eq!(c.port, 443);
}

#[test]
fn local_url_from_log_rejects_remote_hosts() {
    assert!(local_url_from_log("Open http://example.com:8080/").is_none());
    assert!(local_url_from_log("fetch https://api.example.com/v1").is_none());
}

#[test]
fn local_url_from_log_takes_first() {
    let log = "warn: http://localhost:9999/old\nServer: http://127.0.0.1:3000/app";
    let c = local_url_from_log(log).expect("url");
    assert_eq!(c.port, 9999);
    assert_eq!(c.path, "/old");
}

#[test]
fn resolve_target_priority_config_then_args_then_log() {
    let args = ["--port".to_string(), "8080".to_string()];
    let log = "http://localhost:5173/".to_string();

    // config 优先。
    let c = resolve_target(Some(3000), "https", &args, &log).expect("config");
    assert_eq!(c.source, PreviewSource::Config);
    assert_eq!(c.port, 3000);
    assert_eq!(c.scheme, "https");
    assert_eq!(c.host, "127.0.0.1");

    // 无配置端口时取参数。
    let c = resolve_target(None, "http", &args, &log).expect("args");
    assert_eq!(c.source, PreviewSource::Args);
    assert_eq!(c.port, 8080);

    // 无配置端口且无参数时取日志。
    let c = resolve_target(None, "http", &[], &log).expect("log");
    assert_eq!(c.source, PreviewSource::Log);
    assert_eq!(c.port, 5173);
    assert_eq!(c.path, "/");
}

#[test]
fn resolve_target_none_when_nothing_available() {
    assert!(resolve_target(None, "http", &[], "").is_none());
    assert!(resolve_target(Some(0), "http", &[], "no url here").is_none());
}

#[test]
fn format_url_builds_full_url() {
    let c = UrlCandidate {
        scheme: "https".into(),
        host: "127.0.0.1".into(),
        port: 8443,
        path: "/secure?q=1".into(),
        source: PreviewSource::Log,
    };
    assert_eq!(format_url(&c), "https://127.0.0.1:8443/secure?q=1");
    let c = UrlCandidate {
        scheme: "http".into(),
        host: "127.0.0.1".into(),
        port: 3000,
        path: String::new(),
        source: PreviewSource::Config,
    };
    assert_eq!(format_url(&c), "http://127.0.0.1:3000");
}

// ---- PreviewService 集成 ----

/// 注入式端口探测：port → 监听 PID。
struct FakePortProbe {
    listeners: Mutex<HashMap<u16, u32>>,
}

impl FakePortProbe {
    fn new() -> Arc<Self> {
        Arc::new(Self { listeners: Mutex::new(HashMap::new()) })
    }
    fn listen(&self, port: u16, pid: u32) {
        self.listeners.lock().unwrap().insert(port, pid);
    }
}

impl PortProbe for FakePortProbe {
    fn listening_pid(&self, port: u16) -> Option<u32> {
        self.listeners.lock().unwrap().get(&port).copied()
    }
}

fn make_preview(manager: Arc<RuntimeManager>, probe: Arc<FakePortProbe>) -> PreviewService {
    PreviewService::new(manager, probe)
}

#[test]
fn preview_confirmed_when_port_listening_in_job() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let probe = FakePortProbe::new();
    probe.listen(3000, 4242); // 4242 在 job_pids 内。
    let service = make_preview(manager.clone(), probe);
    let root = tmp_root("pv-confirmed");
    let config = base_config(&root); // expected_port = 3000
    let snap = start_run(&manager, &root, &config);
    assert_eq!(snap.state, RunState::Running);

    let target = service.open_preview(&snap.run_id).expect("preview");
    assert_eq!(target.ownership, PortOwnership::Confirmed);
    assert_eq!(target.url, "http://127.0.0.1:3000");
    assert_eq!(target.source, PreviewSource::Config);
    assert_eq!(target.project_id, "p1");
    // 就绪事件已发布。
    let previews = sink.wait_previews(Duration::from_secs(2));
    assert_eq!(previews.len(), 1);
    assert_eq!(previews[0].run_id, snap.run_id);
}

#[test]
fn preview_unavailable_when_port_not_listening() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let probe = FakePortProbe::new(); // 无监听。
    let service = make_preview(manager.clone(), probe);
    let root = tmp_root("pv-not-listening");
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);

    let err = service.open_preview(&snap.run_id).unwrap_err();
    assert_eq!(err.code, "preview_unavailable");
    assert!(err.message.contains("未监听"));
}

#[test]
fn preview_unavailable_when_no_target() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let probe = FakePortProbe::new();
    probe.listen(3000, 4242);
    let service = make_preview(manager.clone(), probe);
    let root = tmp_root("pv-no-target");
    let mut config = base_config(&root);
    config.expected_port = None;
    config.args = vec!["serve".into()];
    let snap = start_run(&manager, &root, &config);

    let err = service.open_preview(&snap.run_id).unwrap_err();
    assert_eq!(err.code, "preview_unavailable");
}

#[test]
fn preview_log_url_preserves_https_and_path() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let probe = FakePortProbe::new();
    probe.listen(8443, 4242);
    let service = make_preview(manager.clone(), probe);
    let root = tmp_root("pv-log-url");
    let mut config = base_config(&root);
    config.expected_port = None;
    config.args = vec!["serve".into()];
    api.spawn_stdout(b"Server ready at https://localhost:8443/secure/dashboard\n");
    let snap = start_run(&manager, &root, &config);
    // 等待日志事件进入总线。
    sink.wait_outputs(1, Duration::from_secs(3));

    let target = service.open_preview(&snap.run_id).expect("preview");
    assert_eq!(target.source, PreviewSource::Log);
    assert_eq!(target.scheme, "https");
    assert_eq!(target.port, 8443);
    assert_eq!(target.path, "/secure/dashboard");
    assert_eq!(target.url, "https://localhost:8443/secure/dashboard");
}

#[test]
fn preview_unconfirmed_when_pid_not_in_job() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let probe = FakePortProbe::new();
    probe.listen(3000, 9999); // 9999 不在 job_pids（4242）。
    let service = make_preview(manager.clone(), probe);
    let root = tmp_root("pv-unconfirmed");
    let config = base_config(&root);
    let snap = start_run(&manager, &root, &config);

    let target = service.open_preview(&snap.run_id).expect("preview");
    assert_eq!(target.ownership, PortOwnership::Unconfirmed);
    assert_eq!(target.url, "http://127.0.0.1:3000");
}

#[test]
fn preview_run_not_found() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let probe = FakePortProbe::new();
    let service = make_preview(manager.clone(), probe);
    let err = service.open_preview("no-such-run").unwrap_err();
    assert_eq!(err.code, "run_not_found");
}

#[test]
fn preview_absent_after_run_exits() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let probe = FakePortProbe::new();
    probe.listen(3000, 4242);
    let service = make_preview(manager.clone(), probe);
    let root = tmp_root("pv-exited");
    let config = base_config(&root);
    api.set_natural_exit(0);
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
    // 运行已终态 → 不可预览。
    let err = service.open_preview(&snap.run_id).unwrap_err();
    assert_eq!(err.code, "run_not_found");
}
