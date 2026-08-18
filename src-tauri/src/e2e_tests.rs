//! E2E 集成测试：使用真实 SQLite 数据库与真实临时文件系统，
//! 直接构造 `AppState`（字段均公开）并走服务层/仓库层全链路，
//! 验证核心用户流程的数据与文件协同：
//! 1. 导入文件 → 2. 打开编辑并保存 → 3. 移入回收站 → 4. 恢复 →
//! 5. 创建备份 → 6. 从备份恢复 → 数据一致。
//!
//! 说明：命令层对 `AppHandle`（事件广播）的包装由各模块单元测试覆盖；
//! 此处验证的是命令层之下的完整数据链路（等价于命令层实际执行逻辑）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use crate::db::connection::open;
use crate::db::migrations::run_migrations;
use crate::services::project_runtime::RuntimeManager;
use crate::services::run_history::InMemoryRunHistoryStore;
use crate::services::terminal_service::TerminalRuntime;
use crate::services::web_preview_service::build_preview_service;
use crate::services::{process_api, test_support};
use crate::{AppState, SearchRuntime};

/// 构造带真实数据库与临时目录的裸 AppState，返回 (state, root)。
fn setup_state() -> (AppState, PathBuf) {
    let root = std::env::temp_dir().join(format!("orange-e2e-{}", crate::db::models::new_id()));
    std::fs::create_dir_all(&root).expect("create temp root");
    let data_dir = root.join("data");
    let managed_dir = root.join("managed-files");
    std::fs::create_dir_all(&data_dir).expect("create data dir");
    std::fs::create_dir_all(&managed_dir).expect("create managed dir");

    let db_path = data_dir.join("workspace.db");
    let mut conn = open(&db_path).expect("open db");
    run_migrations(&mut conn).expect("migrations");

    // 运行管理器使用内存历史与注入式进程 API（E2E 不真正启动进程）
    let api: Arc<dyn process_api::Win32ProcessApi> = test_support::FakeApi::new();
    let sink = test_support::TestSink::new();
    let history = Arc::new(InMemoryRunHistoryStore::default());
    let runtime = Arc::new(RuntimeManager::new(api, sink, history));
    let preview = build_preview_service(runtime.clone());

    let state = AppState {
        data_dir: Mutex::new(data_dir),
        managed_dir: Mutex::new(managed_dir),
        conn: Arc::new(Mutex::new(conn)),
        sampler: Mutex::new(crate::services::system_service::SystemSampler::new()),
        search: SearchRuntime {
            active_queries: Mutex::new(std::collections::HashMap::new()),
            next_query_id: AtomicU64::new(1),
            scan_paused: Arc::new(AtomicBool::new(false)),
            scan_trigger: AtomicU64::new(0),
        },
        terminal: TerminalRuntime::default(),
        runtime,
        preview,
        app_usage: std::sync::Arc::new(crate::services::app_usage_service::AppUsageTracker::new()),
    };
    (state, root)
}

fn write_file(path: &Path, content: &str) {
    std::fs::write(path, content).expect("write file");
}

fn read_file(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read file")
}

/// 通过仓库层导入外部文件（等价于 import_paths 的 external 模式落库路径）。
fn import_external(conn: &rusqlite::Connection, path: &Path) -> crate::db::models::Resource {
    use crate::db::models::{new_id, ResourceKind, ResourceLocation, SourceType};
    let now = crate::db::connection::now_unix();
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".into());
    let resource = crate::db::models::Resource {
        id: new_id(),
        kind: ResourceKind::File,
        name,
        parent_id: None,
        is_favorite: false,
        is_deleted: false,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    conn.execute(
        "INSERT INTO resources
         (id, kind, name, parent_id, is_favorite, is_deleted, created_at, updated_at)
         VALUES (?1, 'file', ?2, NULL, 0, 0, ?3, ?3)",
        rusqlite::params![resource.id, resource.name, now],
    )
    .expect("insert resource");
    let loc = ResourceLocation {
        id: new_id(),
        resource_id: resource.id.clone(),
        source_type: SourceType::External,
        path: path.to_string_lossy().into_owned(),
        canonical_path: Some(path.to_string_lossy().into_owned()),
        file_size: std::fs::metadata(path).ok().map(|m| m.len() as i64),
        modified_at: std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64),
        created_at: now,
        last_verified_at: Some(now),
        content_hash: None,
        hash_algorithm: None,
        is_available: true,
    };
    conn.execute(
        "INSERT INTO resource_locations
         (id, resource_id, source_type, path, canonical_path, file_size, modified_at,
          created_at, last_verified_at, is_available)
         VALUES (?1, ?2, 'external', ?3, ?4, ?5, ?6, ?7, ?8, 1)",
        rusqlite::params![
            loc.id,
            loc.resource_id,
            loc.path,
            loc.canonical_path,
            loc.file_size,
            loc.modified_at,
            loc.created_at,
            loc.last_verified_at,
        ],
    )
    .expect("insert location");
    resource
}

