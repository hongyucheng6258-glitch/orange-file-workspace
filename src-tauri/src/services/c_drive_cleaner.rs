use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_CANDIDATE_FILES: u64 = 100_000;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupMode {
    Safe,
    Deep,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupRisk {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Serialize)]
pub struct CleanupCatalogItem {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub risk: CleanupRisk,
    pub default_selected: bool,
    pub requires_admin: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CleanupScanItem {
    #[serde(flatten)]
    pub item: CleanupCatalogItem,
    pub files: u64,
    pub bytes: u64,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CleanupStats {
    pub files: u64,
    pub bytes: u64,
    pub limit_reached: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CleanupRunStats {
    pub deleted_files: u64,
    pub freed_bytes: u64,
    pub skipped_files: u64,
    pub limit_reached: bool,
    #[serde(skip)]
    candidate_files: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CleanupRunItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub deleted_files: u64,
    pub freed_bytes: u64,
    pub skipped_files: u64,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CleanupRunResult {
    pub status: String,
    pub deleted_files: u64,
    pub freed_bytes: u64,
    pub skipped_files: u64,
    pub items: Vec<CleanupRunItem>,
}

pub fn cleanup_catalog(mode: CleanupMode) -> Vec<CleanupCatalogItem> {
    let mut items = vec![
        catalog(
            "user_temp",
            "用户临时文件",
            "当前用户临时目录中的文件",
            CleanupRisk::Low,
            true,
            false,
        ),
        catalog(
            "windows_temp",
            "Windows 临时文件",
            "Windows 临时目录中可删除的文件",
            CleanupRisk::Medium,
            false,
            true,
        ),
        catalog(
            "icon_cache",
            "图标缓存",
            "资源管理器生成的图标缓存",
            CleanupRisk::Low,
            true,
            false,
        ),
        catalog(
            "thumbnail_cache",
            "缩略图缓存",
            "资源管理器生成的缩略图缓存",
            CleanupRisk::Low,
            true,
            false,
        ),
        catalog(
            "edge_cache",
            "Edge 普通缓存",
            "Edge 页面资源缓存，不含登录和浏览数据",
            CleanupRisk::Low,
            true,
            false,
        ),
        catalog(
            "chrome_cache",
            "Chrome 普通缓存",
            "Chrome 页面资源缓存，不含登录和浏览数据",
            CleanupRisk::Low,
            true,
            false,
        ),
        catalog(
            "firefox_cache",
            "Firefox 普通缓存",
            "Firefox 页面资源缓存，不含登录和浏览数据",
            CleanupRisk::Low,
            true,
            false,
        ),
        catalog(
            "recycle_bin",
            "C 盘回收站",
            "永久清空 C 盘回收站",
            CleanupRisk::Medium,
            true,
            false,
        ),
    ];
    if mode == CleanupMode::Deep {
        items.extend([
            catalog(
                "windows_error_reports",
                "Windows 错误报告",
                "系统与用户错误报告归档",
                CleanupRisk::Medium,
                false,
                true,
            ),
            catalog(
                "crash_dumps",
                "崩溃转储",
                "应用与系统崩溃转储文件",
                CleanupRisk::Medium,
                false,
                true,
            ),
            catalog(
                "windows_update_cache",
                "Windows Update 下载缓存",
                "已下载的系统更新缓存",
                CleanupRisk::High,
                false,
                true,
            ),
            catalog(
                "delivery_optimization_cache",
                "传递优化缓存",
                "Windows 更新传递优化缓存",
                CleanupRisk::High,
                false,
                true,
            ),
        ]);
    }
    items
}

fn catalog(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    risk: CleanupRisk,
    default_selected: bool,
    requires_admin: bool,
) -> CleanupCatalogItem {
    CleanupCatalogItem {
        id,
        name,
        description,
        risk,
        default_selected,
        requires_admin,
    }
}

pub fn validate_item_ids(mode: CleanupMode, item_ids: &[String]) -> Result<(), String> {
    let allowed: HashSet<&str> = cleanup_catalog(mode)
        .into_iter()
        .map(|item| item.id)
        .collect();
    for id in item_ids {
        if !allowed.contains(id.as_str()) {
            return Err(format!("未知清理项：{id}"));
        }
    }
    Ok(())
}

pub fn validate_clean_request(
    mode: CleanupMode,
    item_ids: &[String],
    confirm_recycle_bin: bool,
    _is_admin: bool,
) -> Result<(), String> {
    validate_item_ids(mode, item_ids)?;
    if item_ids.iter().any(|id| id == "recycle_bin") && !confirm_recycle_bin {
        return Err("清空回收站需要独立确认".to_string());
    }
    Ok(())
}

pub fn scan(mode: CleanupMode, is_admin: bool) -> Result<Vec<CleanupScanItem>, String> {
    cleanup_catalog(mode)
        .into_iter()
        .map(|item| {
            if item.requires_admin && !is_admin {
                return Ok(CleanupScanItem {
                    item,
                    files: 0,
                    bytes: 0,
                    status: "requires_admin".into(),
                    message: Some("需要管理员权限".into()),
                });
            }
            let stats = if item.id == "recycle_bin" {
                query_recycle_bin()?
            } else {
                scan_roots(&cleanup_roots(item.id)?)
            };
            Ok(CleanupScanItem {
                item,
                files: stats.files,
                bytes: stats.bytes,
                status: if stats.limit_reached {
                    "partial"
                } else {
                    "ready"
                }
                .into(),
                message: stats.limit_reached.then(|| {
                    format!("已达到候选文件上限 {MAX_CANDIDATE_FILES}，扫描结果为部分状态")
                }),
            })
        })
        .collect()
}

pub fn clean(
    mode: CleanupMode,
    item_ids: &[String],
    confirm_recycle_bin: bool,
    is_admin: bool,
) -> Result<CleanupRunResult, String> {
    validate_clean_request(mode, item_ids, confirm_recycle_bin, is_admin)?;
    let catalog = cleanup_catalog(mode);
    let mut result = CleanupRunResult::default();

    for id in item_ids {
        let item = catalog
            .iter()
            .find(|item| item.id == id)
            .expect("validated item id");
        let run = if item.requires_admin && !is_admin {
            CleanupRunItem {
                id: id.clone(),
                name: item.name.into(),
                status: "requires_admin".into(),
                deleted_files: 0,
                freed_bytes: 0,
                skipped_files: 0,
                message: Some("需要管理员权限，请以管理员身份运行应用".into()),
            }
        } else {
            let cleaned = if item.id == "recycle_bin" {
                empty_recycle_bin()
            } else {
                cleanup_roots(item.id).map(|roots| clean_roots(&roots))
            };
            match cleaned {
                Ok(stats) => CleanupRunItem {
                    id: id.clone(),
                    name: item.name.into(),
                    status: if stats.limit_reached {
                        "partial"
                    } else {
                        "completed"
                    }
                    .into(),
                    deleted_files: stats.deleted_files,
                    freed_bytes: stats.freed_bytes,
                    skipped_files: stats.skipped_files,
                    message: stats.limit_reached.then(|| {
                        format!("已达到候选文件上限 {MAX_CANDIDATE_FILES}，仅完成部分清理")
                    }),
                },
                Err(message) => CleanupRunItem {
                    id: id.clone(),
                    name: item.name.into(),
                    status: "failed".into(),
                    deleted_files: 0,
                    freed_bytes: 0,
                    skipped_files: 0,
                    message: Some(message),
                },
            }
        };
        result.deleted_files += run.deleted_files;
        result.freed_bytes += run.freed_bytes;
        result.skipped_files += run.skipped_files;
        result.items.push(run);
    }
    result.status = if result.items.iter().any(|item| item.status == "partial") {
        "partial"
    } else if result
        .items
        .iter()
        .any(|item| item.status == "failed" || item.status == "requires_admin")
    {
        "completed_with_errors"
    } else {
        "completed"
    }
    .into();
    Ok(result)
}

fn cleanup_roots(id: &str) -> Result<Vec<PathBuf>, String> {
    let local = env_path("LOCALAPPDATA");
    let roaming = env_path("APPDATA");
    let user = env_path("USERPROFILE");
    let temp = env_path("TEMP");
    let windows = env_path("WINDIR").unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let program_data = env_path("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let allowed_roots: Vec<PathBuf> = [
        local.clone(),
        roaming.clone(),
        user.clone(),
        temp.clone(),
        Some(windows.clone()),
        Some(program_data.clone()),
    ]
    .into_iter()
    .flatten()
    .collect();

    let roots = match id {
        "user_temp" => temp.into_iter().collect(),
        "windows_temp" => vec![windows.join("Temp")],
        "icon_cache" => local
            .into_iter()
            .flat_map(|p| {
                let mut files = vec![p.join("IconCache.db")];
                files.extend(cache_database_files(
                    &p.join(r"Microsoft\Windows\Explorer"),
                    "iconcache_",
                ));
                files
            })
            .collect(),
        "thumbnail_cache" => local
            .into_iter()
            .flat_map(|p| {
                cache_database_files(&p.join(r"Microsoft\Windows\Explorer"), "thumbcache_")
            })
            .collect(),
        "edge_cache" => chromium_cache_roots(local.as_deref(), r"Microsoft\Edge\User Data"),
        "chrome_cache" => chromium_cache_roots(local.as_deref(), r"Google\Chrome\User Data"),
        "firefox_cache" => firefox_cache_roots(local.as_deref()),
        "windows_error_reports" => {
            let mut values = vec![program_data.join(r"Microsoft\Windows\WER")];
            if let Some(local) = local {
                values.push(local.join(r"Microsoft\Windows\WER"));
            }
            values
        }
        "crash_dumps" => {
            let mut values = vec![windows.join("Minidump"), windows.join("MEMORY.DMP")];
            if let Some(local) = local {
                values.push(local.join("CrashDumps"));
            }
            values
        }
        "windows_update_cache" => vec![windows.join(r"SoftwareDistribution\Download")],
        "delivery_optimization_cache" => vec![
            windows.join(r"SoftwareDistribution\DeliveryOptimization"),
            program_data.join(r"Microsoft\Windows\DeliveryOptimization\Cache"),
        ],
        "recycle_bin" => Vec::new(),
        _ => return Err(format!("未知清理项：{id}")),
    };

    let protected: Vec<PathBuf> = [Some(PathBuf::from(r"C:\")), Some(windows), user, roaming]
        .into_iter()
        .flatten()
        .collect();
    let protected_refs: Vec<&Path> = protected.iter().map(PathBuf::as_path).collect();
    roots
        .into_iter()
        .filter(|path| path.exists() && is_c_drive_path(path))
        .map(|path| {
            is_safe_cleanup_path(&path, &protected_refs)?;
            let allowed_root = allowed_roots
                .iter()
                .filter(|root| path.starts_with(root))
                .max_by_key(|root| root.components().count())
                .ok_or_else(|| format!("清理根目录不在允许范围内：{}", path.display()))?;
            validate_final_cleanup_root(&path, allowed_root)?;
            Ok(path)
        })
        .collect()
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| validate_environment_root(path).is_ok())
}

fn validate_environment_root(path: &Path) -> Result<(), String> {
    let value = path.to_string_lossy();
    let is_absolute = path.is_absolute() || value.as_bytes().get(1) == Some(&b':');
    let has_dot_component = value
        .replace('/', "\\")
        .split('\\')
        .any(|component| component == "." || component == "..");
    if !is_absolute || has_dot_component {
        return Err(format!("拒绝不安全环境变量路径：{}", path.display()));
    }
    Ok(())
}

fn validate_final_cleanup_root(path: &Path, allowed_root: &Path) -> Result<(), String> {
    let allowed_metadata =
        fs::symlink_metadata(allowed_root).map_err(|e| format!("无法读取允许根目录：{e}"))?;
    if is_reparse_point(&allowed_metadata) {
        return Err(format!("拒绝重解析允许根目录：{}", allowed_root.display()));
    }
    let canonical_root =
        fs::canonicalize(allowed_root).map_err(|e| format!("无法解析允许根目录：{e}"))?;
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("无法读取清理根目录：{e}"))?;
    if is_reparse_point(&metadata) {
        return Err(format!("拒绝重解析清理根：{}", path.display()));
    }
    let canonical_path = fs::canonicalize(path).map_err(|e| format!("无法解析清理根目录：{e}"))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!("拒绝越界清理根目录：{}", path.display()));
    }
    Ok(())
}

fn cache_database_files(directory: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let is_file = entry
                .file_type()
                .map(|kind| kind.is_file() && !kind.is_symlink())
                .unwrap_or(false);
            (is_file && name.starts_with(prefix) && name.ends_with(".db")).then(|| entry.path())
        })
        .collect()
}

fn chromium_cache_roots(local: Option<&Path>, product: &str) -> Vec<PathBuf> {
    let Some(user_data) = local.map(|p| p.join(product)) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&user_data) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_profile = name == "Default" || name.starts_with("Profile ");
            is_profile
                && entry
                    .file_type()
                    .map(|t| t.is_dir() && !t.is_symlink())
                    .unwrap_or(false)
        })
        .flat_map(|entry| {
            let profile = entry.path();
            ["Cache", "Code Cache", "GPUCache"].map(|suffix| profile.join(suffix))
        })
        .filter(|path| path.exists())
        .collect()
}

