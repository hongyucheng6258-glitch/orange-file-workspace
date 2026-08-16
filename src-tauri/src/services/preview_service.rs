use std::io::Read;
use std::path::Path;

use crate::error::AppError;

/// 读取文件前 N 字节并尝试解析为 UTF-8 文本预览。
/// limit_kb 为上限（KB），超过时在结尾追加截断提示。
pub fn read_text_preview(path: &Path, limit_kb: usize) -> Result<String, AppError> {
    let max_bytes = limit_kb.saturating_mul(1024).max(1024);
    let meta = std::fs::metadata(path)?;
    let read_len = (meta.len() as usize).min(max_bytes);

    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; read_len.max(1)];
    let mut filled = 0;
    while filled < buf.len() {
        let n = file.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    buf.truncate(filled);

    // 去掉 BOM
    let bytes = if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &buf[3..]
    } else {
        &buf[..]
    };

    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => return Err(AppError::new("not_utf8", "文件不是 UTF-8 文本")),
    };

    if meta.len() as usize > max_bytes {
        Ok(format!("{text}\n\n… 内容过长，仅显示前 {} KB", limit_kb))
    } else {
        Ok(text)
    }
}

/// 完整读取 UTF-8 文本文件，供编辑器使用。
/// 与 read_text_preview 不同：不截断、不追加提示；超过 max_bytes 返回 file_too_large。
pub fn read_text_full(path: &Path, max_bytes: u64) -> Result<String, AppError> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_bytes {
        return Err(AppError::new(
            "file_too_large",
            format!("文件过大（{} 字节），超过编辑上限 {}", meta.len(), max_bytes),
        ));
    }
    let mut file = std::fs::File::open(path)?;
    let mut buf = Vec::with_capacity(meta.len() as usize);
    file.read_to_end(&mut buf)?;

    let bytes = if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &buf[3..]
    } else {
        &buf[..]
    };
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => Err(AppError::new("not_utf8", "文件不是 UTF-8 文本")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_utf8_text() {
        let tmp = std::env::temp_dir().join(format!("nexus-prev-{}", crate::db::models::new_id()));
        std::fs::write(&tmp, "hello\nworld").expect("write");

        let text = read_text_preview(&tmp, 256).expect("read");
        assert_eq!(text, "hello\nworld");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn strips_bom() {
        let tmp = std::env::temp_dir().join(format!("nexus-bom-{}", crate::db::models::new_id()));
        std::fs::write(&tmp, [0xEF, 0xBB, 0xBF, b'h', b'i']).expect("write");

        let text = read_text_preview(&tmp, 256).expect("read");
        assert_eq!(text, "hi");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn rejects_binary() {
        let tmp = std::env::temp_dir().join(format!("nexus-bin-{}", crate::db::models::new_id()));
        std::fs::write(&tmp, [0x00, 0x01, 0x02, 0xFF, 0xFE]).expect("write");

        assert!(read_text_preview(&tmp, 256).is_err());

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn respects_custom_limit() {
        let tmp = std::env::temp_dir().join(format!("nexus-lim-{}", crate::db::models::new_id()));
        // 4 KB 内容，限制 1 KB：应包含截断提示
        let content = vec![b'a'; 4096];
        std::fs::write(&tmp, &content).expect("write");

        let text = read_text_preview(&tmp, 1).expect("read");
        assert!(text.contains("内容过长"), "limit hint expected");
        assert!(text.len() < 2048, "text should be truncated");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn read_text_full_returns_entire_file() {
        let tmp = std::env::temp_dir().join(format!("nexus-full-{}", crate::db::models::new_id()));
        // 300KiB，超过原 256KiB 预览上限
        let content = vec![b'a'; 300 * 1024];
        std::fs::write(&tmp, &content).expect("write");

        let text = read_text_full(&tmp, 10 * 1024 * 1024).expect("read");
        assert_eq!(text.len(), 300 * 1024, "必须返回完整内容");
        assert!(!text.contains("内容过长"), "不应包含截断提示");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn read_text_full_rejects_oversize() {
        let tmp = std::env::temp_dir().join(format!("nexus-over-{}", crate::db::models::new_id()));
        std::fs::write(&tmp, vec![b'a'; 2048]).expect("write");

        let err = read_text_full(&tmp, 1024).expect_err("should reject");
        assert_eq!(err.code, "file_too_large");

        let _ = std::fs::remove_file(&tmp);
    }
}
