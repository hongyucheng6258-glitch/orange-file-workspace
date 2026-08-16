// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod commands;
mod db;
mod error;
mod events;
mod ipc;
mod services;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use db::connection::open;
use db::migrations::run_migrations;
use ipc::CommandResult;

/// 全局应用状态：数据目录、托管目录与数据库连接。
/// data_dir 与 managed_dir 使用内部可变性，支持运行时迁移切换。
pub struct AppState {
    pub data_dir: std::sync::Mutex<PathBuf>,
    pub managed_dir: std::sync::Mutex<PathBuf>,
    pub conn: Mutex<rusqlite::Connection>,
    pub sampler: Mutex<services::system_service::SystemSampler>,
    pub runtime: Arc<services::project_runtime::RuntimeManager>,
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
        .and_then(|buf| {
            serde_json::from_str::<AppConfig>(&String::from_utf8_lossy(&buf)).ok()
        });
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
                let _ = scope.allow_directory(&data_dir, true);
                let _ = scope.allow_directory(&managed_dir, true);
            }
            // 项目运行管理器：确认协议 + 进程生命周期 + 日志。
            let runtime_manager = Arc::new(services::project_runtime::RuntimeManager::new(
                Arc::new(services::process_api::Win32ProcessApiImpl),
                Arc::new(commands::project_runtime::AppRunEventSink::new(
                    app.handle().clone(),
                )),
            ));
            app.manage(AppState {
                data_dir: Mutex::new(data_dir),
                managed_dir: Mutex::new(managed_dir),
                conn: Mutex::new(conn),
                sampler: Mutex::new(services::system_service::SystemSampler::new()),
                runtime: runtime_manager.clone(),
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
            // Keep Tauri's native drop registration intact. The experimental global
            // registration revoked it and regressed both drag-in and window input.
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            app_info,
            commands::resources::list_children,
            commands::resources::get_resource,
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
            commands::system::get_admin_status
            ,
            commands::project_runtime::detect_project_runtime,
            commands::project_runtime::prepare_run_confirmation,
            commands::project_runtime::confirm_run_config,
            commands::project_runtime::start_project_process,
            commands::project_runtime::stop_project_process,
            commands::project_runtime::restart_project_process,
            commands::project_runtime::get_project_run,
            commands::project_runtime::get_process_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
