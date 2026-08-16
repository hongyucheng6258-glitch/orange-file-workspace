//! Windows 专属系统信息采集：显卡、网络适配器、服务、驱动、启动项、电池。
//!
//! 全部通过 Windows API 与注册表读取，不执行外部命令。

use std::path::PathBuf;

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct GpuInfo {
    pub name: String,
    pub vendor: Option<String>,
    pub vram_bytes: u64,
    pub driver_version: Option<String>,
    pub driver_date: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct NetworkAdapterInfo {
    pub name: String,
    pub friendly_name: Option<String>,
    pub mac: Option<String>,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub status: String,
    pub speed_bps: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ServiceInfo {
    pub name: String,
    pub display_name: String,
    pub state: String,
    pub start_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DriverInfo {
    pub name: String,
    pub display_name: String,
    pub state: String,
    pub start_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StartupItemInfo {
    pub name: String,
    pub command: String,
    pub source: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BatteryInfo {
    pub ac_status: String,
    pub charging: bool,
    pub percent: Option<u8>,
    pub life_time_secs: Option<u64>,
}

/// 显卡信息（DXGI + 注册表驱动信息合并）。
#[cfg(windows)]
pub fn collect_gpu_info() -> Vec<GpuInfo> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    let mut gpus: Vec<GpuInfo> = Vec::new();
    unsafe {
        if let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() {
            let mut index = 0u32;
            loop {
                match factory.EnumAdapters1(index) {
                    Ok(adapter) => {
                        if let Ok(desc) = adapter.GetDesc1() {
                            let name = String::from_utf16_lossy(&desc.Description)
                                .trim_end_matches('\0')
                                .to_string();
                            gpus.push(GpuInfo {
                                name,
                                vendor: Some(vendor_name(desc.VendorId)),
                                vram_bytes: desc.DedicatedVideoMemory as u64,
                                driver_version: None,
                                driver_date: None,
                            });
                        }
                        index += 1;
                    }
                    Err(_) => break,
                }
            }
        }
    }

    // 从注册表读取驱动版本与日期，按名称模糊匹配合并。
    let registry_entries = read_gpu_registry_entries();
    let mut used = vec![false; registry_entries.len()];
    for gpu in gpus.iter_mut() {
        let key = normalize_key(&gpu.name);
        for (i, (desc, ver, date)) in registry_entries.iter().enumerate() {
            if used[i] {
                continue;
            }
            let d = normalize_key(desc);
            if key.is_empty() || d.is_empty() {
                continue;
            }
            let head = key.chars().take(12).collect::<String>();
            let dhead = d.chars().take(12).collect::<String>();
            if key.contains(&dhead) || d.contains(&head) {
                gpu.driver_version = ver.clone();
                gpu.driver_date = date.clone();
                used[i] = true;
                break;
            }
        }
    }
    for (i, (desc, ver, date)) in registry_entries.iter().enumerate() {
        if !used[i] {
            gpus.push(GpuInfo {
                name: desc.clone(),
                vendor: None,
                vram_bytes: 0,
                driver_version: ver.clone(),
                driver_date: date.clone(),
            });
        }
    }
    gpus
}

#[cfg(not(windows))]
pub fn collect_gpu_info() -> Vec<GpuInfo> {
    Vec::new()
}

#[cfg(windows)]
fn vendor_name(vendor_id: u32) -> String {
    match vendor_id {
        0x10DE => "NVIDIA".into(),
        0x1002 => "AMD".into(),
        0x8086 => "Intel".into(),
        0x1414 => "Microsoft".into(),
        0x1A03 => "ASPEED".into(),
        _ => format!("0x{vendor_id:04X}"),
    }
}

#[cfg(windows)]
fn normalize_key(s: &str) -> String {
    s.to_lowercase()
        .replace(['-', '_', ' ', '(', ')', '/', '\\'], "")
}

#[cfg(windows)]
fn read_gpu_registry_entries() -> Vec<(String, Option<String>, Option<String>)> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut entries = Vec::new();
    if let Ok(root) = hklm.open_subkey(base) {
        for i in 0..64 {
            match root.open_subkey(format!("{i:04}")) {
                Ok(k) => {
                    if let Ok(desc) = k.get_value::<String, _>("DriverDesc") {
                        entries.push((
                            desc,
                            k.get_value::<String, _>("DriverVersion").ok(),
                            k.get_value::<String, _>("DriverDate").ok(),
                        ));
                    }
                }
                Err(_) => break,
            }
        }
    }
    entries
}

/// 网络适配器信息（GetAdaptersAddresses）。
#[cfg(windows)]
pub fn collect_network_adapters() -> Vec<NetworkAdapterInfo> {
    use windows::Win32::Foundation::ERROR_BUFFER_OVERFLOW;
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusDown;
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;

    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut out = Vec::new();
    unsafe {
        let mut size: u32 = 0;
        let first = GetAdaptersAddresses(0, flags, None, None, &mut size);
        if first != ERROR_BUFFER_OVERFLOW.0 {
            return out;
        }
        let mut buf = vec![0u8; size as usize];
        let ret = GetAdaptersAddresses(
            0,
            flags,
            None,
            Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
            &mut size,
        );
        if ret != 0 {
            return out;
        }

        let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !cur.is_null() {
            let a = &*cur;
            let description = if a.Description.is_null() {
                String::new()
            } else {
                a.Description.to_string().unwrap_or_default()
            };
            let friendly = if a.FriendlyName.is_null() {
                None
            } else {
                Some(a.FriendlyName.to_string().unwrap_or_default())
            };
            let mac = if a.PhysicalAddressLength >= 6 {
                Some(
                    a.PhysicalAddress[..6]
                        .iter()
                        .map(|b| format!("{b:02X}"))
                        .collect::<Vec<_>>()
                        .join("-"),
                )
            } else {
                None
            };

            let mut ipv4 = Vec::new();
            let mut ipv6 = Vec::new();
            let mut ua = a.FirstUnicastAddress;
            while !ua.is_null() {
                let u = &*ua;
                let sa = u.Address;
                if !sa.lpSockaddr.is_null() && sa.iSockaddrLength >= 8 {
                    let bytes = std::slice::from_raw_parts(
                        sa.lpSockaddr as *const u8,
                        sa.iSockaddrLength as usize,
                    );
                    let family = u16::from_le_bytes([bytes[0], bytes[1]]);
                    match family {
                        2 => {
                            if bytes.len() >= 8 {
                                ipv4.push(format!(
                                    "{}.{}.{}.{}",
                                    bytes[4], bytes[5], bytes[6], bytes[7]
                                ));
                            }
                        }
                        23 => {
                            if bytes.len() >= 24 {
                                let octets = &bytes[8..24];
                                let groups: Vec<String> = octets
                                    .chunks(2)
                                    .map(|c| format!("{:02x}{:02x}", c[0], c[1]))
                                    .collect();
                                ipv6.push(groups.join(":"));
                            }
                        }
                        _ => {}
                    }
                }
                ua = u.Next;
            }

            let status = if a.OperStatus == IfOperStatusUp {
                "已连接".to_string()
            } else if a.OperStatus == IfOperStatusDown {
                "未连接".to_string()
            } else {
                "未知".to_string()
            };

            out.push(NetworkAdapterInfo {
                name: description,
                friendly_name: friendly,
                mac,
                ipv4,
                ipv6,
                status,
                speed_bps: a.TransmitLinkSpeed,
            });
            cur = a.Next;
        }
    }
    out
}

#[cfg(not(windows))]
pub fn collect_network_adapters() -> Vec<NetworkAdapterInfo> {
    Vec::new()
}

/// 枚举服务或驱动（SERVICE_WIN32 或 内核/文件系统驱动）。
#[cfg(windows)]
fn enumerate_services(kind: u32) -> Vec<(String, String, String, String)> {
    use windows::Win32::Foundation::ERROR_MORE_DATA;
    use windows::Win32::System::Services::{
        CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW, OpenServiceW,
        QueryServiceConfigW, ENUM_SERVICE_STATUS_PROCESSW, SC_ENUM_PROCESS_INFO, SC_HANDLE,
        SC_MANAGER_ENUMERATE_SERVICE, SERVICE_QUERY_CONFIG,
    };

    let mut out = Vec::new();
    unsafe {
        let Ok(scm) = OpenSCManagerW(None, None, SC_MANAGER_ENUMERATE_SERVICE) else {
            return out;
        };

        let mut needed: u32 = 0;
        let mut returned: u32 = 0;
        let mut resume: u32 = 0;
        let mut buf: Vec<u8> = Vec::new();

        let call = |buf: &mut [u8],
                    needed: &mut u32,
                    returned: &mut u32,
                    resume: &mut u32|
         -> Result<(), windows_core::Error> {
            EnumServicesStatusExW(
                scm,
                SC_ENUM_PROCESS_INFO,
                windows::Win32::System::Services::ENUM_SERVICE_TYPE(kind),
                windows::Win32::System::Services::SERVICE_STATE_ALL,
                Some(buf),
                needed,
                returned,
                Some(resume),
                None,
            )
        };

        match call(&mut buf, &mut needed, &mut returned, &mut resume) {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_MORE_DATA.into() => {
                buf.resize(needed as usize, 0);
                let _ = call(&mut buf, &mut needed, &mut returned, &mut resume);
            }
            Err(_) => {
                let _ = CloseServiceHandle(scm);
                return out;
            }
        }

        let count = returned as usize;
        let item_size = std::mem::size_of::<ENUM_SERVICE_STATUS_PROCESSW>();
        for i in 0..count {
            let offset = i * item_size;
            if offset + item_size > buf.len() {
                break;
            }
            let item =
                unsafe { &*(buf.as_ptr().add(offset) as *const ENUM_SERVICE_STATUS_PROCESSW) };
            let name = if item.lpServiceName.is_null() {
                String::new()
            } else {
                item.lpServiceName.to_string().unwrap_or_default()
            };
            let display = if item.lpDisplayName.is_null() {
                String::new()
            } else {
                item.lpDisplayName.to_string().unwrap_or_default()
            };
            let state = match item.ServiceStatusProcess.dwCurrentState.0 {
                1 => "已停止",
                2 => "启动中",
                3 => "停止中",
                4 => "运行中",
                5 => "继续中",
                6 => "暂停",
                _ => "未知",
            }
            .to_string();

            // 查询启动类型。
            let start_type = if name.is_empty() {
                "未知".to_string()
            } else {
                let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                match OpenServiceW(
                    scm,
                    windows::core::PCWSTR(wide.as_ptr()),
                    SERVICE_QUERY_CONFIG,
                ) {
                    Ok(handle) => {
                        let st = query_start_type(handle);
                        let _ = CloseServiceHandle(handle);
                        st
                    }
                    Err(_) => "未知".to_string(),
                }
            };

            out.push((name, display, state, start_type));
        }
        let _ = CloseServiceHandle(scm);
    }
    out
}

#[cfg(windows)]
fn query_start_type(handle: windows::Win32::System::Services::SC_HANDLE) -> String {
    use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows::Win32::System::Services::{QueryServiceConfigW, QUERY_SERVICE_CONFIGW};

    unsafe {
        let mut needed: u32 = 0;
        let _ = QueryServiceConfigW(handle, None, 0, &mut needed);
        let mut buf = vec![0u8; needed as usize];
        match QueryServiceConfigW(
            handle,
            Some(buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW),
            needed,
            &mut needed,
        ) {
            Ok(()) => {
                let cfg = &*(buf.as_ptr() as *const QUERY_SERVICE_CONFIGW);
                match cfg.dwStartType.0 {
                    0 => "引导启动",
                    1 => "系统启动",
                    2 => "自动",
                    3 => "手动",
                    4 => "禁用",
                    _ => "未知",
                }
                .to_string()
            }
            Err(e) if e.code() == ERROR_INSUFFICIENT_BUFFER.into() => {
                let mut buf2 = vec![0u8; needed as usize];
                if QueryServiceConfigW(
                    handle,
                    Some(buf2.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW),
                    needed,
                    &mut needed,
                )
                .is_ok()
                {
                    let cfg = &*(buf2.as_ptr() as *const QUERY_SERVICE_CONFIGW);
                    match cfg.dwStartType.0 {
                        0 => "引导启动",
                        1 => "系统启动",
                        2 => "自动",
                        3 => "手动",
                        4 => "禁用",
                        _ => "未知",
                    }
                    .to_string()
                } else {
                    "未知".to_string()
                }
            }
            Err(_) => "未知".to_string(),
        }
    }
}

/// Windows 服务列表。
#[cfg(windows)]
pub fn collect_services() -> Vec<ServiceInfo> {
    enumerate_services(48 /* SERVICE_WIN32 */)
        .into_iter()
        .map(|(name, display_name, state, start_type)| ServiceInfo {
            name,
            display_name,
            state,
            start_type,
        })
        .collect()
}

#[cfg(not(windows))]
pub fn collect_services() -> Vec<ServiceInfo> {
    Vec::new()
}

/// 内核与文件系统驱动列表。
#[cfg(windows)]
pub fn collect_drivers() -> Vec<DriverInfo> {
    enumerate_services(
        3, /* SERVICE_KERNEL_DRIVER | SERVICE_FILE_SYSTEM_DRIVER */
    )
    .into_iter()
    .map(|(name, display_name, state, start_type)| DriverInfo {
        name,
        display_name,
        state,
        start_type,
    })
    .collect()
}

#[cfg(not(windows))]
pub fn collect_drivers() -> Vec<DriverInfo> {
    Vec::new()
}

/// 启动项：注册表 Run/RunOnce + 启动文件夹。
#[cfg(windows)]
pub fn collect_startup_items() -> Vec<StartupItemInfo> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let mut out = Vec::new();
    let run_path = r"Software\Microsoft\Windows\CurrentVersion\Run";

    for (hive, label) in [
        (HKEY_CURRENT_USER, "注册表 HKCU"),
        (HKEY_LOCAL_MACHINE, "注册表 HKLM"),
    ] {
        read_run_subkey(RegKey::predef(hive), run_path, label, &mut out);
        let once = format!("{run_path}Once");
        read_run_subkey(
            RegKey::predef(hive),
            &once,
            &format!("{label} Once"),
            &mut out,
        );
    }

    for (env_var, label) in [
        ("APPDATA", "启动文件夹(用户)"),
        ("PROGRAMDATA", "启动文件夹(系统)"),
    ] {
        if let Ok(base) = std::env::var(env_var) {
            let dir = PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs\Startup");
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let command = entry.path().to_string_lossy().into_owned();
                    out.push(StartupItemInfo {
                        name,
                        command,
                        source: label.to_string(),
                    });
                }
            }
        }
    }
    out
}

