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
}
