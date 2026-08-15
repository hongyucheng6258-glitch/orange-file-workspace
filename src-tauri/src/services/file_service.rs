use std::path::Path;

use crate::error::AppError;

/// 将路径转为绝对路径并统一分隔符为 `\`。
pub fn normalize_path(path: &Path) -> Result<String, AppError> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(abs.to_string_lossy().replace('/', "\\"))
}

/// 生成路径去重键：Windows 下大小写不敏感，统一小写。
pub fn canonical_path_key(path: &str) -> String {
    path.to_lowercase()
}

/// 从扩展名推断 MIME 类型。返回 None 表示未知。
pub fn infer_mime(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        "md" | "markdown" => "text/markdown",
        "txt" => "text/plain",
        "log" => "text/plain",
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" | "c" | "h" | "cpp"
        | "hpp" | "cs" | "rb" | "php" | "swift" | "kt" | "toml" | "yaml" | "yml" | "sh"
        | "sql" | "vue" | "svelte" | "jsonc" => "text/plain",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "exe" => "application/x-msdownload",
        "dll" => "application/x-msdownload",
        "msi" => "application/x-msi",
        _ => return None,
    };
    Some(mime.to_string())
}

/// 判断是否可安全读取为 UTF-8 文本。
#[allow(dead_code)] // 设计 API：外部导入时用于文本/二进制判断，有单元测试覆盖
pub fn is_likely_text(path: &Path) -> bool {
    matches!(
        infer_mime(path).as_deref(),
        Some("text/plain")
            | Some("text/markdown")
            | Some("text/html")
            | Some("text/css")
            | Some("text/csv")
            | Some("application/json")
            | Some("application/xml")
    )
}

/// 读取文件基础元数据（大小与修改时间），不读取内容。
pub fn stat_basic(path: &Path) -> Result<(i64, Option<i64>), AppError> {
    let meta = std::fs::metadata(path)?;
    let size = meta.len() as i64;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    Ok((size, modified))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn normalize_replaces_forward_slashes() {
        let out = normalize_path(Path::new("C:/Users/test/file.txt")).expect("normalize");
        assert!(out.contains("\\"), "should use backslashes: {out}");
    }

    #[test]
    fn canonical_key_is_case_insensitive() {
        assert_eq!(
            canonical_path_key("C:\\Users\\Test\\File.TXT"),
            canonical_path_key("c:\\users\\test\\file.txt")
        );
    }

    #[test]
    fn mime_inference() {
        assert_eq!(
            infer_mime(Path::new("a.png")).as_deref(),
            Some("image/png")
        );
        assert_eq!(
            infer_mime(Path::new("b.PDF")).as_deref(),
            Some("application/pdf")
        );
        assert_eq!(
            infer_mime(Path::new("c.rs")).as_deref(),
            Some("text/plain")
        );
        assert_eq!(infer_mime(Path::new("d.unknown_ext")), None);
    }

    #[test]
    fn text_detection() {
        assert!(is_likely_text(Path::new("x.md")));
        assert!(is_likely_text(Path::new("y.json")));
        assert!(!is_likely_text(Path::new("z.png")));
    }

    #[test]
    fn stat_reports_size_and_time() {
        let tmp = std::env::temp_dir().join(format!("nexus-test-{}", uuid::Uuid::new_v4()));
        std::fs::write(&tmp, b"hello").expect("write");
        let (size, modified) = stat_basic(&tmp).expect("stat");
        assert_eq!(size, 5);
        assert!(modified.is_some());
        let _ = std::fs::remove_file(&tmp);
    }
}
