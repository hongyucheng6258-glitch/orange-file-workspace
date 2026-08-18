use tauri::State;

use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::c_drive_cleaner;
use crate::services::system_service;
use crate::services::system_windows;
use crate::AppState;

/// 系统总览：设备、操作系统、CPU、内存等静态与当前状态。
#[tauri::command]
pub fn get_system_overview(
    state: State<AppState>,
) -> CommandResult<system_service::SystemOverview> {
    let mut sampler = state.sampler.lock().expect("sampler lock poisoned");
    Ok(system_service::collect_overview(&mut sampler))
}

/// 磁盘分区信息。
#[tauri::command]
pub fn get_storage_info(state: State<AppState>) -> CommandResult<Vec<system_service::StorageInfo>> {
    let mut sampler = state.sampler.lock().expect("sampler lock poisoned");
    Ok(system_service::collect_storage(&mut sampler))
}

/// 进程列表，按 CPU 或内存排序。
#[tauri::command]
pub fn get_processes(
    state: State<AppState>,
    sort_by: Option<String>,
    limit: Option<usize>,
) -> CommandResult<Vec<system_service::ProcessInfo>> {
    let mut sampler = state.sampler.lock().expect("sampler lock poisoned");
    Ok(system_service::collect_processes(
        &mut sampler,
        sort_by.as_deref().unwrap_or("cpu"),
        limit.unwrap_or(200),
    ))
}

/// 实时快照：CPU、内存、网络速率。
#[tauri::command]
pub fn get_system_snapshot(
    state: State<AppState>,
) -> CommandResult<system_service::SystemSnapshot> {
    let mut sampler = state.sampler.lock().expect("sampler lock poisoned");
    Ok(system_service::collect_snapshot(&mut sampler))
}

/// 导出系统诊断报告（JSON + HTML）。
#[tauri::command]
pub fn export_system_report(state: State<AppState>) -> CommandResult<system_service::ReportOutput> {
    let mut sampler = state.sampler.lock().expect("sampler lock poisoned");
    let data_dir = state.data_dir.lock().expect("dir lock");
    system_service::export_report(&mut sampler, &data_dir, env!("CARGO_PKG_VERSION"))
        .map_err(|message| AppError::new("export_report_failed", message))
}

/// 显卡信息（DXGI + 驱动注册表）。
#[tauri::command]
pub fn get_gpu_info() -> CommandResult<Vec<system_windows::GpuInfo>> {
    Ok(system_windows::collect_gpu_info())
}

/// 网络适配器信息。
#[tauri::command]
pub fn get_network_adapters() -> CommandResult<Vec<system_windows::NetworkAdapterInfo>> {
    Ok(system_windows::collect_network_adapters())
}

/// Windows 服务列表。
#[tauri::command]
pub fn get_services() -> CommandResult<Vec<system_windows::ServiceInfo>> {
    Ok(system_windows::collect_services())
}

/// 内核与文件系统驱动列表。
#[tauri::command]
pub fn get_drivers() -> CommandResult<Vec<system_windows::DriverInfo>> {
    Ok(system_windows::collect_drivers())
}

/// 启动项列表。
#[tauri::command]
pub fn get_startup_items() -> CommandResult<Vec<system_windows::StartupItemInfo>> {
    Ok(system_windows::collect_startup_items())
}

/// 电池状态。
#[tauri::command]
pub fn get_battery_info() -> CommandResult<system_windows::BatteryInfo> {
    Ok(system_windows::collect_battery())
}

/// 安全状态。
#[tauri::command]
pub fn get_security_status() -> CommandResult<system_windows::SecurityStatus> {
    Ok(system_windows::collect_security_status())
}

/// 综合健康检测。
#[tauri::command]
pub fn get_system_health(state: State<AppState>) -> CommandResult<Vec<system_service::HealthItem>> {
    let mut sampler = state.sampler.lock().expect("sampler lock poisoned");
    Ok(system_service::collect_health(&mut sampler))
}