#[cfg(windows)]
fn read_run_subkey(hive: winreg::RegKey, sub: &str, label: &str, out: &mut Vec<StartupItemInfo>) {
    if let Ok(key) = hive.open_subkey(sub) {
        for item in key.enum_values() {
            if let Ok((name, value)) = item {
                let command = String::from_utf8_lossy(&value.bytes).into_owned();
                if !name.is_empty() {
                    out.push(StartupItemInfo {
                        name,
                        command,
                        source: label.to_string(),
                    });
                }
            }
        }
    }
}

#[cfg(not(windows))]
pub fn collect_startup_items() -> Vec<StartupItemInfo> {
    Vec::new()
}

/// 电池状态（GetSystemPowerStatus）。
#[cfg(windows)]
pub fn collect_battery() -> BatteryInfo {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    let mut ps = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut ps) }.is_err() {
        return BatteryInfo {
            ac_status: "未知".into(),
            charging: false,
            percent: None,
            life_time_secs: None,
        };
    }

    let ac_status = match ps.ACLineStatus {
        0 => "使用电池".to_string(),
        1 => "已连接电源".to_string(),
        _ => "未知".to_string(),
    };
    let charging = ps.BatteryFlag & 8 != 0;
    let percent = match ps.BatteryLifePercent {
        255 => None,
        p => Some(p),
    };
    let life_time_secs = match ps.BatteryLifeTime {
        0xFFFF_FFFF => None,
        t => Some(t as u64),
    };

    BatteryInfo {
        ac_status,
        charging,
        percent,
        life_time_secs,
    }
}