#[test]
fn full_core_flow_import_edit_trash_restore_backup() {
    let (state, root) = setup_state();

    // ---- 1. 导入一个外部文件 ----
    let src = root.join("hello.txt");
    write_file(&src, "v1\n");
    let resource = {
        let conn = state.conn.lock().expect("db lock");
        import_external(&conn, &src)
    };

    // 资源入库，可通过列表查询
    let children = {
        let conn = state.conn.lock().expect("db lock");
        crate::db::repositories::list_children(&conn, None, false).expect("list children")
    };
    assert!(
        children.iter().any(|r| r.id == resource.id),
        "导入后应可列出"
    );
    let resource_id = resource.id.clone();

    // ---- 2. 打开编辑并保存 ----
    let (content, _session) = {
        let conn = state.conn.lock().expect("db lock");
        crate::services::editor_service::open_session(&conn, &resource_id, &src)
            .expect("open session")
    };
    assert_eq!(content, "v1\n");
    {
        let conn = state.conn.lock().expect("db lock");
        crate::services::editor_service::save_session(
            &conn,
            &resource_id,
            &src,
            "v2-edited\n",
            false,
        )
        .expect("save session");
    }
    assert_eq!(read_file(&src), "v2-edited\n", "磁盘文件应包含编辑后的内容");

    // ---- 3. 移入回收站（软删除）----
    {
        let conn = state.conn.lock().expect("db lock");
        let now = crate::db::connection::now_unix();
        crate::db::repositories::soft_delete(&conn, &resource_id, now).expect("soft delete");
    }
    let trash = {
        let conn = state.conn.lock().expect("db lock");
        crate::db::repositories::list_trash(&conn).expect("list trash")
    };
    assert!(
        trash.iter().any(|r| r.id == resource_id),
        "回收站应包含被删除的资源"
    );
    // 普通列表不再出现
    let children_after = {
        let conn = state.conn.lock().expect("db lock");
        crate::db::repositories::list_children(&conn, None, false).expect("list children")
    };
    assert!(!children_after.iter().any(|r| r.id == resource_id));

    // ---- 4. 从回收站恢复 ----
    {
        let conn = state.conn.lock().expect("db lock");
        let now = crate::db::connection::now_unix();
        crate::db::repositories::restore(&conn, &resource_id, now).expect("restore");
    }
    let children_restored = {
        let conn = state.conn.lock().expect("db lock");
        crate::db::repositories::list_children(&conn, None, false).expect("list children")
    };
    assert!(
        children_restored.iter().any(|r| r.id == resource_id),
        "恢复后资源应重新出现在列表"
    );

    // ---- 5. 创建备份 ----
    let (_backup_dir, backup) =
        crate::services::backup_service::create_backup(&state, true, "manual")
            .expect("create backup");
    assert_eq!(backup.status, "completed");
    assert_eq!(backup.backup_type, "full");

    // ---- 6. 从备份恢复：先改数据库数据再恢复，验证数据库回滚 ----
    // 说明：外部模式文件不参与备份（备份只含数据库 + 托管文件），
    // 因此用数据库中的资源名称变更来验证恢复后的数据一致性。
    {
        let conn = state.conn.lock().expect("db lock");
        let now = crate::db::connection::now_unix();
        crate::db::repositories::rename_resource(&conn, &resource_id, "renamed-after-backup", now)
            .expect("rename after backup");
    }
    crate::services::backup_service::restore_from_dir(&state, Path::new(&backup.path))
        .expect("restore backup");
    let restored_name = {
        let conn = state.conn.lock().expect("db lock");
        crate::db::repositories::get_resource(&conn, &resource_id)
            .expect("get resource")
            .expect("resource exists")
            .name
    };
    assert_eq!(
        restored_name, "hello.txt",
        "恢复备份后数据库中的资源名称应回到备份时的状态"
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn backup_validate_rejects_missing_dir() {
    let (state, root) = setup_state();
    let res =
        crate::services::backup_service::validate_backup(Path::new(&root.join("does-not-exist")));
    assert!(res.is_err(), "不存在的备份目录应校验失败");
    drop(state);
    let _ = std::fs::remove_dir_all(&root);
}