/// 结束进程。
#[tauri::command]
pub fn kill_process(pid: u32) -> CommandResult<()> {
    system_windows::kill_process(pid).map_err(|m| AppError::new("kill_process_failed", m))
}

/// 启动服务。
#[tauri::command]
pub fn start_service(name: String) -> CommandResult<()> {
    system_windows::start_service(&name).map_err(|m| AppError::new("start_service_failed", m))
}

/// 停止服务。
#[tauri::command]
pub fn stop_service(name: String) -> CommandResult<()> {
    system_windows::stop_service(&name).map_err(|m| AppError::new("stop_service_failed", m))
}

/// 温度传感器。
#[tauri::command]
pub fn get_temperature_info() -> CommandResult<Vec<system_service::TemperatureInfo>> {
    Ok(system_service::collect_temperatures())
}

/// 磁盘 SMART 健康状态。
#[tauri::command]
pub fn get_disk_health() -> CommandResult<Vec<system_windows::DiskHealthInfo>> {
    Ok(system_windows::collect_disk_health())
}

/// GPU 显存指标。
#[tauri::command]
pub fn get_gpu_metrics() -> CommandResult<Vec<system_windows::GpuMetric>> {
    Ok(system_windows::collect_gpu_metrics())
}

/// 刷新桌面图标。
#[tauri::command]
pub fn refresh_desktop_icons() -> CommandResult<()> {
    system_windows::refresh_desktop_icons().map_err(|m| AppError::new("tool_failed", m))
}

/// 清理图标缓存。
#[tauri::command]
pub fn clear_icon_cache() -> CommandResult<system_windows::ToolCleanResult> {
    Ok(system_windows::clear_icon_cache())
}

/// 清理缩略图缓存。
#[tauri::command]
pub fn clear_thumb_cache() -> CommandResult<system_windows::ToolCleanResult> {
    Ok(system_windows::clear_thumb_cache())
}

/// 清理用户临时文件。
#[tauri::command]
pub fn clear_temp_files() -> CommandResult<system_windows::ToolCleanResult> {
    Ok(system_windows::clear_temp_files())
}

/// 刷新 DNS 解析缓存。
#[tauri::command]
pub fn flush_dns_cache() -> CommandResult<()> {
    system_windows::flush_dns_cache().map_err(|m| AppError::new("tool_failed", m))
}

/// 重启资源管理器。
#[tauri::command]
pub fn restart_explorer() -> CommandResult<()> {
    system_windows::restart_explorer().map_err(|m| AppError::new("tool_failed", m))
}

/// 以管理员身份执行高级工具（UAC 提权）。
#[tauri::command]
pub fn run_admin_tool(tool: String) -> CommandResult<()> {
    system_windows::run_admin_tool(&tool).map_err(|m| AppError::new("tool_failed", m))
}

/// 当前进程是否为管理员。
#[tauri::command]
pub fn get_admin_status() -> CommandResult<bool> {
    Ok(system_windows::is_admin())
}

/// 扫描固定白名单内的 C 盘可清理项。
#[tauri::command]
pub fn scan_c_drive_cleanup(
    mode: c_drive_cleaner::CleanupMode,
) -> CommandResult<Vec<c_drive_cleaner::CleanupScanItem>> {
    c_drive_cleaner::scan(mode, system_windows::is_admin())
        .map_err(|message| AppError::new("c_drive_cleanup_scan_failed", message))
}

/// 清理调用方选择的固定项目 ID，不接受任意路径。
#[tauri::command]
pub fn clean_c_drive_items(
    mode: c_drive_cleaner::CleanupMode,
    item_ids: Vec<String>,
    confirm_recycle_bin: bool,
) -> CommandResult<c_drive_cleaner::CleanupRunResult> {
    c_drive_cleaner::clean(
        mode,
        &item_ids,
        confirm_recycle_bin,
        system_windows::is_admin(),
    )
    .map_err(|message| AppError::new("c_drive_cleanup_failed", message))
}