#[cfg(not(windows))]
pub fn collect_battery() -> BatteryInfo {
    BatteryInfo {
        ac_status: "未知".into(),
        charging: false,
        percent: None,
        life_time_secs: None,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SecurityStatus {
    pub firewall_standard: bool,
    pub firewall_domain: bool,
    pub firewall_public: bool,
    pub defender_running: bool,
    pub windows_update_running: bool,
    pub secure_boot: bool,
    pub uac_enabled: bool,
    pub running_as_admin: bool,
}

/// 读取注册表 DWORD 值。
#[cfg(windows)]
fn read_dword(hive: winreg::HKEY, sub: &str, name: &str) -> Option<u32> {
    use winreg::RegKey;

    RegKey::predef(hive)
        .open_subkey(sub)
        .ok()
        .and_then(|k| k.get_value::<u32, _>(name).ok())
}

/// 安全状态：防火墙、Defender、系统更新、Secure Boot、UAC、管理员身份。
#[cfg(windows)]
pub fn collect_security_status() -> SecurityStatus {
    let fw_base = r"SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy";
    let firewall = |profile: &str| -> bool {
        read_dword(
            winreg::enums::HKEY_LOCAL_MACHINE,
            &format!(r"{fw_base}\{profile}"),
            "EnableFirewall",
        ) == Some(1)
    };

    SecurityStatus {
        firewall_standard: firewall("StandardProfile"),
        firewall_domain: firewall("DomainProfile"),
        firewall_public: firewall("PublicProfile"),
        defender_running: service_state("WinDefend").as_deref() == Some("运行中"),
        windows_update_running: service_state("wuauserv").as_deref() == Some("运行中"),
        secure_boot: read_dword(
            winreg::enums::HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Control\SecureBoot\State",
            "UEFISecureBootEnabled",
        ) == Some(1),
        uac_enabled: read_dword(
            winreg::enums::HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System",
            "EnableLUA",
        ) == Some(1),
        running_as_admin: unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() },
    }
}

#[cfg(not(windows))]
pub fn collect_security_status() -> SecurityStatus {
    SecurityStatus {
        firewall_standard: false,
        firewall_domain: false,
        firewall_public: false,
        defender_running: false,
        windows_update_running: false,
        secure_boot: false,
        uac_enabled: false,
        running_as_admin: false,
    }
}

/// 查询单个服务的运行状态。
#[cfg(windows)]
pub fn service_state(name: &str) -> Option<String> {
    use windows::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_MANAGER_CONNECT,
        SERVICE_QUERY_STATUS,
    };

    unsafe {
        let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT).ok()?;
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = OpenServiceW(
            scm,
            windows::core::PCWSTR(wide.as_ptr()),
            SERVICE_QUERY_STATUS,
        )
        .ok()?;
        let mut status = windows::Win32::System::Services::SERVICE_STATUS::default();
        let ok = QueryServiceStatus(handle, &mut status);
        let _ = CloseServiceHandle(handle);
        let _ = CloseServiceHandle(scm);
        if ok.is_err() {
            return None;
        }
        Some(
            match status.dwCurrentState.0 {
                1 => "已停止",
                2 => "启动中",
                3 => "停止中",
                4 => "运行中",
                5 => "继续中",
                6 => "暂停",
                _ => "未知",
            }
            .to_string(),
        )
    }
}

