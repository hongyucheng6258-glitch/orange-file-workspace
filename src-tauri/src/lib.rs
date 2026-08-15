// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod commands;
mod db;
mod error;
mod events;
mod ipc;
mod services;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::Manager;

use db::connection::open;
use db::migrations::run_migrations;
use ipc::CommandResult;

/// 全局应用状态：数据目录与数据库连接。
pub struct AppState {
    pub data_dir: PathBuf,
    pub conn: Mutex<rusqlite::Connection>,
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
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            std::fs::create_dir_all(data_dir.join("thumbnails"))?;
            let db_path = data_dir.join("workspace.db");
            let conn = open(&db_path)?;
            let mut conn = conn;
            run_migrations(&mut conn)?;
            app.manage(AppState {
                data_dir,
                conn: Mutex::new(conn),
            });
            services::watcher_service::start_managed_watcher(app.handle().clone());
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
            commands::resources::list_trash,
            commands::resources::verify_location,
            commands::import::import_paths,
            commands::import::cancel_task,
            commands::import::list_tasks,
            commands::previews::get_thumbnail,
            commands::previews::get_text_preview,
            commands::previews::hash_resources,
            commands::pages::create_page,
            commands::pages::get_page,
            commands::pages::save_page_blocks,
            commands::pages::list_pages,
            commands::pages::rename_page,
            commands::pages::delete_page,
            commands::projects::import_project,
            commands::projects::list_projects,
            commands::projects::get_project,
            commands::projects::list_project_files,
            commands::editor::open_file,
            commands::editor::save_file,
            commands::editor::save_file_force,
            commands::editor::discard_session,
            commands::resources::toggle_favorite,
            commands::resources::list_favorites,
            commands::resources::delete_permanently,
            commands::search::search_resources,
            commands::backups::create_backup,
            commands::backups::list_backups,
            commands::backups::restore_backup,
            commands::backups::app_environment
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
