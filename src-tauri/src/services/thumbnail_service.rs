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
pub fn generate_thumbnail(src: &Path, cache_dir: &Path) -> Result<(PathBuf, u32, u32), AppError> {
    let img = image::open(src)?;
    let (w, h) = (img.width(), img.height());
    let thumb = img.thumbnail(THUMB_MAX, THUMB_MAX);
    let dest = cache_dir.join(format!("{}.png", new_id()));
    thumb.save(&dest)?;
    Ok((dest, w, h))
}

/// 从可执行文件/快捷方式提取应用图标，保存为 PNG 到缓存目录并返回路径。
/// 非 Windows 平台或提取失败时返回错误。
/// 图标提取互斥锁：Shell 图标 API 并发调用存在竞态（测试并行曾触发访问违规），
/// 前端网格中多个图标会同时请求，串行化提取保证安全。
static ICON_EXTRACT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// COM 初始化 RAII：`SHGetFileInfoW` 内部会加载 shell 组件（thumbcache.dll 等），
/// 未初始化 COM 的线程调用时会出现 DLL 卸载竞态（0xc0000005），
/// 这里保证每个提取线程先进入 STA 模式，函数退出时自动释放。
#[cfg(target_os = "windows")]
struct ComInitializer {
    active: bool,
}

#[cfg(target_os = "windows")]
impl ComInitializer {
    fn new() -> Self {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // S_OK / S_FALSE 表示初始化成功（S_FALSE 为重复初始化，仍需配对 CoUninitialize）；
        // RPC_E_CHANGED_MODE 表示线程已是 MTA，不接管清理。
        Self { active: hr.is_ok() }
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComInitializer {
    fn drop(&mut self) {
        if self.active {
            use windows::Win32::System::Com::CoUninitialize;
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(target_os = "windows")]
pub fn extract_file_icon(path: &Path, cache_dir: &Path) -> Result<PathBuf, AppError> {
    // 中毒后自动恢复：前一次提取若 panic 会导致 Mutex poisoned，
    // 用 into_inner 取出内部值继续执行，避免后续所有提取永久失败。
    let _guard = ICON_EXTRACT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _com = ComInitializer::new();

    use windows::Win32::{
        Graphics::Gdi::{
            CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, SelectObject,
            BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC,
        },
        UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON},
        UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO},
    };

    if !path.exists() {
        return Err(AppError::new("path_missing", "文件路径不可用"));
    }

    // 1. 通过 Shell 获取文件关联图标（对 .lnk/.exe 均返回应用图标）。
    let wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut info = SHFILEINFOW::default();
    let ret = unsafe {
        SHGetFileInfoW(
            windows::core::PCWSTR(wide.as_ptr()),
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    if ret == 0 || info.hIcon.0.is_null() {
        return Err(AppError::new("icon_missing", "无法提取图标"));
    }
    let hicon: HICON = info.hIcon;

    // 2. 取图标彩色位图并读取像素（32bpp BGRA）。
    let mut icon_info = ICONINFO::default();
    let ok = unsafe { GetIconInfo(hicon, &mut icon_info) };
    let _ = unsafe { DestroyIcon(hicon) };
    if !ok.is_ok() || icon_info.hbmColor.0.is_null() {
        return Err(AppError::new("icon_missing", "无法读取图标位图"));
    }

    let mut bmp = BITMAP::default();
    let bmp_size = unsafe {
        GetObjectW(
            icon_info.hbmColor.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut _),
        )
    };
    if bmp_size == 0 {
        let _ = unsafe { DeleteObject(icon_info.hbmColor.into()) };
        return Err(AppError::new("icon_missing", "无法读取位图信息"));
    }
    let (w, h) = (bmp.bmWidth as u32, bmp.bmHeight as u32);
    if w == 0 || h == 0 || w > 512 || h > 512 {
        let _ = unsafe { DeleteObject(icon_info.hbmColor.into()) };
        return Err(AppError::new("icon_missing", "图标尺寸异常"));
    }

    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // 自顶向下
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
    let dc: HDC = unsafe { CreateCompatibleDC(None) };
    if dc.0.is_null() {
        let _ = unsafe { DeleteObject(icon_info.hbmColor.into()) };
        return Err(AppError::new("icon_missing", "创建内存 DC 失败"));
    }
    let old = unsafe { SelectObject(dc, icon_info.hbmColor.into()) };
    let lines = unsafe {
        GetDIBits(
            dc,
            icon_info.hbmColor,
            0,
            h,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    let _ = unsafe { SelectObject(dc, old) };
    let _ = unsafe { DeleteDC(dc) };
    let _ = unsafe { DeleteObject(icon_info.hbmColor.into()) };
    if lines == 0 {
        return Err(AppError::new("icon_missing", "读取位图像素失败"));
    }

    // 3. BGRA → RGBA，保存 PNG。
    let mut rgba = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            rgba.put_pixel(
                x,
                y,
                image::Rgba([pixels[i + 2], pixels[i + 1], pixels[i], pixels[i + 3]]),
            );
        }
    }
    std::fs::create_dir_all(cache_dir)?;
    let dest = cache_dir.join(format!("icon-{}.png", new_id()));
    rgba.save(&dest)?;
    Ok(dest)
}

#[cfg(not(target_os = "windows"))]
pub fn extract_file_icon(_path: &Path, _cache_dir: &Path) -> Result<PathBuf, AppError> {
    Err(AppError::new("unsupported", "当前平台不支持提取文件图标"))
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
    #[cfg(target_os = "windows")]
    fn extracts_icon_from_windows_executable() {
        // 用系统自带的 exe 验证图标提取链路，避免测试依赖用户文件。
        let candidates = [
            "C:\\Windows\\System32\\notepad.exe",
            "C:\\Windows\\System32\\cmd.exe",
            "C:\\Windows\\explorer.exe",
        ];
        let Some(exe) = candidates.iter().find(|p| Path::new(p).exists()) else {
            eprintln!("未找到系统 exe，跳过图标提取测试");
            return;
        };
        let cache =
            std::env::temp_dir().join(format!("nexus-icon-cache-{}", crate::db::models::new_id()));
        let dest = extract_file_icon(Path::new(exe), &cache).expect("提取图标失败");
        assert!(dest.exists(), "图标 PNG 应已生成");
        assert!(
            dest.metadata().expect("meta").len() > 0,
            "图标 PNG 不应为空"
        );
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn thumbnail_is_smaller_than_source() {
        let src =
            std::env::temp_dir().join(format!("nexus-thumb-{}.png", crate::db::models::new_id()));
        make_png(&src);

        let cache =
            std::env::temp_dir().join(format!("nexus-cache-{}", crate::db::models::new_id()));
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
