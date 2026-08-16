//! 系统信息采集服务。
//!
//! 负责采集设备、操作系统、CPU、内存、磁盘、网络和进程信息。
//! 实时指标由前端按需轮询，本服务通过共享采样器计算增量速率。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;
use sysinfo::{Disks, Networks, ProcessStatus, ProcessesToUpdate, System, Users};

/// 网络采样记录，用于计算上下行速率。
pub struct NetSample {
    received: u64,
    transmitted: u64,
    at: Instant,
}

/// 共享采样器：持有同一个 `System` 实例，保证 CPU 使用率与网络速率是相对上次采样的增量。
pub struct SystemSampler {
    pub sys: System,
    pub disks: Disks,
    pub nets: Networks,
    pub users: Users,
    pub prev_net: HashMap<String, NetSample>,
}

impl SystemSampler {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Self {
            sys,
            disks: Disks::new_with_refreshed_list(),
            nets: Networks::new_with_refreshed_list(),
            users: Users::new_with_refreshed_list(),
            prev_net: HashMap::new(),
        }
    }

    /// 刷新 CPU 与内存指标。
    fn refresh_cpu_mem(&mut self) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
    }
}

impl Default for SystemSampler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SystemOverview {
    pub device_name: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub manufacturer: Option<String>,
    pub product_name: Option<String>,
    pub bios_version: Option<String>,
    pub cpu_brand: Option<String>,
    pub cpu_vendor: Option<String>,
    pub cpu_cores: usize,
    pub cpu_threads: usize,
    pub cpu_frequency: u64,
    pub cpu_usage: f32,
    pub mem_total: u64,
    pub mem_used: u64,
    pub mem_percent: f32,
    pub swap_total: u64,
    pub swap_used: u64,
    pub uptime: u64,
    pub boot_time: u64,
    pub process_count: usize,
    pub disk_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StorageInfo {
    pub name: String,
    pub mount_point: String,
    pub kind: String,
    pub file_system: String,
    pub total_space: u64,
    pub available_space: u64,
    pub is_removable: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32,
    pub memory: u64,
    pub path: Option<String>,
    pub status: String,
    pub start_time: u64,
    pub user: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct NetworkRate {
    pub name: String,
    pub down_rate: f64,
    pub up_rate: f64,
    pub total_down: u64,
    pub total_up: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SystemSnapshot {
    pub cpu_usage: f32,
    pub cpu_per_core: Vec<f32>,
    pub mem_total: u64,
    pub mem_used: u64,
    pub mem_percent: f32,
    pub swap_total: u64,
    pub swap_used: u64,
    pub networks: Vec<NetworkRate>,
    pub timestamp: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ReportOutput {
    pub json_path: PathBuf,
    pub html_path: PathBuf,
}

/// 读取 Windows 注册表字符串值（REG_SZ 或 REG_MULTI_SZ）。
#[cfg(windows)]
fn read_reg_value<T: winreg::types::FromRegValue>(sub: &str, name: &str) -> Option<T> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    hklm.open_subkey(sub)
        .ok()
        .and_then(|k| k.get_value(name).ok())
}

#[cfg(windows)]
fn read_firmware_info() -> (Option<String>, Option<String>, Option<String>) {
    let bios = r"HARDWARE\DESCRIPTION\System\BIOS";
    (
        read_reg_value(bios, "SystemManufacturer"),
        read_reg_value(bios, "SystemProductName"),
        read_reg_value::<Vec<String>>(bios, "BIOSVersion").map(|v| v.join("; ")),
    )
}

#[cfg(not(windows))]
fn read_firmware_info() -> (Option<String>, Option<String>, Option<String>) {
    (None, None, None)
}

/// 将进程状态映射为中文描述。
fn status_text(status: ProcessStatus) -> String {
    match status {
        ProcessStatus::Run => "运行中".into(),
        ProcessStatus::Sleep => "睡眠".into(),
        ProcessStatus::Stop => "已停止".into(),
        ProcessStatus::Zombie => "僵尸".into(),
        ProcessStatus::Idle => "空闲".into(),
        ProcessStatus::Dead => "已结束".into(),
        ProcessStatus::Tracing => "调试中".into(),
        ProcessStatus::Wakekill => "唤醒终止".into(),
        ProcessStatus::Waking => "唤醒中".into(),
        ProcessStatus::Parked => "已驻留".into(),
        _ => "其他".into(),
    }
}

/// 采集系统总览（静态信息 + 当前 CPU/内存使用）。
pub fn collect_overview(sampler: &mut SystemSampler) -> SystemOverview {
    sampler.refresh_cpu_mem();
    sampler.disks.refresh(true);
    sampler.sys.refresh_processes(ProcessesToUpdate::All, true);

    let sys = &sampler.sys;
    let (manufacturer, product, bios) = read_firmware_info();
    let cpus = sys.cpus();
    let mem_total = sys.total_memory();
    let mem_used = sys.used_memory();
    let mem_percent = if mem_total > 0 {
        (mem_used as f32 / mem_total as f32) * 100.0
    } else {
        0.0
    };

    SystemOverview {
        device_name: System::host_name(),
        os_name: System::name(),
        os_version: System::os_version(),
        kernel_version: System::kernel_version(),
        manufacturer,
        product_name: product,
        bios_version: bios,
        cpu_brand: cpus.first().map(|c| c.brand().to_string()),
        cpu_vendor: cpus.first().map(|c| c.vendor_id().to_string()),
        cpu_cores: sys.physical_core_count().unwrap_or(0),
        cpu_threads: cpus.len(),
        cpu_frequency: cpus.first().map(|c| c.frequency()).unwrap_or(0),
        cpu_usage: sys.global_cpu_usage(),
        mem_total,
        mem_used,
        mem_percent,
        swap_total: sys.total_swap(),
        swap_used: sys.used_swap(),
        uptime: System::uptime(),
        boot_time: System::boot_time(),
        process_count: sys.processes().len(),
        disk_count: sampler.disks.list().len(),
    }
}

/// 采集磁盘分区信息。
pub fn collect_storage(sampler: &mut SystemSampler) -> Vec<StorageInfo> {
    sampler.disks.refresh(true);
    sampler
        .disks
        .list()
        .iter()
        .map(|d| StorageInfo {
            name: d.name().to_string_lossy().into_owned(),
            mount_point: d.mount_point().to_string_lossy().into_owned(),
            kind: format!("{:?}", d.kind()),
            file_system: d.file_system().to_string_lossy().into_owned(),
            total_space: d.total_space(),
            available_space: d.available_space(),
            is_removable: d.is_removable(),
        })
        .collect()
}

/// 采集进程列表，按 CPU 或内存降序排列。
pub fn collect_processes(
    sampler: &mut SystemSampler,
    sort_by: &str,
    limit: usize,
) -> Vec<ProcessInfo> {
    sampler.sys.refresh_cpu_usage();
    sampler.sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut list: Vec<ProcessInfo> = sampler
        .sys
        .processes()
        .iter()
        .map(|(pid, p)| {
            let user = p.user_id().and_then(|uid| {
                sampler
                    .users
                    .list()
                    .iter()
                    .find(|u| u.id() == uid)
                    .map(|u| u.name().to_string())
            });
            ProcessInfo {
                pid: pid.as_u32(),
                name: p.name().to_string_lossy().into_owned(),
                cpu_usage: p.cpu_usage(),
                memory: p.memory(),
                path: p.exe().map(|e| e.to_string_lossy().into_owned()),
                status: status_text(p.status()),
                start_time: p.start_time(),
                user,
            }
        })
        .collect();

    if sort_by == "memory" {
        list.sort_by_key(|p| std::cmp::Reverse(p.memory));
    } else {
        list.sort_by(|a, b| b.cpu_usage.total_cmp(&a.cpu_usage));
    }
    list.truncate(limit);
    list
}

/// 采集实时快照：CPU、内存、网络速率。
pub fn collect_snapshot(sampler: &mut SystemSampler) -> SystemSnapshot {
    sampler.refresh_cpu_mem();
    sampler.nets.refresh(true);

    let now = Instant::now();
    let mut rates: Vec<NetworkRate> = Vec::new();

    for (name, data) in sampler.nets.list() {
        let received = data.received();
        let transmitted = data.transmitted();
        let key = name.clone();
        let (down, up) = match sampler.prev_net.get(&key) {
            Some(prev) => {
                let dt = now.duration_since(prev.at).as_secs_f64();
                if dt > 0.0 {
                    (
                        received.saturating_sub(prev.received) as f64 / dt,
                        transmitted.saturating_sub(prev.transmitted) as f64 / dt,
                    )
                } else {
                    (0.0, 0.0)
                }
            }
            None => (0.0, 0.0),
        };
        rates.push(NetworkRate {
            name: name.to_string(),
            down_rate: down,
            up_rate: up,
            total_down: received,
            total_up: transmitted,
        });
        sampler.prev_net.insert(
            key,
            NetSample {
                received,
                transmitted,
                at: now,
            },
        );
    }
    rates.sort_by(|a, b| b.down_rate.total_cmp(&a.down_rate));

    let mem_total = sampler.sys.total_memory();
    let mem_used = sampler.sys.used_memory();
    let mem_percent = if mem_total > 0 {
        (mem_used as f32 / mem_total as f32) * 100.0
    } else {
        0.0
    };

    SystemSnapshot {
        cpu_usage: sampler.sys.global_cpu_usage(),
        cpu_per_core: sampler.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
        mem_total,
        mem_used,
        mem_percent,
        swap_total: sampler.sys.total_swap(),
        swap_used: sampler.sys.used_swap(),
        networks: rates,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    }
}

/// 将 Unix 秒时间戳格式化为本地时间字符串（YYYY-MM-DD HH:MM:SS）。
pub fn format_local_time(epoch_secs: u64) -> String {
    let secs = epoch_secs as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // Howard Hinnant civil_from_days 算法，仅依赖整数运算。
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn report_table(title: &str, rows: Vec<(String, String)>) -> String {
    let mut out = format!("<section><h3>{}</h3><table><tbody>", html_escape(title));
    for (k, v) in rows {
        out.push_str(&format!(
            "<tr><th>{}</th><td>{}</td></tr>",
            html_escape(&k),
            html_escape(&v)
        ));
    }
    out.push_str("</tbody></table></section>");
    out
}

/// 生成诊断报告（JSON + HTML），保存到数据目录下的 system-reports/。
pub fn export_report(
    sampler: &mut SystemSampler,
    data_dir: &Path,
    app_version: &str,
) -> Result<ReportOutput, String> {
    let overview = collect_overview(sampler);
    let storage = collect_storage(sampler);
    let processes = collect_processes(sampler, "cpu", 50);
    let snapshot = collect_snapshot(sampler);

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stamp = format_local_time(now_secs).replace(['-', ':', ' '], "");

    let dir = data_dir.join("system-reports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建报告目录失败：{e}"))?;
    let json_path = dir.join(format!("system-report-{stamp}.json"));
    let html_path = dir.join(format!("system-report-{stamp}.html"));

    let payload = serde_json::json!({
        "generated_at": now_secs,
        "generated_at_text": format_local_time(now_secs),
        "app_version": app_version,
        "overview": overview,
        "storage": storage,
        "snapshot": snapshot,
        "top_processes": processes,
    });
    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入 JSON 报告失败：{e}"))?;

    let mut html = String::new();
    html.push_str("<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\">");
    html.push_str("<title>Orange 系统诊断报告</title><style>");
    html.push_str(
        "body{font-family:system-ui,'Segoe UI',sans-serif;margin:24px;color:#1a1a1a;background:#fff}h1{font-size:20px}section{margin:20px 0}h3{margin:0 0 8px;font-size:14px;color:#4b3fe3}table{border-collapse:collapse;width:100%}th,td{border:1px solid #ddd;padding:6px 10px;text-align:left;font-size:13px}th{width:220px;background:#f6f6f8}",
    );
    html.push_str("</style></head><body>");
    html.push_str(&format!(
        "<h1>Orange 系统诊断报告</h1><p>生成时间：{}　应用版本：{}</p>",
        html_escape(&format_local_time(now_secs)),
        html_escape(app_version)
    ));

    html.push_str(&report_table(
        "设备与系统",
        vec![
            (
                "设备名称".into(),
                overview.device_name.unwrap_or_else(|| "未知".into()),
            ),
            (
                "操作系统".into(),
                overview.os_name.unwrap_or_else(|| "未知".into()),
            ),
            (
                "系统版本".into(),
                overview.os_version.unwrap_or_else(|| "未知".into()),
            ),
            (
                "内核版本".into(),
                overview.kernel_version.unwrap_or_else(|| "未知".into()),
            ),
            (
                "制造商".into(),
                overview.manufacturer.unwrap_or_else(|| "未知".into()),
            ),
            (
                "产品型号".into(),
                overview.product_name.unwrap_or_else(|| "未知".into()),
            ),
            (
                "BIOS 版本".into(),
                overview.bios_version.unwrap_or_else(|| "未知".into()),
            ),
            ("运行时间".into(), format!("{} 秒", overview.uptime)),
        ],
    ));

    html.push_str(&report_table(
        "处理器与内存",
        vec![
            (
                "CPU".into(),
                overview.cpu_brand.unwrap_or_else(|| "未知".into()),
            ),
            ("物理核心".into(), overview.cpu_cores.to_string()),
            ("逻辑线程".into(), overview.cpu_threads.to_string()),
            ("CPU 使用率".into(), format!("{:.1}%", overview.cpu_usage)),
            ("内存总量".into(), format_bytes(overview.mem_total)),
            ("内存已用".into(), format_bytes(overview.mem_used)),
            ("内存使用率".into(), format!("{:.1}%", overview.mem_percent)),
            ("交换空间总量".into(), format_bytes(overview.swap_total)),
            ("交换空间已用".into(), format_bytes(overview.swap_used)),
        ],
    ));

    if !storage.is_empty() {
        let mut rows = Vec::new();
        for d in &storage {
            rows.push((
                d.name.clone(),
                format!(
                    "{}（{}，共 {}，可用 {}）",
                    d.mount_point,
                    d.file_system,
                    format_bytes(d.total_space),
                    format_bytes(d.available_space)
                ),
            ));
        }
        html.push_str(&report_table("磁盘存储", rows));
    }

    if !processes.is_empty() {
        let mut rows = Vec::new();
        for p in &processes {
            rows.push((
                format!("PID {}", p.pid),
                format!(
                    "{}（CPU {:.1}%，内存 {}，{}）",
                    p.name,
                    p.cpu_usage,
                    format_bytes(p.memory),
                    p.status
                ),
            ));
        }
        html.push_str(&report_table("高占用进程（前 50）", rows));
    }

    html.push_str("</body></html>");
    std::fs::write(&html_path, html).map_err(|e| format!("写入 HTML 报告失败：{e}"))?;

    Ok(ReportOutput {
        json_path,
        html_path,
    })
}

/// 字节数格式化。
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let units = ["KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes as f64;
    let mut unit = "B";
    for u in units {
        value /= 1024.0;
        unit = u;
        if value < 1024.0 {
            break;
        }
    }
    if value >= 100.0 {
        format!("{value:.0} {unit}")
    } else {
        format!("{value:.1} {unit}")
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct HealthItem {
    pub level: String,
    pub title: String,
    pub detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TemperatureInfo {
    pub label: String,
    pub temperature_c: Option<f32>,
    pub max_c: Option<f32>,
}

/// 温度传感器（Windows 下通常来自 ACPI，笔记本常见，台式机可能为空）。
pub fn collect_temperatures() -> Vec<TemperatureInfo> {
    let mut comps = sysinfo::Components::new_with_refreshed_list();
    comps.refresh(true);
    comps
        .list()
        .iter()
        .map(|c| TemperatureInfo {
            label: c.label().to_string(),
            temperature_c: c.temperature(),
            max_c: c.max(),
        })
        .collect()
}

/// 综合健康检测：CPU、内存、交换空间、磁盘、进程数与安全状态。
pub fn collect_health(sampler: &mut SystemSampler) -> Vec<HealthItem> {
    let mut items = Vec::new();

    sampler.refresh_cpu_mem();
    sampler.disks.refresh(true);
    sampler.sys.refresh_processes(ProcessesToUpdate::All, true);

    let cpu = sampler.sys.global_cpu_usage();
    let mem_total = sampler.sys.total_memory();
    let mem_used = sampler.sys.used_memory();
    let mem_percent = if mem_total > 0 {
        (mem_used as f32 / mem_total as f32) * 100.0
    } else {
        0.0
    };
    let swap_total = sampler.sys.total_swap();
    let swap_used = sampler.sys.used_swap();
    let swap_percent = if swap_total > 0 {
        (swap_used as f32 / swap_total as f32) * 100.0
    } else {
        0.0
    };

    items.push(if cpu > 95.0 {
        HealthItem {
            level: "danger".into(),
            title: "CPU 占用过高".into(),
            detail: format!("当前 CPU 使用率 {cpu:.1}%，可能影响系统响应。"),
        }
    } else if cpu > 85.0 {
        HealthItem {
            level: "warning".into(),
            title: "CPU 占用偏高".into(),
            detail: format!("当前 CPU 使用率 {cpu:.1}%。"),
        }
    } else {
        HealthItem {
            level: "ok".into(),
            title: "CPU 负载正常".into(),
            detail: format!("当前 CPU 使用率 {cpu:.1}%。"),
        }
    });

    items.push(if mem_percent > 90.0 {
        HealthItem {
            level: "danger".into(),
            title: "内存不足".into(),
            detail: format!(
                "内存使用率 {mem_percent:.1}%（{}/{}）。",
                format_bytes(mem_used),
                format_bytes(mem_total)
            ),
        }
    } else if mem_percent > 80.0 {
        HealthItem {
            level: "warning".into(),
            title: "内存占用偏高".into(),
            detail: format!(
                "内存使用率 {mem_percent:.1}%（{}/{}）。",
                format_bytes(mem_used),
                format_bytes(mem_total)
            ),
        }
    } else {
        HealthItem {
            level: "ok".into(),
            title: "内存充足".into(),
            detail: format!(
                "内存使用率 {mem_percent:.1}%（{}/{}）。",
                format_bytes(mem_used),
                format_bytes(mem_total)
            ),
        }
    });

    if swap_total > 0 {
        items.push(if swap_percent > 80.0 {
            HealthItem {
                level: "warning".into(),
                title: "交换空间使用过高".into(),
                detail: format!(
                    "交换空间使用率 {swap_percent:.1}%（{}/{}）。",
                    format_bytes(swap_used),
                    format_bytes(swap_total)
                ),
            }
        } else {
            HealthItem {
                level: "ok".into(),
                title: "交换空间正常".into(),
                detail: format!(
                    "交换空间使用率 {swap_percent:.1}%（{}/{}）。",
                    format_bytes(swap_used),
                    format_bytes(swap_total)
                ),
            }
        });
    }

    let low_disks: Vec<String> = sampler
        .disks
        .list()
        .iter()
        .filter(|d| {
            d.total_space() > 0 && (d.available_space() as f64 / d.total_space() as f64) < 0.10
        })
        .map(|d| {
            format!(
                "{}（{}）",
                d.name().to_string_lossy(),
                d.mount_point().to_string_lossy()
            )
        })
        .collect();
    items.push(if low_disks.is_empty() {
        HealthItem {
            level: "ok".into(),
            title: "磁盘空间充足".into(),
            detail: "所有分区可用空间均高于 10%。".into(),
        }
    } else {
        HealthItem {
            level: "warning".into(),
            title: "磁盘空间不足".into(),
            detail: format!("以下分区可用空间低于 10%：{}。", low_disks.join("、")),
        }
    });

    let process_count = sampler.sys.processes().len();
    items.push(if process_count > 1000 {
        HealthItem {
            level: "warning".into(),
            title: "进程数量异常".into(),
            detail: format!("当前运行 {process_count} 个进程，数量偏高。"),
        }
    } else {
        HealthItem {
            level: "ok".into(),
            title: "进程数量正常".into(),
            detail: format!("当前运行 {process_count} 个进程。"),
        }
    });

    // 安全状态联动检测。
    let sec = crate::services::system_windows::collect_security_status();
    let firewall_on = sec.firewall_standard && sec.firewall_public;
    let defender_on = sec.defender_running;
    items.push(if !firewall_on && !defender_on {
        HealthItem {
            level: "danger".into(),
            title: "防火墙与安全软件均已关闭".into(),
            detail: "Windows 防火墙与 Defender 都未启用，系统暴露风险较高。".into(),
        }
    } else if !firewall_on || !defender_on {
        HealthItem {
            level: "warning".into(),
            title: "部分安全防护未开启".into(),
            detail: format!(
                "防火墙：{}；Defender：{}。",
                if firewall_on {
                    "已开启"
                } else {
                    "已关闭"
                },
                if defender_on {
                    "运行中"
                } else {
                    "未运行"
                }
            ),
        }
    } else {
        HealthItem {
            level: "ok".into(),
            title: "基础安全防护正常".into(),
            detail: "Windows 防火墙与 Defender 均已启用。".into(),
        }
    });

    let battery = crate::services::system_windows::collect_battery();
    if let Some(percent) = battery.percent {
        if percent <= 20 && battery.ac_status == "使用电池" {
            items.push(HealthItem {
                level: "warning".into(),
                title: "电池电量偏低".into(),
                detail: format!("当前电量 {percent}%，建议尽快连接电源。"),
            });
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_overview_returns_device_info() {
        let mut s = SystemSampler::new();
        let o = collect_overview(&mut s);
        assert!(o.device_name.is_some() && !o.device_name.as_deref().unwrap_or("").is_empty());
        assert!(o.mem_total > 0);
        assert!(o.cpu_threads > 0);
        assert!(o.cpu_cores > 0);
    }

    #[test]
    fn collect_storage_lists_drives() {
        let mut s = SystemSampler::new();
        let list = collect_storage(&mut s);
        assert!(!list.is_empty());
        assert!(list.iter().any(|d| d.total_space > 0));
    }

    #[test]
    fn collect_processes_sorted_by_cpu() {
        let mut s = SystemSampler::new();
        let list = collect_processes(&mut s, "cpu", 20);
        assert!(!list.is_empty());
        assert!(list.windows(2).all(|w| w[0].cpu_usage >= w[1].cpu_usage));
        let by_mem = collect_processes(&mut s, "memory", 10);
        assert!(by_mem.windows(2).all(|w| w[0].memory >= w[1].memory));
    }

    #[test]
    fn collect_snapshot_has_memory_and_cpu() {
        let mut s = SystemSampler::new();
        let snap = collect_snapshot(&mut s);
        assert!(snap.mem_total > 0);
        assert!(!snap.cpu_per_core.is_empty());
        assert!(snap.timestamp > 0);
    }

    #[test]
    fn export_report_writes_both_files() {
        let mut s = SystemSampler::new();
        let dir = std::env::temp_dir().join("nexus-system-test");
        let _ = std::fs::remove_dir_all(&dir);
        let out = export_report(&mut s, &dir, "0.1.0-test").expect("export should succeed");
        assert!(out.html_path.exists(), "HTML 报告应存在");
        assert!(out.json_path.exists(), "JSON 报告应存在");
        let html = std::fs::read_to_string(&out.html_path).unwrap();
        assert!(html.contains("系统诊断报告"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn format_local_time_shape() {
        let t = format_local_time(1_700_000_000);
        assert_eq!(t.len(), 19);
        assert!(t.as_bytes()[4] == b'-' && t.as_bytes()[7] == b'-');
    }

    #[test]
    fn format_bytes_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2 * 1024), "2.0 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn health_check_produces_complete_items() {
        let mut s = SystemSampler::new();
        let items = collect_health(&mut s);
        assert!(
            items.len() >= 6,
            "健康检测应至少包含 6 项，实际 {}",
            items.len()
        );
        for item in &items {
            assert!(!item.title.is_empty());
            assert!(!item.detail.is_empty());
            assert!(["ok", "warning", "danger"].contains(&item.level.as_str()));
        }
        assert!(items.iter().any(|i| i.level == "ok"), "至少有一项应为正常");
    }

    #[test]
    fn temperature_collector_does_not_panic() {
        let list = collect_temperatures();
        for t in &list {
            assert!(!t.label.is_empty());
            if let Some(c) = t.temperature_c {
                assert!(c.is_finite());
            }
        }
    }
}
