//! 开机自启：通过注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 管理。
//! 状态以注册表为准，是唯一事实来源，不写入应用数据库。

use std::path::Path;

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

/// 注册表 Run 键路径。
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// 本应用的自启值名称（与应用 identifier 一致）。
const VALUE_NAME: &str = "com.nexus.file-workspace";

/// 生成注册表值：带引号的 exe 绝对路径。
fn command_value(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// 启用或禁用开机自启。
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run = hkcu
        .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
        .or_else(|_| hkcu.create_subkey(RUN_KEY).map(|(key, _)| key))
        .map_err(|e| format!("无法打开注册表 Run 键: {e}"))?;

    if enabled {
        let exe =
            std::env::current_exe().map_err(|e| format!("无法获取应用可执行文件路径: {e}"))?;
        run.set_value(VALUE_NAME, &command_value(&exe))
            .map_err(|e| format!("写入开机自启失败: {e}"))?;
    } else {
        match run.delete_value(VALUE_NAME) {
            Ok(()) => {}
            // 值不存在时视为已禁用
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("移除开机自启失败: {e}")),
        }
    }
    Ok(())
}

/// 查询开机自启是否启用。
pub fn is_enabled() -> bool {
    let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ)
    else {
        return false;
    };
    hkcu.get_value::<String, _>(VALUE_NAME).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_value_quotes_exe_path() {
        assert_eq!(
            command_value(Path::new(r"C:\Program Files\Orange\orange.exe")),
            r#""C:\Program Files\Orange\orange.exe""#
        );
        assert_eq!(
            command_value(Path::new(r"C:\Orange.exe")),
            r#""C:\Orange.exe""#
        );
    }
}
