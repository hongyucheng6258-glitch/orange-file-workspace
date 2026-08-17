// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod commands;
mod db;
mod error;
mod events;
mod ipc;
mod services;

#[cfg(test)]
mod e2e_tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use tauri::Manager;

use db::connection::open;
use db::migrations::run_migrations;
use ipc::CommandResult;

/// 全局搜索运行时：活动查询表 + 后台扫描控制。
pub struct SearchRuntime {
    pub active_queries: Mutex<HashMap<u64, ()>>,
    pub next_query_id: AtomicU64,
    pub scan_paused: Arc<AtomicBool>,
    pub scan_trigger: AtomicU64, // 自增触发重建
}

/// 全局应用状态：数据目录、托管目录与数据库连接。
/// data_dir 与 managed_dir 使用内部可变性，支持运行时迁移切换。
pub struct AppState {
    pub data_dir: std::sync::Mutex<PathBuf>,
    pub managed_dir: std::sync::Mutex<PathBuf>,
    pub conn: Mutex<rusqlite::Connection>,
    pub sampler: Mutex<services::system_service::SystemSampler>,
    pub search: SearchRuntime,
    pub terminal: services::terminal_service::TerminalRuntime,
    pub runtime: Arc<services::project_runtime::RuntimeManager>,
    pub preview: Arc<services::web_preview_service::PreviewService>,
}