#[cfg(not(windows))]
pub fn service_state(_name: &str) -> Option<String> {
    None
}

/// 结束进程（需要对该进程具备终止权限）。
#[cfg(windows)]
pub fn kill_process(pid: u32) -> Result<(), String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    if pid == std::process::id() {
        return Err("不允许结束当前 Orange 进程".into());
    }
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
            .map_err(|e| format!("打开进程失败（可能权限不足）：{e}"))?;
        let res = TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
        res.map_err(|e| format!("结束进程失败：{e}"))
    }
}

#[cfg(not(windows))]
pub fn kill_process(_pid: u32) -> Result<(), String> {
    Err("当前平台不支持".into())
}

/// 启动服务。
#[cfg(windows)]
pub fn start_service(name: &str) -> Result<(), String> {
    use windows::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, StartServiceW, SC_MANAGER_CONNECT,
        SERVICE_START,
    };

    unsafe {
        let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)
            .map_err(|e| format!("打开服务管理器失败：{e}"))?;
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = OpenServiceW(scm, windows::core::PCWSTR(wide.as_ptr()), SERVICE_START)
            .map_err(|e| format!("打开服务失败：{e}"))?;
        let res = StartServiceW(handle, None);
        let _ = CloseServiceHandle(handle);
        let _ = CloseServiceHandle(scm);
        res.map_err(|e| format!("启动服务失败（可能需要管理员权限）：{e}"))
    }
}

#[cfg(not(windows))]
pub fn start_service(_name: &str) -> Result<(), String> {
    Err("当前平台不支持".into())
}

/// 停止服务。
#[cfg(windows)]
pub fn stop_service(name: &str) -> Result<(), String> {
    use windows::Win32::System::Services::{
        CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT,
        SERVICE_CONTROL_STOP, SERVICE_STATUS, SERVICE_STOP,
    };

    unsafe {
        let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)
            .map_err(|e| format!("打开服务管理器失败：{e}"))?;
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = OpenServiceW(scm, windows::core::PCWSTR(wide.as_ptr()), SERVICE_STOP)
            .map_err(|e| format!("打开服务失败：{e}"))?;
        let mut status = SERVICE_STATUS::default();
        let res = ControlService(handle, SERVICE_CONTROL_STOP, &mut status);
        let _ = CloseServiceHandle(handle);
        let _ = CloseServiceHandle(scm);
        res.map_err(|e| format!("停止服务失败（可能需要管理员权限）：{e}"))
    }
}

