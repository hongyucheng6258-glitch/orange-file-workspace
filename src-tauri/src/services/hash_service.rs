use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::AppError;

/// 计算文件 SHA-256 哈希（十六进制小写）。
pub fn sha256_file(path: &Path) -> Result<String, AppError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_hex() {
        let tmp = std::env::temp_dir().join(format!("nexus-hash-{}", crate::db::models::new_id()));
        std::fs::write(&tmp, b"hello world").expect("write");

        let h1 = sha256_file(&tmp).expect("hash 1");
        let h2 = sha256_file(&tmp).expect("hash 2");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn hash_differs_by_content() {
        let a = std::env::temp_dir().join(format!("nexus-ha-{}", crate::db::models::new_id()));
        let b = std::env::temp_dir().join(format!("nexus-hb-{}", crate::db::models::new_id()));
        std::fs::write(&a, b"aaa").expect("write");
        std::fs::write(&b, b"bbb").expect("write");

        assert_ne!(
            sha256_file(&a).expect("hash a"),
            sha256_file(&b).expect("hash b")
        );

        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
    }
}