fn firefox_cache_roots(local: Option<&Path>) -> Vec<PathBuf> {
    let Some(profiles) = local.map(|p| p.join(r"Mozilla\Firefox\Profiles")) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(profiles) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|t| t.is_dir() && !t.is_symlink())
                .unwrap_or(false)
        })
        .map(|entry| entry.path().join("cache2"))
        .filter(|path| path.exists())
        .collect()
}

pub fn is_safe_cleanup_path(path: &Path, protected_roots: &[&Path]) -> Result<(), String> {
    if !path.is_absolute() || path.parent().is_none() {
        return Err(format!("拒绝危险清理路径：{}", path.display()));
    }
    let normalized = normalize_path(path);
    for protected in protected_roots {
        if normalized == normalize_path(protected) {
            return Err(format!("拒绝清理受保护目录：{}", path.display()));
        }
    }
    Ok(())
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn is_c_drive_path(path: &Path) -> bool {
    let normalized = normalize_path(path);
    normalized == "c:" || normalized.starts_with(r"c:\")
}

fn scan_roots(roots: &[PathBuf]) -> CleanupStats {
    scan_roots_with_limit(roots, MAX_CANDIDATE_FILES)
}

fn scan_roots_with_limit(roots: &[PathBuf], limit: u64) -> CleanupStats {
    let mut total = CleanupStats::default();
    for root in roots {
        scan_entry_with_limit(root, &mut total, limit);
        if total.limit_reached {
            break;
        }
    }
    total
}

fn scan_entry_with_limit(path: &Path, total: &mut CleanupStats, limit: u64) {
    if total.files >= limit {
        total.limit_reached = true;
        return;
    }
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if is_reparse_point(&metadata) {
        return;
    }
    if metadata.is_file() {
        total.files += 1;
        total.bytes = total.bytes.saturating_add(metadata.len());
        if total.files >= limit {
            total.limit_reached = true;
        }
        return;
    }
    if !metadata.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        scan_entry_with_limit(&entry.path(), total, limit);
        if total.limit_reached {
            break;
        }
    }
}

fn clean_roots(roots: &[PathBuf]) -> CleanupRunStats {
    clean_roots_with_limit(roots, MAX_CANDIDATE_FILES)
}

fn clean_roots_with_limit(roots: &[PathBuf], limit: u64) -> CleanupRunStats {
    let mut total = CleanupRunStats::default();
    for root in roots {
        clean_entry_with_limit(root, true, &mut total, limit);
        if total.limit_reached {
            break;
        }
    }
    total
}

fn clean_entry_with_limit(path: &Path, keep_root: bool, total: &mut CleanupRunStats, limit: u64) {
    if total.candidate_files >= limit {
        total.limit_reached = true;
        return;
    }
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if is_reparse_point(&metadata) {
        total.skipped_files += 1;
        return;
    }
    if metadata.is_file() {
        total.candidate_files += 1;
        match fs::remove_file(path) {
            Ok(()) => {
                total.deleted_files += 1;
                total.freed_bytes = total.freed_bytes.saturating_add(metadata.len());
            }
            Err(_) => total.skipped_files += 1,
        }
        return;
    }
    if !metadata.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        total.skipped_files += 1;
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        clean_entry_with_limit(&entry.path(), false, total, limit);
        if total.limit_reached {
            break;
        }
    }
    if !keep_root && !total.limit_reached {
        let _ = fs::remove_dir(path);
    }
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn query_recycle_bin() -> Result<CleanupStats, String> {
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    use windows_core::w;
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    unsafe { SHQueryRecycleBinW(w!(r"C:\"), &mut info) }
        .map_err(|e| format!("读取回收站失败：{e}"))?;
    Ok(CleanupStats {
        files: info.i64NumItems.max(0) as u64,
        bytes: info.i64Size.max(0) as u64,
        limit_reached: false,
    })
}

#[cfg(not(windows))]
fn query_recycle_bin() -> Result<CleanupStats, String> {
    Err("C 盘清理仅支持 Windows".into())
}

#[cfg(windows)]
fn empty_recycle_bin() -> Result<CleanupRunStats, String> {
    use windows::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };
    use windows_core::w;
    let before = query_recycle_bin()?;
    unsafe {
        SHEmptyRecycleBinW(
            None,
            w!(r"C:\"),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    }
    .map_err(|e| format!("清空回收站失败：{e}"))?;
    Ok(CleanupRunStats {
        deleted_files: before.files,
        freed_bytes: before.bytes,
        skipped_files: 0,
        limit_reached: false,
        candidate_files: before.files,
    })
}

#[cfg(not(windows))]
fn empty_recycle_bin() -> Result<CleanupRunStats, String> {
    Err("C 盘清理仅支持 Windows".into())
}

#[cfg(test)]
fn scan_directory_for_test(root: &Path) -> CleanupStats {
    scan_roots(&[root.to_path_buf()])
}

#[cfg(test)]
fn clean_directory_for_test(root: &Path) -> CleanupRunStats {
    clean_roots(&[root.to_path_buf()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_and_deep_catalog_have_expected_items() {
        let safe = cleanup_catalog(CleanupMode::Safe);
        let deep = cleanup_catalog(CleanupMode::Deep);
        assert_eq!(safe.len(), 8);
        assert_eq!(deep.len(), 12);
        assert!(safe
            .iter()
            .any(|item| item.id == "recycle_bin" && item.default_selected));
        assert!(deep
            .iter()
            .filter(|item| item.default_selected)
            .all(|item| item.risk == CleanupRisk::Low || item.id == "recycle_bin"));
    }

    #[test]
    fn unknown_item_id_is_rejected() {
        let error = validate_item_ids(CleanupMode::Safe, &["unknown".to_string()]).unwrap_err();
        assert!(error.contains("unknown"));
    }

    #[test]
    fn recycle_bin_requires_explicit_confirmation() {
        let error =
            validate_clean_request(CleanupMode::Safe, &["recycle_bin".to_string()], false, true)
                .unwrap_err();
        assert!(error.contains("回收站"));
    }

    #[test]
    fn scan_and_clean_test_directory_counts_bytes() {
        let root = std::env::temp_dir().join("orange_c_drive_cleaner_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("a.tmp"), vec![0u8; 64]).unwrap();
        std::fs::write(root.join("nested").join("b.tmp"), vec![0u8; 32]).unwrap();
        let scan = scan_directory_for_test(&root);
        assert_eq!(scan.files, 2);
        assert_eq!(scan.bytes, 96);
        let cleaned = clean_directory_for_test(&root);
        assert_eq!(cleaned.deleted_files, 2);
        assert_eq!(cleaned.freed_bytes, 96);
        assert!(!root.join("a.tmp").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dangerous_cleanup_roots_are_rejected() {
        assert!(is_safe_cleanup_path(Path::new(r"C:\"), &[Path::new(r"C:\Windows")]).is_err());
        assert!(
            is_safe_cleanup_path(Path::new(r"C:\Windows"), &[Path::new(r"C:\Windows")]).is_err()
        );
        assert!(
            is_safe_cleanup_path(Path::new(r"C:\Windows\Temp"), &[Path::new(r"C:\Windows")])
                .is_ok()
        );
        assert!(is_c_drive_path(Path::new(r"C:\Users\Test\AppData")));
        assert!(!is_c_drive_path(Path::new(r"D:\Users\Test\AppData")));
    }

    #[test]
    fn chromium_roots_exclude_service_worker_cache_storage() {
        let root = std::env::temp_dir().join("orange_chromium_cache_roots_test");
        let user_data = root.join("Browser").join("User Data");
        let profile = user_data.join("Default");
        for suffix in [
            "Cache",
            "Code Cache",
            "GPUCache",
            r"Service Worker\CacheStorage",
            "Local Storage",
        ] {
            std::fs::create_dir_all(profile.join(suffix)).unwrap();
        }
        let roots = chromium_cache_roots(Some(&root), r"Browser\User Data");
        assert_eq!(roots.len(), 3);
        assert!(roots.iter().all(|path| {
            let value = normalize_path(path);
            [r"\cache", r"\code cache", r"\gpucache"]
                .iter()
                .any(|suffix| value.ends_with(suffix))
        }));
        assert!(!roots.iter().any(|path| {
            let value = normalize_path(path);
            value.contains(r"service worker\cachestorage") || value.contains("local storage")
        }));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn environment_roots_reject_dot_components() {
        assert!(validate_environment_root(Path::new(r"C:\Users\.\Temp")).is_err());
        assert!(validate_environment_root(Path::new(r"C:\Users\Test\..\Temp")).is_err());
        assert!(validate_environment_root(Path::new(r"C:\Users\Test\Temp")).is_ok());
    }

    #[test]
    fn canonical_cleanup_root_must_stay_within_allowed_root() {
        let base = std::env::temp_dir().join("orange_cleanup_root_boundary_test");
        let allowed = base.join("allowed");
        let inside = allowed.join("cache");
        let outside = base.join("outside");
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        assert!(validate_final_cleanup_root(&inside, &allowed).is_ok());
        assert!(validate_final_cleanup_root(&outside, &allowed).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleanup_stops_at_candidate_limit_and_reports_partial() {
        let root = std::env::temp_dir().join("orange_cleanup_candidate_limit_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for index in 0..3 {
            std::fs::write(root.join(format!("{index}.tmp")), [index]).unwrap();
        }
        let stats = clean_roots_with_limit(std::slice::from_ref(&root), 2);
        assert_eq!(stats.deleted_files, 2);
        assert!(stats.limit_reached);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