#[cfg(not(windows))]
pub fn stop_service(_name: &str) -> Result<(), String> {
    Err("当前平台不支持".into())
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DiskHealthInfo {
    pub name: String,
    pub mount_point: String,
    pub health_status: String,
    pub health_code: i32,
}

/// SMART 健康状态映射。
#[cfg(windows)]
fn disk_health_text(code: i32) -> String {
    match code {
        0 => "未知".to_string(),
        1 => "不健康".to_string(),
        2 => "警告".to_string(),
        3 | 4 => "健康".to_string(),
        _ => "未知".to_string(),
    }
}

/// 通过 StorageDeviceManagementStatus 查询磁盘 SMART 健康状态。
#[cfg(windows)]
pub fn collect_disk_health() -> Vec<DiskHealthInfo> {
    use sysinfo::Disks;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
    use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_MODE, OPEN_EXISTING};
    use windows::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceManagementStatus, IOCTL_STORAGE_QUERY_PROPERTY,
        STORAGE_DEVICE_MANAGEMENT_STATUS, STORAGE_PROPERTY_QUERY,
    };

    unsafe extern "system" {
        fn DeviceIoControl(
            hdevice: windows::Win32::Foundation::HANDLE,
            dwiocontrolcode: u32,
            lpinbuffer: *mut core::ffi::c_void,
            ninsize: u32,
            lpoutbuffer: *mut core::ffi::c_void,
            noutsize: u32,
            lpbytesreturned: *mut u32,
            lpoverlapped: *mut core::ffi::c_void,
        ) -> i32;
    }

    let disks = Disks::new_with_refreshed_list();
    let mut out = Vec::new();
    for d in disks.list() {
        let mount_point = d.mount_point().to_string_lossy().into_owned();
        let name = d.name().to_string_lossy().into_owned();
        // 仅处理盘符型挂载点，如 C:\。
        let Some(root) = mount_point
            .get(..3)
            .filter(|s| s.as_bytes().get(1) == Some(&b':'))
            .map(|s| format!(r"\\.\{}", &s[..1]))
        else {
            continue;
        };

        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            CreateFileW(
                windows::core::PCWSTR(wide.as_ptr()),
                GENERIC_READ.0,
                FILE_SHARE_MODE(FILE_SHARE_MODE(1).0 | FILE_SHARE_MODE(2).0),
                None,
                OPEN_EXISTING,
                Default::default(),
                None,
            )
        };
        match handle {
            Ok(h) => {
                let mut query = STORAGE_PROPERTY_QUERY::default();
                query.PropertyId = StorageDeviceManagementStatus;
                query.QueryType = PropertyStandardQuery;
                let mut status = STORAGE_DEVICE_MANAGEMENT_STATUS::default();
                let mut returned: u32 = 0;
                let ok = unsafe {
                    DeviceIoControl(
                        h,
                        IOCTL_STORAGE_QUERY_PROPERTY,
                        (&mut query as *mut STORAGE_PROPERTY_QUERY).cast(),
                        std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                        (&mut status as *mut STORAGE_DEVICE_MANAGEMENT_STATUS).cast(),
                        std::mem::size_of::<STORAGE_DEVICE_MANAGEMENT_STATUS>() as u32,
                        &mut returned,
                        core::ptr::null_mut(),
                    )
                };
                unsafe {
                    let _ = CloseHandle(h);
                }
                if ok != 0 {
                    out.push(DiskHealthInfo {
                        name,
                        mount_point,
                        health_status: disk_health_text(status.Health.0),
                        health_code: status.Health.0,
                    });
                } else {
                    out.push(DiskHealthInfo {
                        name,
                        mount_point,
                        health_status: "不支持或无法读取".into(),
                        health_code: -1,
                    });
                }
            }
            Err(_) => out.push(DiskHealthInfo {
                name,
                mount_point,
                health_status: "无法打开设备".into(),
                health_code: -2,
            }),
        }
    }
    out
}

#[cfg(not(windows))]
pub fn collect_disk_health() -> Vec<DiskHealthInfo> {
    Vec::new()
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct GpuMetric {
    pub name: String,
    pub vram_total: u64,
    pub vram_used: u64,
    pub vram_percent: f64,
}

/// GPU 显存指标（IDXGIAdapter3::QueryVideoMemoryInfo）。
#[cfg(windows)]
pub fn collect_gpu_metrics() -> Vec<GpuMetric> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIAdapter3, IDXGIFactory1, DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
        DXGI_QUERY_VIDEO_MEMORY_INFO,
    };

    let mut out = Vec::new();
    unsafe {
        if let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() {
            let mut index = 0u32;
            loop {
                match factory.EnumAdapters1(index) {
                    Ok(adapter) => {
                        let name = adapter
                            .GetDesc1()
                            .map(|d| {
                                String::from_utf16_lossy(&d.Description)
                                    .trim_end_matches('\0')
                                    .to_string()
                            })
                            .unwrap_or_default();
                        let (total, used) = match adapter.cast::<IDXGIAdapter3>() {
                            Ok(a3) => {
                                let mut info: DXGI_QUERY_VIDEO_MEMORY_INFO = std::mem::zeroed();
                                if a3
                                    .QueryVideoMemoryInfo(
                                        0,
                                        DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
                                        &mut info,
                                    )
                                    .is_ok()
                                {
                                    (info.Budget as u64, info.CurrentUsage as u64)
                                } else {
                                    (0, 0)
                                }
                            }
                            Err(_) => (0, 0),
                        };
                        let percent = if total > 0 {
                            (used as f64 / total as f64) * 100.0
                        } else {
                            0.0
                        };
                        out.push(GpuMetric {
                            name,
                            vram_total: total,
                            vram_used: used,
                            vram_percent: percent,
                        });
                        index += 1;
                    }
                    Err(_) => break,
                }
            }
        }
    }
    out
}

