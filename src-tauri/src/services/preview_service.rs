use std::io::Read;
use std::path::Path;

use crate::error::AppError;

/// 文本预览最大字节数。
const PREVIEW_MAX_BYTES: usize = 256 * 1024;

/// 读取文件前 N 字节并尝试解析为 UTF-8 文本预览。
/// 超过限制时在结尾追加截断提示。
pub fn read_text_preview(path: &Path) -> Result<String, AppError> {
    let meta = std::fs::metadata(path)?;
    let read_len = (meta.len() as usize).min(PREVIEW_MAX_BYTES);

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

    if meta.len() as usize > PREVIEW_MAX_BYTES {
        Ok(format!("{text}\n\n… 内容过长，仅显示前 {} KB", PREVIEW_MAX_BYTES / 1024))
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

        let text = read_text_preview(&tmp).expect("read");
        assert_eq!(text, "hello\nworld");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn strips_bom() {
        let tmp = std::env::temp_dir().join(format!("nexus-bom-{}", crate::db::models::new_id()));
        std::fs::write(&tmp, [0xEF, 0xBB, 0xBF, b'h', b'i']).expect("write");

        let text = read_text_preview(&tmp).expect("read");
        assert_eq!(text, "hi");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn rejects_binary() {
        let tmp = std::env::temp_dir().join(format!("nexus-bin-{}", crate::db::models::new_id()));
        std::fs::write(&tmp, [0x00, 0x01, 0x02, 0xFF, 0xFE]).expect("write");

        assert!(read_text_preview(&tmp).is_err());

        let _ = std::fs::remove_file(&tmp);
    }
}
