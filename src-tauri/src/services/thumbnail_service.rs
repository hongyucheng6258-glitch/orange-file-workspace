use std::path::{Path, PathBuf};

use crate::db::models::new_id;
use crate::error::AppError;

/// 缩略图边长上限。
const THUMB_MAX: u32 = 256;

/// 图片扩展名判断。
pub fn is_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .as_deref(),
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp") | Some("bmp")
    )
}

/// 生成缩略图并保存为 PNG 到缓存目录。返回 (缓存路径, 原图宽, 原图高)。
pub fn generate_thumbnail(
    src: &Path,
    cache_dir: &Path,
) -> Result<(PathBuf, u32, u32), AppError> {
    let img = image::open(src)?;
    let (w, h) = (img.width(), img.height());
    let thumb = img.thumbnail(THUMB_MAX, THUMB_MAX);
    let dest = cache_dir.join(format!("{}.png", new_id()));
    thumb.save(&dest)?;
    Ok((dest, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_png(path: &Path) {
        let img = image::RgbaImage::from_pixel(400, 300, image::Rgba([10, 120, 230, 255]));
        img.save(path).expect("save png");
    }

    #[test]
    fn is_image_detection() {
        assert!(is_image(Path::new("a.PNG")));
        assert!(is_image(Path::new("b.jpeg")));
        assert!(is_image(Path::new("c.webp")));
        assert!(!is_image(Path::new("d.txt")));
        assert!(!is_image(Path::new("e.rs")));
    }

    #[test]
    fn thumbnail_is_smaller_than_source() {
        let src = std::env::temp_dir().join(format!("nexus-thumb-{}.png", crate::db::models::new_id()));
        make_png(&src);

        let cache = std::env::temp_dir().join(format!("nexus-cache-{}", crate::db::models::new_id()));
        std::fs::create_dir_all(&cache).expect("mkdir");

        let (dest, w, h) = generate_thumbnail(&src, &cache).expect("generate");
        assert!(dest.exists());
        assert_eq!(w, 400);
        assert_eq!(h, 300);
        assert!(dest.metadata().expect("meta").len() > 0);

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_dir_all(&cache);
    }
}
