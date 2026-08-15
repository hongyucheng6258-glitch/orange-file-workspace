// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod error;
mod ipc;

use ipc::CommandResult;

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
        .invoke_handler(tauri::generate_handler![greet, app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