impl AppState {
    /// 从运行时构造扫描控制句柄（共享暂停标志）。
    pub fn scan_control(&self) -> crate::services::scan_service::ScanControl {
        crate::services::scan_service::ScanControl {
            paused: self.search.scan_paused.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// 启动配置：固定位置 `%APPDATA%\com.nexus.file-workspace\config.json`。
/// 配置数据目录后，数据库/缩略图/备份等全部迁移到该目录。
#[derive(serde::Deserialize, Default)]
struct AppConfig {
    data_dir: Option<String>,
}

/// 解析数据目录：读取固定位置的 config.json，未配置时回退默认 AppData 目录。
fn resolve_data_dir(default_dir: &std::path::Path, config_path: &std::path::Path) -> PathBuf {
    let cfg = std::fs::read(config_path)
        .ok()
        .and_then(|buf| serde_json::from_str::<AppConfig>(&String::from_utf8_lossy(&buf)).ok());
    cfg.and_then(|c| c.data_dir.map(PathBuf::from))
        .unwrap_or_else(|| default_dir.to_path_buf())
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// 返回应用版本，用于前端初始化自检。
#[tauri::command]
fn app_info() -> CommandResult<serde_json::Value> {
    Ok(serde_json::json!({
        "name": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let default_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&default_data_dir)?;
            let config_path = default_data_dir.join("config.json");
            let data_dir = resolve_data_dir(&default_data_dir, &config_path);
            std::fs::create_dir_all(&data_dir)?;
            std::fs::create_dir_all(data_dir.join("thumbnails"))?;
            let db_path = data_dir.join("workspace.db");
            let conn = open(&db_path)?;
            let mut conn = conn;
            run_migrations(&mut conn)?;
            // 迁移中断恢复：清理残留临时目录或旧目录
            services::migration_service::recover_on_startup(&conn, &data_dir);
            // 托管目录：优先读取应用设置，默认 data_dir/managed-files。
            let managed_dir = db::repositories::get_setting(&conn, "managed_dir")
                .ok()
                .flatten()
                .and_then(|v| serde_json::from_str::<String>(&v).ok())
                .map(PathBuf::from)
                .unwrap_or_else(|| data_dir.join("managed-files"));
            std::fs::create_dir_all(&managed_dir)?;
            // 数据目录/托管目录可能位于 E 盘等自定义位置，
            // 需先加入 asset 协议白名单，否则缩略图和图标无法通过 convertFileSrc 加载。
            {
                let scope = app.asset_protocol_scope();
                scope.allow_directory(&data_dir, true)?;
                scope.allow_directory(&managed_dir, true)?;
                eprintln!(
                    "asset scope: data_dir={} allowed={}, managed_dir={} allowed={}",
                    data_dir.display(),
                    scope.is_allowed(&data_dir),
                    managed_dir.display(),
                    scope.is_allowed(&managed_dir)
                );
            }
            // 项目运行管理器：确认协议 + 进程生命周期 + 日志 + 运行历史。
            // 历史存储使用独立连接（WAL 支持多连接并发读写）。
            let history_store = Arc::new(services::run_history::SqliteRunHistoryStore::new(open(
                &db_path,
            )?));
            let runtime_manager = Arc::new(services::project_runtime::RuntimeManager::new(
                Arc::new(services::process_api::Win32ProcessApiImpl),
                Arc::new(commands::project_runtime::AppRunEventSink::new(
                    app.handle().clone(),
                )),
                history_store,
            ));
            // 启动时加载已退出历史（仅终态，不误报为运行中）。
            runtime_manager.load_history();
            // Web 端口预览服务：目标解析 + 监听检测 + Job 归属校验。
            let preview_service =
                services::web_preview_service::build_preview_service(runtime_manager.clone());
            app.manage(AppState {
                data_dir: Mutex::new(data_dir),
                managed_dir: Mutex::new(managed_dir),
                conn: Mutex::new(conn),
                sampler: Mutex::new(services::system_service::SystemSampler::new()),
                search: SearchRuntime {
                    active_queries: Mutex::new(HashMap::new()),
                    next_query_id: AtomicU64::new(1),
                    scan_paused: Arc::new(AtomicBool::new(false)),
                    scan_trigger: AtomicU64::new(0),
                },
                terminal: services::terminal_service::TerminalRuntime::default(),
                runtime: runtime_manager.clone(),
                preview: preview_service.clone(),
            });
            // 运行管理器：确认票据过期清理 + 停止超时清理重试。
            {
                let runtime = runtime_manager.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    runtime.sweep_expired();
                    runtime.cleanup_tick();
                });
            }
            services::watcher_service::start_managed_watcher(app.handle().clone());
            services::backup_service::start_backup_scheduler(app.handle().clone());
            // 后台构建应用索引（开始菜单 / App Paths / 卸载注册表 / Store 快捷方式），不阻塞启动
            {
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    #[cfg(windows)]
                    {
                        let entries = crate::services::app_index::windows::collect_apps();
                        // 空结果防护：所有来源临时不可读时跳过重建，避免清空既有应用索引
                        if entries.is_empty() {
                            return;
                        }
                        let data_dir = app_handle
                            .state::<AppState>()
                            .data_dir
                            .lock()
                            .expect("data dir lock")
                            .clone();
                        if let Ok(mut conn) =
                            crate::db::connection::open(&data_dir.join("workspace.db"))
                        {
                            let _ =
                                crate::services::app_index::rebuild_app_index(&mut conn, &entries);
                        }
                    }
                });
            }
            // 后台全盘扫描工作线程（目录遍历/跳过规则/检查点/代次清理/低优先级调度）
            services::scan_service::start_scan_worker(app.handle().clone());
            // 文件变化监听：已完成卷增量更新索引（Task 12）
            services::scan_service::start_watchers(app.handle().clone());
            // Keep Tauri's native drop registration intact. The experimental global
            // registration revoked it and regressed both drag-in and window input.

            // 系统托盘：关闭窗口时最小化到托盘，托盘菜单提供打开/退出。
            {
                use tauri::{
                    menu::{Menu, MenuItem},
                    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
                    WindowEvent,
                };

                fn show_main_window(app: &tauri::AppHandle) {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }

                let show_item = MenuItem::with_id(app, "show", "打开橙子", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
                let mut tray = TrayIconBuilder::with_id("main-tray").tooltip("橙子的工作台");
                if let Some(icon) = app.default_window_icon() {
                    tray = tray.icon(icon.clone());
                }
                let _tray = tray
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => show_main_window(app),
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            show_main_window(tray.app_handle());
                        }
                    })
                    .build(app)?;

                // 关闭窗口：若开启“最小化到托盘”，阻止关闭并隐藏窗口。
                let close_handle = app.handle().clone();
                if let Some(window) = app.get_webview_window("main") {
                    let close_handle = close_handle.clone();
                    window.on_window_event(move |event| {
                        if let WindowEvent::CloseRequested { api, .. } = event {
                            let minimize = close_handle
                                .state::<AppState>()
                                .conn
                                .lock()
                                .map(|conn| {
                                    services::settings_service::load_settings(&conn)
                                        .map(|s| s.general.minimize_to_tray)
                                        .unwrap_or(true)
                                })
                                .unwrap_or(true);
                            if minimize {
                                api.prevent_close();
                                if let Some(w) = close_handle.get_webview_window("main") {
                                    let _ = w.hide();
                                }
                            }
                        }
                    });
                }

                // 开机自启（--autostart）且开启“最小化到托盘”时保持隐藏驻留托盘；
                // 其余情况（手动启动等）正常显示窗口。
                let autostart_flag = std::env::args().any(|a| a == "--autostart");
                let minimize_to_tray = close_handle
                    .state::<AppState>()
                    .conn
                    .lock()
                    .map(|conn| {
                        services::settings_service::load_settings(&conn)
                            .map(|s| s.general.minimize_to_tray)
                            .unwrap_or(true)
                    })
                    .unwrap_or(true);
                if !(autostart_flag && minimize_to_tray) {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            app_info,
            commands::autostart::set_autostart,
            commands::autostart::get_autostart,
            // Tags
            commands::tags::create_tag,
            commands::tags::list_tags,
            commands::tags::get_tag,
            commands::tags::update_tag,
            commands::tags::delete_tag,
            commands::tags::add_tag_to_resource,
            commands::tags::remove_tag_from_resource,
            commands::tags::get_resource_tags,
            commands::tags::get_resources_by_tag,
            // Recent items
            commands::recent::record_resource_access,
            commands::recent::get_recent_items,
            commands::recent::remove_recent_item,
            commands::recent::clear_recent_items,
            commands::recent::cleanup_old_recent_items,
            // Workspace sessions
            commands::workspace_sessions::save_workspace_session,
            commands::workspace_sessions::get_workspace_session,
            commands::workspace_sessions::delete_workspace_session,
            commands::workspace_sessions::list_workspace_sessions,
            commands::workspace_sessions::mark_session_restored,
            // Git
            commands::git::register_git_repository,
            commands::git::get_git_repository,
            commands::git::update_git_repository_status,
            commands::git::mark_git_repository_fetched,
            commands::git::save_git_file_status,
            commands::git::get_git_file_status,
            commands::git::list_git_file_statuses,
            commands::git::clear_git_file_statuses,
            commands::git::delete_git_repository,
            // Command palette
            commands::command_palette::record_command_execution,
            commands::command_palette::get_frequent_commands,
            commands::command_palette::get_recent_commands,
            commands::command_palette::get_commands_by_category,
            commands::command_palette::search_commands,
            commands::command_palette::clear_command_history,
            commands::command_palette::delete_command_history,
            commands::command_palette::get_command_statistics,
            commands::resources::list_children,
            commands::resources::get_resource,
            commands::resources::find_resource_by_path,
            commands::resources::create_folder,
            commands::resources::rename_resource,
            commands::resources::move_resource,
            commands::resources::trash_resources,
            commands::resources::restore_resource,
            commands::resources::restore_resources,
            commands::resources::list_trash,
            commands::resources::verify_location,
            commands::import::import_paths,
            commands::import::cancel_task,
            commands::import::list_tasks,
            commands::previews::get_thumbnail,
            commands::previews::get_file_icon,
            commands::previews::get_text_preview,
            commands::previews::hash_resources,
            commands::pages::create_page,
            commands::pages::get_page,
            commands::pages::save_page_blocks,
            commands::pages::save_page_document,
            commands::pages::list_pages,
            commands::pages::rename_page,
            commands::pages::delete_page,
            commands::projects::import_project,
            commands::projects::list_projects,
            commands::projects::get_project,
            commands::projects::list_project_files,
            commands::projects::delete_project,
            commands::editor::open_file,
            commands::editor::save_file,
            commands::editor::save_file_force,
            commands::editor::discard_session,
            commands::editor::save_draft,
            commands::editor::list_recent_files,
            commands::editor::read_disk_content,
            commands::resources::toggle_favorite,
            commands::resources::list_favorites,
            commands::resources::get_ancestors,
            commands::resources::delete_permanently,
            commands::search::search_resources,
            commands::drag::drag_out,
            commands::dashboard::dashboard_stats,
            commands::backups::create_backup,
            commands::backups::list_backups,
            commands::backups::restore_backup,
            commands::backups::app_environment,
            commands::backups::delete_backup,
            commands::backups::export_backup,
            commands::backups::reveal_backups,
            commands::backups::validate_backup,
            commands::settings::get_settings,
            commands::settings::update_setting,
            commands::settings::reset_settings_category,
            commands::migration::validate_migration_target,
            commands::migration::start_migration,
            commands::migration::get_migration_status,
            commands::migration::cancel_migration,
            commands::system::get_system_overview,
            commands::system::get_storage_info,
            commands::system::get_processes,
            commands::system::get_system_snapshot,
            commands::system::export_system_report,
            commands::system::get_gpu_info,
            commands::system::get_network_adapters,
            commands::system::get_services,
            commands::system::get_drivers,
            commands::system::get_startup_items,
            commands::system::get_battery_info,
            commands::system::get_security_status,
            commands::system::get_system_health,
            commands::system::kill_process,
            commands::system::start_service,
            commands::system::stop_service,
            commands::system::get_temperature_info,
            commands::system::get_disk_health,
            commands::system::get_gpu_metrics,
            commands::system::refresh_desktop_icons,
            commands::system::clear_icon_cache,
            commands::system::clear_thumb_cache,
            commands::system::clear_temp_files,
            commands::system::flush_dns_cache,
            commands::system::restart_explorer,
            commands::system::run_admin_tool,
            commands::system::get_admin_status,
            commands::global_search::start_global_search,
            commands::global_search::cancel_global_search,
            commands::global_search::open_search_result,
            commands::global_search::reveal_search_result,
            commands::global_search::get_search_index_status,
            commands::global_search::pause_search_index,
            commands::global_search::resume_search_index,
            commands::global_search::rebuild_search_index,
            commands::global_search::get_search_settings,
            commands::global_search::update_search_settings,
            commands::terminal::terminal_spawn,
            commands::terminal::terminal_write,
            commands::terminal::terminal_resize,
            commands::terminal::terminal_close,
            commands::terminal::terminal_list,
            commands::terminal::terminal_list_shells,
            commands::terminal::terminal_history_record,
            commands::terminal::terminal_history_list,
            commands::terminal::terminal_history_clear,
            commands::project_runtime::detect_project_runtime,
            commands::project_runtime::prepare_run_confirmation,
            commands::project_runtime::confirm_run_config,
            commands::project_runtime::start_project_process,
            commands::project_runtime::stop_project_process,
            commands::project_runtime::restart_project_process,
            commands::project_runtime::get_project_run,
            commands::project_runtime::list_project_runs_by_project,
            commands::project_runtime::get_process_logs,
            commands::project_preview::open_project_preview,
            commands::run_center::list_project_runs,
            // Project tasks
            commands::project_tasks::create_project_task,
            commands::project_tasks::list_project_tasks,
            commands::project_tasks::update_project_task,
            commands::project_tasks::delete_project_task,
            commands::project_tasks::reorder_project_tasks,
            commands::project_tasks::link_task_resource,
            commands::project_tasks::unlink_task_resource,
            commands::project_tasks::list_task_links,
            commands::project_tasks::list_links_by_resource
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // 应用退出兜底：终止所有终端会话与项目运行进程，避免孤儿进程。
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<AppState>();
                services::terminal_service::shutdown_all(&state.terminal);
                state.runtime.shutdown_all();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn tmp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nexus-lib-{tag}-{}", db::models::new_id()))
    }

    #[test]
    fn config_data_dir_overrides_default() {
        let tmp = tmp_dir("cfg1");
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let cfg = tmp.join("config.json");
        std::fs::write(&cfg, r#"{"data_dir": "E:\\some\\dir"}"#).expect("write");
        let resolved = resolve_data_dir(Path::new("C:\\default"), &cfg);
        assert_eq!(resolved, PathBuf::from("E:\\some\\dir"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn config_missing_uses_default() {
        let tmp = tmp_dir("cfg2");
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let resolved = resolve_data_dir(Path::new("C:\\default"), &tmp.join("nope.json"));
        assert_eq!(resolved, PathBuf::from("C:\\default"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn config_broken_uses_default() {
        let tmp = tmp_dir("cfg3");
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let cfg = tmp.join("config.json");
        std::fs::write(&cfg, "not json at all").expect("write");
        let resolved = resolve_data_dir(Path::new("C:\\default"), &cfg);
        assert_eq!(resolved, PathBuf::from("C:\\default"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