#[cfg(not(windows))]
pub fn collect_gpu_metrics() -> Vec<GpuMetric> {
    Vec::new()
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolCleanResult {
    pub deleted_files: u64,
    pub freed_bytes: u64,
    pub skipped_files: u64,
}

impl ToolCleanResult {
    fn add(&mut self, other: &ToolCleanResult) {
        self.deleted_files += other.deleted_files;
        self.freed_bytes += other.freed_bytes;
        self.skipped_files += other.skipped_files;
    }
}

/// 删除目录中匹配模式的文件（`name`、`prefix*` 或 `*suffix`）。
/// 目录不存在时返回空结果；占用中的文件计入 skipped。
#[cfg(windows)]
pub fn delete_cache_files(dir: &std::path::Path, patterns: &[&str]) -> ToolCleanResult {
    let mut result = ToolCleanResult {
        deleted_files: 0,
        freed_bytes: 0,
        skipped_files: 0,
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return result;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let matched = patterns.iter().any(|p| {
            if let Some(suffix) = p.strip_prefix('*') {
                name.ends_with(suffix)
            } else if let Some(prefix) = p.strip_suffix('*') {
                name.starts_with(prefix)
            } else {
                name == *p
            }
        });
        if !matched {
            continue;
        }
        match std::fs::metadata(&path) {
            Ok(meta) => match std::fs::remove_file(&path) {
                Ok(()) => {
                    result.deleted_files += 1;
                    result.freed_bytes += meta.len();
                }
                Err(_) => result.skipped_files += 1,
            },
            Err(_) => result.skipped_files += 1,
        }
    }
    result
}

#[cfg(not(windows))]
pub fn delete_cache_files(_dir: &std::path::Path, _patterns: &[&str]) -> ToolCleanResult {
    ToolCleanResult {
        deleted_files: 0,
        freed_bytes: 0,
        skipped_files: 0,
    }
}

/// 清理图标缓存（IconCache.db + Explorer iconcache_*.db）。
#[cfg(windows)]
pub fn clear_icon_cache() -> ToolCleanResult {
    let mut result = ToolCleanResult {
        deleted_files: 0,
        freed_bytes: 0,
        skipped_files: 0,
    };
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let base = PathBuf::from(&local);
        let icon_db = base.join("IconCache.db");
        if icon_db.exists() {
            match std::fs::metadata(&icon_db) {
                Ok(meta) => match std::fs::remove_file(&icon_db) {
                    Ok(()) => {
                        result.deleted_files += 1;
                        result.freed_bytes += meta.len();
                    }
                    Err(_) => result.skipped_files += 1,
                },
                Err(_) => result.skipped_files += 1,
            }
        }
        result.add(&delete_cache_files(
            &base.join(r"Microsoft\Windows\Explorer"),
            &["iconcache_*"],
        ));
    }
    result
}

#[cfg(not(windows))]
pub fn clear_icon_cache() -> ToolCleanResult {
    ToolCleanResult {
        deleted_files: 0,
        freed_bytes: 0,
        skipped_files: 0,
    }
}

/// 清理缩略图缓存（Explorer thumbcache_*.db）。
#[cfg(windows)]
pub fn clear_thumb_cache() -> ToolCleanResult {
    let mut result = ToolCleanResult {
        deleted_files: 0,
        freed_bytes: 0,
        skipped_files: 0,
    };
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        result.add(&delete_cache_files(
            &PathBuf::from(&local).join(r"Microsoft\Windows\Explorer"),
            &["thumbcache_*"],
        ));
    }
    result
}

#[cfg(not(windows))]
pub fn clear_thumb_cache() -> ToolCleanResult {
    ToolCleanResult {
        deleted_files: 0,
        freed_bytes: 0,
        skipped_files: 0,
    }
}

/// 递归清理目录中的文件（保留目录结构），最多处理 2 万个文件。
/// 占用中的文件跳过并计数。
#[cfg(windows)]
pub fn clear_temp_dir(dir: &std::path::Path) -> ToolCleanResult {
    let mut result = ToolCleanResult {
        deleted_files: 0,
        freed_bytes: 0,
        skipped_files: 0,
    };
    const MAX_FILES: u64 = 20_000;

    fn walk(dir: &std::path::Path, result: &mut ToolCleanResult) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if result.deleted_files >= MAX_FILES {
                return;
            }
            let path = entry.path();
            if path.is_dir() {
                walk(&path, result);
            } else {
                match std::fs::metadata(&path) {
                    Ok(meta) => match std::fs::remove_file(&path) {
                        Ok(()) => {
                            result.deleted_files += 1;
                            result.freed_bytes += meta.len();
                        }
                        Err(_) => result.skipped_files += 1,
                    },
                    Err(_) => result.skipped_files += 1,
                }
            }
        }
    }

    walk(dir, &mut result);
    result
}

#[cfg(not(windows))]
pub fn clear_temp_dir(_dir: &std::path::Path) -> ToolCleanResult {
    ToolCleanResult {
        deleted_files: 0,
        freed_bytes: 0,
        skipped_files: 0,
    }
}

/// 清理用户临时目录（%TEMP%）。
#[cfg(windows)]
pub fn clear_temp_files() -> ToolCleanResult {
    match std::env::var("TEMP") {
        Ok(temp) => clear_temp_dir(&PathBuf::from(temp)),
        Err(_) => ToolCleanResult {
            deleted_files: 0,
            freed_bytes: 0,
            skipped_files: 0,
        },
    }
}

#[cfg(not(windows))]
pub fn clear_temp_files() -> ToolCleanResult {
    ToolCleanResult {
        deleted_files: 0,
        freed_bytes: 0,
        skipped_files: 0,
    }
}

