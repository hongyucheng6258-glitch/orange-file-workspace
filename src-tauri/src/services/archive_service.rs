use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// 压缩包内单个条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<String>,
}

/// 压缩包信息：条目列表 + 统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    pub format: String,
    pub entry_count: usize,
    pub total_uncompressed: u64,
    pub entries: Vec<ArchiveEntry>,
}

/// 列出压缩包内容。目前支持 zip 格式。
pub fn list_archive(path: &Path, max_entries: usize) -> Result<ArchiveInfo, AppError> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "zip" => list_zip(path, max_entries),
        _ => {
            // 尝试以 zip 方式打开（某些 .jar .epub 等也是 zip）
            if list_zip(path, max_entries).is_ok() {
                return list_zip(path, max_entries);
            }
            Err(AppError::new(
                "unsupported_archive",
                format!("不支持的压缩格式: .{ext}"),
            ))
        }
    }
}

fn list_zip(path: &Path, max_entries: usize) -> Result<ArchiveInfo, AppError> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| AppError::new("archive_read_error", format!("读取压缩包失败: {e}")))?;

    let total = archive.len();
    let limit = total.min(max_entries);
    let mut entries = Vec::with_capacity(limit);
    let mut total_uncompressed: u64 = 0;

    for i in 0..limit {
        let entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.name().to_string();
        let size = entry.size();
        let is_dir = entry.is_dir();
        let modified = entry
            .last_modified()
            .map(|d| format!("{}-{:02}-{:02} {:02}:{:02}", d.year(), d.month(), d.day(), d.hour(), d.minute()));
        total_uncompressed += size;
        entries.push(ArchiveEntry {
            path: name,
            size,
            is_dir,
            modified,
        });
    }

    Ok(ArchiveInfo {
        format: "zip".to_string(),
        entry_count: total,
        total_uncompressed,
        entries,
    })
}
