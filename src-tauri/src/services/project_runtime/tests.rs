//! 运行状态机测试：注入式进程 API + 事件收集 sink（替身见 `test_support`）。

use super::*;
use std::sync::atomic::AtomicU32;
use std::sync::mpsc;

use crate::services::run_history::InMemoryRunHistoryStore;
use crate::services::test_support::*;

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

// ---- 运行列表与历史 ----

#[test]
fn list_runs_includes_active_runs() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root1 = tmp_root("list-a");
    let root2 = tmp_root("list-b");
    let snap1 = start_run(&manager, &root1, &base_config(&root1));
    let snap2 = start_run(&manager, &root2, &base_config(&root2));
    assert_eq!(snap1.state, RunState::Running);
    assert_eq!(snap2.state, RunState::Running);

    let active = manager.list_runs(false);
    assert_eq!(active.len(), 2);
    let ids: Vec<&str> = active.iter().map(|s| s.run_id.as_str()).collect();
    assert!(ids.contains(&snap1.run_id.as_str()));
    assert!(ids.contains(&snap2.run_id.as_str()));
    // 不含已退出记录。
    assert!(active.iter().all(|s| !s.state.is_terminal()));
}

#[test]
fn list_runs_includes_exited_and_dedupes() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let manager = make_manager(api.clone(), sink.clone());
    let root = tmp_root("list-exited");
    api.set_natural_exit(0);
    let snap = start_run(&manager, &root, &base_config(&root));
    assert!(wait_until(
        || manager
            .get_run(&snap.run_id)
            .map(|s| s.state.is_terminal())
            .unwrap_or(false),
        Duration::from_secs(3)
    ));
    let all = manager.list_runs(true);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].run_id, snap.run_id);
    assert!(all[0].state.is_terminal());
    // run_id 不重复。
    let ids: Vec<&str> = all.iter().map(|s| s.run_id.as_str()).collect();
    assert_eq!(
        ids.len(),
        ids.iter().collect::<std::collections::HashSet<_>>().len()
    );
    // include_exited=false 时不含已退出。
    let active = manager.list_runs(false);
    assert!(active.iter().all(|s| !s.state.is_terminal()) || active.is_empty());
}

#[test]
fn history_recorded_on_terminal() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let history = Arc::new(InMemoryRunHistoryStore::new());
    let manager = make_manager_with(api.clone(), sink.clone(), history.clone());
    let root = tmp_root("hist");
    api.set_natural_exit(0);
    let snap = start_run(&manager, &root, &base_config(&root));
    assert!(wait_until(
        || manager
            .get_run(&snap.run_id)
            .map(|s| s.state.is_terminal())
            .unwrap_or(false),
        Duration::from_secs(3)
    ));
    let recorded = history.list(10);
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].run_id, snap.run_id);
    assert!(recorded[0].state.is_terminal());
}

#[test]
fn load_history_restores_terminal_only() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let history = Arc::new(InMemoryRunHistoryStore::new());
    // 第一个会话：运行并退出 → 历史记录。
    {
        let manager = make_manager_with(api.clone(), sink.clone(), history.clone());
        let root = tmp_root("hist-load");
        api.set_natural_exit(0);
        let snap = start_run(&manager, &root, &base_config(&root));
        assert!(wait_until(
            || manager
                .get_run(&snap.run_id)
                .map(|s| s.state.is_terminal())
                .unwrap_or(false),
            Duration::from_secs(3)
        ));
        manager.shutdown_all();
    }
    // 模拟应用重启：新 manager 加载历史。
    let manager2 = make_manager_with(api.clone(), sink.clone(), history.clone());
    manager2.load_history();
    let all = manager2.list_runs(true);
    assert_eq!(all.len(), 1);
    assert!(all[0].state.is_terminal());
    assert_eq!(all[0].pid, None, "历史记录不得报告运行中 PID");
    // 不允许误报为活动实例。
    assert!(manager2
        .list_runs(false)
        .iter()
        .all(|s| !s.state.is_terminal()));
}

#[test]
fn history_keeps_project_run_out_of_active() {
    let api = FakeApi::new();
    let sink = TestSink::new();
    let history = Arc::new(InMemoryRunHistoryStore::new());
    let manager = make_manager_with(api.clone(), sink.clone(), history.clone());
    // 预置历史记录（exited）。
    let hist_snap = RunSnapshot {
        run_id: "hist-run-1".into(),
        project_id: "p1".into(),
        state: RunState::Exited,
        cwd: "C:\\proj".into(),
        pid: None,
        started_at: Some(1),
        exit_code: Some(0),
        error_code: None,
        error_message: None,
        stop_reason: None,
        summary: serde_json::json!({
            "executable": "node",
            "args": [],
            "cwd": "C:\\proj",
            "env": {},
            "expected_port": null,
            "preview_scheme": "http",
        }),
    };
    history.record(&hist_snap, "key-hist").unwrap();
    manager.load_history();
    // 历史中的 run_id 在活动列表中不存在。
    let active = manager.list_runs(false);
    assert!(active.iter().all(|s| s.run_id != "hist-run-1"));
    let all = manager.list_runs(true);
    assert!(all.iter().any(|s| s.run_id == "hist-run-1"));
}