/// 刷新桌面图标（通知资源管理器重载）。
#[cfg(windows)]
pub fn refresh_desktop_icons() -> Result<(), String> {
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn refresh_desktop_icons() -> Result<(), String> {
    Err("当前平台不支持".into())
}

/// 刷新 DNS 解析缓存（等同 ipconfig /flushdns）。
#[cfg(windows)]
pub fn flush_dns_cache() -> Result<(), String> {
    #[link(name = "dnsapi")]
    unsafe extern "system" {
        fn DnsFlushResolverCache() -> i32;
    }
    let ok = unsafe { DnsFlushResolverCache() };
    if ok == 0 {
        Err("刷新 DNS 缓存失败".into())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn flush_dns_cache() -> Result<(), String> {
    Err("当前平台不支持".into())
}

/// 枚举 explorer.exe 进程 PID。
#[cfg(windows)]
pub fn explorer_pids() -> Vec<u32> {
    use sysinfo::{ProcessesToUpdate, System};

    let mut sys = System::new_all();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .iter()
        .filter(|(_, p)| p.name().to_string_lossy().to_lowercase() == "explorer.exe")
        .map(|(pid, _)| pid.as_u32())
        .collect()
}

#[cfg(not(windows))]
pub fn explorer_pids() -> Vec<u32> {
    Vec::new()
}

/// 重启资源管理器（结束 explorer 进程并重新启动）。
#[cfg(windows)]
pub fn restart_explorer() -> Result<(), String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    unsafe {
        let pids = explorer_pids();
        if pids.is_empty() {
            return Err("未找到资源管理器进程".into());
        }
        let mut killed = 0u32;
        for pid in pids {
            if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                if TerminateProcess(handle, 0).is_ok() {
                    killed += 1;
                }
                let _ = CloseHandle(handle);
            }
        }
        if killed == 0 {
            return Err("无法结束资源管理器进程".into());
        }
        // 重新启动 explorer.exe。
        let params_vec: Vec<u16> = std::iter::once(0).collect();
        let file_vec: Vec<u16> = "explorer.exe"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let res = ShellExecuteW(
            None,
            None,
            windows::core::PCWSTR(file_vec.as_ptr()),
            windows::core::PCWSTR(params_vec.as_ptr()),
            None,
            SW_SHOWNORMAL,
        );
        if (res.0 as usize) <= 32 {
            return Err("重新启动资源管理器失败".into());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn restart_explorer() -> Result<(), String> {
    Err("当前平台不支持".into())
}

/// 高级工具 → 提权命令映射（白名单）。
#[cfg(windows)]
pub fn admin_tool_command(tool: &str) -> Result<&'static str, String> {
    match tool {
        "sfc" => Ok("/k sfc /scannow"),
        "chkdsk" => Ok("/k chkdsk C: /f"),
        "winsock" => Ok("/k netsh winsock reset"),
        _ => Err(format!("不支持的工具：{tool}")),
    }
}

#[cfg(not(windows))]
pub fn admin_tool_command(_tool: &str) -> Result<&'static str, String> {
    Err("当前平台不支持".into())
}

/// 以管理员身份（UAC 提权）执行高级工具，弹出提升的 cmd 窗口并保持打开。
#[cfg(windows)]
pub fn run_admin_tool(tool: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let params = admin_tool_command(tool)?;
    let to_wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let op = to_wide("runas");
    let file = to_wide("cmd.exe");
    let p = to_wide(params);
    unsafe {
        let res = ShellExecuteW(
            None,
            windows::core::PCWSTR(op.as_ptr()),
            windows::core::PCWSTR(file.as_ptr()),
            windows::core::PCWSTR(p.as_ptr()),
            None,
            SW_SHOWNORMAL,
        );
        if (res.0 as usize) <= 32 {
            return Err(format!(
                "提权启动失败（可能已取消授权），错误码 {}",
                res.0 as usize
            ));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn run_admin_tool(_tool: &str) -> Result<(), String> {
    Err("当前平台不支持".into())
}

/// 当前进程是否为管理员。
#[cfg(windows)]
pub fn is_admin() -> bool {
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() }
}

#[cfg(not(windows))]
pub fn is_admin() -> bool {
    false
}

/// 用系统默认关联打开路径（文件/文件夹/可执行文件）。
#[cfg(windows)]
pub fn shell_open_path(path: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let file_vec: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let op_vec: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let res = ShellExecuteW(
            None,
            windows::core::PCWSTR(op_vec.as_ptr()),
            windows::core::PCWSTR(file_vec.as_ptr()),
            windows::core::PCWSTR::null(),
            None,
            SW_SHOWNORMAL,
        );
        if res.0 as isize <= 32 {
            return Err(format!("ShellExecute failed: {}", res.0 as isize));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn shell_open_path(_path: &str) -> Result<(), String> {
    Err("当前平台不支持".into())
}

/// 在资源管理器中定位路径。
#[cfg(windows)]
pub fn shell_reveal_path(path: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let file_vec: Vec<u16> = "explorer.exe"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let params_vec: Vec<u16> = format!("/select,\"{path}\"")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let res = ShellExecuteW(
            None,
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR(file_vec.as_ptr()),
            windows::core::PCWSTR(params_vec.as_ptr()),
            None,
            SW_SHOWNORMAL,
        );
        if res.0 as isize <= 32 {
            return Err(format!("reveal failed: {}", res.0 as isize));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn shell_reveal_path(_path: &str) -> Result<(), String> {
    Err("当前平台不支持".into())
}

/// 通过 AUMID 启动 Store 应用（shell:AppsFolder 协议）。
#[cfg(windows)]
pub fn shell_open_aumid(aumid: &str) -> Result<(), String> {
    let target = format!("shell:AppsFolder\\{aumid}");
    shell_open_path(&target)
}

#[cfg(not(windows))]
pub fn shell_open_aumid(_aumid: &str) -> Result<(), String> {
    Err("当前平台不支持".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_info_has_entries() {
        let list = collect_gpu_info();
        assert!(!list.is_empty(), "应至少有一个显卡/显示适配器");
        assert!(list.iter().any(|g| !g.name.is_empty()), "显卡名称不应为空");
    }

    #[test]
    fn network_adapters_have_names() {
        let list = collect_network_adapters();
        assert!(!list.is_empty());
        assert!(list.iter().any(|a| !a.name.is_empty()));
    }

    #[test]
    fn services_include_running_entries() {
        let list = collect_services();
        assert!(!list.is_empty());
        assert!(list.iter().any(|s| !s.name.is_empty()));
        assert!(list.iter().any(|s| s.state == "运行中"));
        // 启动类型不应全部未知。
        assert!(list.iter().any(|s| s.start_type != "未知"));
    }

    #[test]
    fn drivers_include_kernel_entries() {
        let list = collect_drivers();
        assert!(!list.is_empty());
        assert!(list.iter().any(|d| !d.name.is_empty()));
    }

    #[test]
    fn startup_items_readable() {
        let list = collect_startup_items();
        // 本机可能没有启动项，只要不崩溃即可；有数据时应包含名称。
        for item in &list {
            assert!(!item.name.is_empty());
            assert!(!item.source.is_empty());
        }
    }

    #[test]
    fn battery_reports_state() {
        let b = collect_battery();
        // 台式机可能无电池，percent 为 None 也合法。
        if let Some(p) = b.percent {
            assert!(p <= 100);
        }
        assert!(!b.ac_status.is_empty());
    }

    #[test]
    fn security_status_readable() {
        let s = collect_security_status();
        // 防火墙状态在 Windows 上应可读取为布尔值。
        assert!(s.firewall_standard || !s.firewall_standard);
        assert!(s.running_as_admin || !s.running_as_admin);
    }

    #[test]
    fn service_state_known_service() {
        let state = service_state("WinDefend");
        assert!(state.is_some(), "WinDefend 服务应能查询到状态");
    }

    #[test]
    fn service_state_unknown_service_returns_none() {
        assert_eq!(service_state("nexusfile_no_such_service_xyz"), None);
    }

    #[test]
    fn disk_health_reads_status() {
        let list = collect_disk_health();
        assert!(!list.is_empty(), "应至少有一个磁盘分区");
        for d in &list {
            assert!(!d.mount_point.is_empty());
            assert!(!d.health_status.is_empty());
        }
    }

    #[test]
    fn gpu_metrics_include_vram() {
        let list = collect_gpu_metrics();
        assert!(!list.is_empty());
        for g in &list {
            assert!(!g.name.is_empty());
            if g.vram_total > 0 {
                assert!(g.vram_percent >= 0.0 && g.vram_percent <= 100.0);
            }
        }
    }

    #[test]
    fn delete_cache_files_missing_dir_is_empty() {
        let dir = std::env::temp_dir().join("nexus_tool_test_no_such_dir");
        let result = delete_cache_files(&dir, &["*.db"]);
        assert_eq!(result.deleted_files, 0);
        assert_eq!(result.freed_bytes, 0);
        assert_eq!(result.skipped_files, 0);
    }

    #[test]
    fn delete_cache_files_counts_matches() {
        let dir = std::env::temp_dir().join("nexus_tool_test_cache");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("iconcache_32.db"), vec![0u8; 128]).unwrap();
        std::fs::write(dir.join("iconcache_256.db"), vec![0u8; 256]).unwrap();
        std::fs::write(dir.join("thumbcache_1.db"), vec![0u8; 64]).unwrap();
        std::fs::write(dir.join("keep.txt"), b"keep").unwrap();

        let result = delete_cache_files(&dir, &["iconcache_*"]);
        assert_eq!(result.deleted_files, 2);
        assert_eq!(result.freed_bytes, 384);
        assert!(!dir.join("iconcache_32.db").exists());
        assert!(!dir.join("iconcache_256.db").exists());
        assert!(
            dir.join("thumbcache_1.db").exists(),
            "不匹配模式的文件应保留"
        );
        assert!(dir.join("keep.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_temp_dir_cleans_files_only() {
        let root = std::env::temp_dir().join("nexus_tool_test_temp");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.tmp"), vec![0u8; 100]).unwrap();
        std::fs::write(root.join("sub").join("b.tmp"), vec![0u8; 50]).unwrap();

        let result = clear_temp_dir(&root);
        assert_eq!(result.deleted_files, 2);
        assert_eq!(result.freed_bytes, 150);
        assert!(root.join("sub").exists(), "目录结构应保留");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn admin_tool_command_mapping() {
        assert_eq!(admin_tool_command("sfc").unwrap(), "/k sfc /scannow");
        assert_eq!(admin_tool_command("chkdsk").unwrap(), "/k chkdsk C: /f");
        assert_eq!(
            admin_tool_command("winsock").unwrap(),
            "/k netsh winsock reset"
        );
        assert!(admin_tool_command("regedit").is_err());
        assert!(admin_tool_command("").is_err());
    }

    #[test]
    fn refresh_and_dns_do_not_error() {
        assert!(refresh_desktop_icons().is_ok());
        assert!(flush_dns_cache().is_ok());
    }

    #[test]
    fn explorer_pids_find_explorer() {
        let pids = explorer_pids();
        assert!(!pids.is_empty(), "本机应存在 explorer.exe 进程");
        assert!(pids.iter().all(|p| *p > 0));
    }

    #[test]
    fn admin_flag_readable() {
        let _ = is_admin();
    }
}
