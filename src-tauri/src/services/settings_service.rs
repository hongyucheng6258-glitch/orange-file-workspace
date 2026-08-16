//! 应用设置领域模型、默认值与持久化。
//!
//! 设置唯一来源为 `app_settings` 表；storage 字段由命令层从 `AppState` 附加，
//! 不参与本模块的写入。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::repositories::{get_setting, set_setting};
use crate::error::AppError;

/// 完整设置。storage 为只读信息（来自 AppState）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    pub general: GeneralSettings,
    pub appearance: AppearanceSettings,
    pub terminal: TerminalSettings,
    pub ignore: IgnoreSettings,
    pub backup: BackupSettings,
    pub storage: StorageSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct GeneralSettings {
    /// 启动后展示的页面：home | files
    pub launch_behavior: String,
    /// 默认导入模式：managed | external
    pub default_import_mode: String,
    /// 重复文件策略：skip | keep_both
    pub duplicate_policy: String,
    /// 文本预览大小上限（MB），1..=1024
    pub preview_size_limit_mb: u32,
    /// 关闭窗口时最小化到系统托盘
    pub minimize_to_tray: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AppearanceSettings {
    /// system | light | dark
    pub theme_mode: String,
    /// comfortable | compact
    pub density: String,
}

/// 内置终端外观设置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TerminalSettings {
    /// 字号（px），10..=24
    pub font_size: u32,
    /// block | bar | underline
    pub cursor_style: String,
    /// 配色方案：campbell | vs_dark | one_dark | dracula | solarized_dark
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct IgnoreRule {
    /// 匹配模式：name 规则匹配文件名/目录名，path 规则匹配路径片段
    pub kind: String,
    pub pattern: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct IgnoreSettings {
    pub custom_rules: Vec<IgnoreRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct BackupSettings {
    pub enabled: bool,
    /// daily | weekly
    pub frequency: String,
    /// "HH:MM"
    pub run_time: String,
    /// 1..=30
    pub retention_count: u32,
    /// metadata | full
    pub backup_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StorageSettings {
    pub data_dir: String,
    pub managed_dir: String,
}

/// 设置键定义。storage 类键不在此列（由迁移流程处理）。
pub const KEY_LAUNCH_BEHAVIOR: &str = "general.launch_behavior";
pub const KEY_DEFAULT_IMPORT_MODE: &str = "general.default_import_mode";
pub const KEY_DUPLICATE_POLICY: &str = "general.duplicate_policy";
pub const KEY_PREVIEW_SIZE_LIMIT_MB: &str = "general.preview_size_limit_mb";
pub const KEY_MINIMIZE_TO_TRAY: &str = "general.minimize_to_tray";
pub const KEY_THEME_MODE: &str = "appearance.theme_mode";
pub const KEY_DENSITY: &str = "appearance.density";
pub const KEY_TERMINAL_FONT_SIZE: &str = "terminal.font_size";
pub const KEY_TERMINAL_CURSOR_STYLE: &str = "terminal.cursor_style";
pub const KEY_TERMINAL_THEME: &str = "terminal.theme";
pub const KEY_CUSTOM_RULES: &str = "ignore.custom_rules";
pub const KEY_BACKUP_ENABLED: &str = "backup.enabled";
pub const KEY_BACKUP_FREQUENCY: &str = "backup.frequency";
pub const KEY_BACKUP_RUN_TIME: &str = "backup.run_time";
pub const KEY_BACKUP_RETENTION: &str = "backup.retention_count";
pub const KEY_BACKUP_TYPE: &str = "backup.backup_type";

/// 默认设置。首次运行与重置分类时使用。
pub fn default_settings() -> AppSettings {
    AppSettings {
        general: GeneralSettings {
            launch_behavior: "home".into(),
            default_import_mode: "managed".into(),
            duplicate_policy: "skip".into(),
            preview_size_limit_mb: 256,
            minimize_to_tray: true,
        },
        appearance: AppearanceSettings {
            theme_mode: "system".into(),
            density: "comfortable".into(),
        },
        terminal: TerminalSettings {
            font_size: 14,
            cursor_style: "bar".into(),
            theme: "campbell".into(),
        },
        ignore: IgnoreSettings {
            custom_rules: Vec::new(),
        },
        backup: BackupSettings {
            enabled: false,
            frequency: "daily".into(),
            run_time: "02:00".into(),
            retention_count: 7,
            backup_type: "full".into(),
        },
        storage: StorageSettings {
            data_dir: String::new(),
            managed_dir: String::new(),
        },
    }
}

/// 分类名到该分类所有键的映射。
pub fn category_keys(category: &str) -> Option<&'static [&'static str]> {
    match category {
        "general" => Some(&[
            KEY_LAUNCH_BEHAVIOR,
            KEY_DEFAULT_IMPORT_MODE,
            KEY_DUPLICATE_POLICY,
            KEY_PREVIEW_SIZE_LIMIT_MB,
            KEY_MINIMIZE_TO_TRAY,
        ]),
        "appearance" => Some(&[KEY_THEME_MODE, KEY_DENSITY]),
        "terminal" => Some(&[
            KEY_TERMINAL_FONT_SIZE,
            KEY_TERMINAL_CURSOR_STYLE,
            KEY_TERMINAL_THEME,
        ]),
        "ignore" => Some(&[KEY_CUSTOM_RULES]),
        "backup" => Some(&[
            KEY_BACKUP_ENABLED,
            KEY_BACKUP_FREQUENCY,
            KEY_BACKUP_RUN_TIME,
            KEY_BACKUP_RETENTION,
            KEY_BACKUP_TYPE,
        ]),
        _ => None,
    }
}

/// 从数据库读取完整设置（storage 由调用方填充）。
pub fn load_settings(conn: &rusqlite::Connection) -> Result<AppSettings, AppError> {
    let mut s = default_settings();
    s.general.launch_behavior = read_str(conn, KEY_LAUNCH_BEHAVIOR, &s.general.launch_behavior)?;
    s.general.default_import_mode = read_str(
        conn,
        KEY_DEFAULT_IMPORT_MODE,
        &s.general.default_import_mode,
    )?;
    s.general.duplicate_policy = read_str(conn, KEY_DUPLICATE_POLICY, &s.general.duplicate_policy)?;
    s.general.preview_size_limit_mb = read_u32(
        conn,
        KEY_PREVIEW_SIZE_LIMIT_MB,
        s.general.preview_size_limit_mb,
    )?;
    s.general.minimize_to_tray = read_bool(conn, KEY_MINIMIZE_TO_TRAY, s.general.minimize_to_tray)?;
    s.appearance.theme_mode = read_str(conn, KEY_THEME_MODE, &s.appearance.theme_mode)?;
    s.appearance.density = read_str(conn, KEY_DENSITY, &s.appearance.density)?;
    s.terminal.font_size = read_u32(conn, KEY_TERMINAL_FONT_SIZE, s.terminal.font_size)?;
    s.terminal.cursor_style = read_str(conn, KEY_TERMINAL_CURSOR_STYLE, &s.terminal.cursor_style)?;
    s.terminal.theme = read_str(conn, KEY_TERMINAL_THEME, &s.terminal.theme)?;
    s.ignore.custom_rules = read_rules(conn)?;
    s.backup.enabled = read_bool(conn, KEY_BACKUP_ENABLED, s.backup.enabled)?;
    s.backup.frequency = read_str(conn, KEY_BACKUP_FREQUENCY, &s.backup.frequency)?;
    s.backup.run_time = read_str(conn, KEY_BACKUP_RUN_TIME, &s.backup.run_time)?;
    s.backup.retention_count = read_u32(conn, KEY_BACKUP_RETENTION, s.backup.retention_count)?;
    s.backup.backup_type = read_str(conn, KEY_BACKUP_TYPE, &s.backup.backup_type)?;
    Ok(s)
}

/// 校验并写入单个设置，返回写入后的完整设置。
/// key 必须属于白名单；value 必须通过对应校验。
pub fn update_setting(
    conn: &rusqlite::Connection,
    key: &str,
    value: Value,
) -> Result<AppSettings, AppError> {
    let encoded = validate_and_encode(key, value)?;
    set_setting(conn, key, &encoded)?;
    load_settings(conn)
}

/// 重置某分类的全部设置，返回重置后的完整设置。
pub fn reset_category(
    conn: &rusqlite::Connection,
    category: &str,
) -> Result<AppSettings, AppError> {
    let keys = category_keys(category)
        .ok_or_else(|| AppError::new("invalid_category", format!("未知设置分类: {category}")))?;
    for key in keys {
        conn.execute("DELETE FROM app_settings WHERE key = ?1", [key])?;
    }
    load_settings(conn)
}

// ---------- 内部工具 ----------

fn read_str(conn: &rusqlite::Connection, key: &str, fallback: &str) -> Result<String, AppError> {
    Ok(get_setting(conn, key)?
        .and_then(|v| serde_json::from_str::<String>(&v).ok())
        .unwrap_or_else(|| fallback.to_string()))
}

fn read_u32(conn: &rusqlite::Connection, key: &str, fallback: u32) -> Result<u32, AppError> {
    Ok(get_setting(conn, key)?
        .and_then(|v| serde_json::from_str::<u32>(&v).ok())
        .unwrap_or(fallback))
}

fn read_bool(conn: &rusqlite::Connection, key: &str, fallback: bool) -> Result<bool, AppError> {
    Ok(get_setting(conn, key)?
        .and_then(|v| serde_json::from_str::<bool>(&v).ok())
        .unwrap_or(fallback))
}

fn read_rules(conn: &rusqlite::Connection) -> Result<Vec<IgnoreRule>, AppError> {
    let raw = get_setting(conn, KEY_CUSTOM_RULES)?;
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let rules: Vec<IgnoreRule> = serde_json::from_str(&raw).unwrap_or_default();
    Ok(rules)
}

/// 按 key 校验 value，返回可写入的 JSON 字符串。
fn validate_and_encode(key: &str, value: Value) -> Result<String, AppError> {
    let bad = |why: &str| AppError::new("invalid_setting", format!("{key} 不合法: {why}"));
    match key {
        KEY_LAUNCH_BEHAVIOR => {
            let s = value.as_str().ok_or_else(|| bad("必须是字符串"))?;
            if !matches!(s, "home" | "files") {
                return Err(bad("取值应为 home 或 files"));
            }
            Ok(serde_json::to_string(s)?)
        }
        KEY_DEFAULT_IMPORT_MODE => {
            let s = value.as_str().ok_or_else(|| bad("必须是字符串"))?;
            if !matches!(s, "managed" | "external") {
                return Err(bad("取值应为 managed 或 external"));
            }
            Ok(serde_json::to_string(s)?)
        }
        KEY_DUPLICATE_POLICY => {
            let s = value.as_str().ok_or_else(|| bad("必须是字符串"))?;
            if !matches!(s, "skip" | "keep_both") {
                return Err(bad("取值应为 skip 或 keep_both"));
            }
            Ok(serde_json::to_string(s)?)
        }
        KEY_PREVIEW_SIZE_LIMIT_MB => {
            let n = value.as_u64().ok_or_else(|| bad("必须是整数"))?;
            if !(1..=1024).contains(&n) {
                return Err(bad("取值范围 1..=1024"));
            }
            Ok(serde_json::to_string(&(n as u32))?)
        }
        KEY_MINIMIZE_TO_TRAY => {
            let b = value.as_bool().ok_or_else(|| bad("必须是布尔值"))?;
            Ok(serde_json::to_string(&b)?)
        }
        KEY_THEME_MODE => {
            let s = value.as_str().ok_or_else(|| bad("必须是字符串"))?;
            if !matches!(s, "system" | "light" | "dark") {
                return Err(bad("取值应为 system、light 或 dark"));
            }
            Ok(serde_json::to_string(s)?)
        }
        KEY_DENSITY => {
            let s = value.as_str().ok_or_else(|| bad("必须是字符串"))?;
            if !matches!(s, "comfortable" | "compact") {
                return Err(bad("取值应为 comfortable 或 compact"));
            }
            Ok(serde_json::to_string(s)?)
        }
        KEY_TERMINAL_FONT_SIZE => {
            let n = value.as_u64().ok_or_else(|| bad("必须是整数"))?;
            if !(10..=24).contains(&n) {
                return Err(bad("取值范围 10..=24"));
            }
            Ok(serde_json::to_string(&(n as u32))?)
        }
        KEY_TERMINAL_CURSOR_STYLE => {
            let s = value.as_str().ok_or_else(|| bad("必须是字符串"))?;
            if !matches!(s, "block" | "bar" | "underline") {
                return Err(bad("取值应为 block、bar 或 underline"));
            }
            Ok(serde_json::to_string(s)?)
        }
        KEY_TERMINAL_THEME => {
            let s = value.as_str().ok_or_else(|| bad("必须是字符串"))?;
            if !matches!(
                s,
                "campbell" | "vs_dark" | "one_dark" | "dracula" | "solarized_dark"
            ) {
                return Err(bad("未知的配色方案"));
            }
            Ok(serde_json::to_string(s)?)
        }
        KEY_CUSTOM_RULES => {
            let arr = value.as_array().ok_or_else(|| bad("必须是数组"))?;
            let mut rules = Vec::with_capacity(arr.len());
            for (i, item) in arr.iter().enumerate() {
                let obj = item
                    .as_object()
                    .ok_or_else(|| bad(&format!("第 {i} 项不是对象")))?;
                let kind = obj
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| bad(&format!("第 {i} 项缺少 kind")))?
                    .to_string();
                let pattern = obj
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| bad(&format!("第 {i} 项缺少 pattern")))?
                    .to_string();
                let enabled = obj
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| bad(&format!("第 {i} 项缺少 enabled")))?;
                if !matches!(kind.as_str(), "name" | "path") {
                    return Err(bad(&format!("第 {i} 项 kind 应为 name 或 path")));
                }
                if pattern.is_empty() || pattern.len() > 200 {
                    return Err(bad(&format!("第 {i} 项 pattern 长度应为 1..=200")));
                }
                rules.push(IgnoreRule {
                    kind,
                    pattern,
                    enabled,
                });
            }
            Ok(serde_json::to_string(&rules)?)
        }
        KEY_BACKUP_ENABLED => {
            let b = value.as_bool().ok_or_else(|| bad("必须是布尔值"))?;
            Ok(serde_json::to_string(&b)?)
        }
        KEY_BACKUP_FREQUENCY => {
            let s = value.as_str().ok_or_else(|| bad("必须是字符串"))?;
            if !matches!(s, "daily" | "weekly") {
                return Err(bad("取值应为 daily 或 weekly"));
            }
            Ok(serde_json::to_string(s)?)
        }
        KEY_BACKUP_RUN_TIME => {
            let s = value.as_str().ok_or_else(|| bad("必须是字符串"))?;
            let valid = s.len() == 5
                && s.as_bytes()[2] == b':'
                && s.as_bytes()[0].is_ascii_digit()
                && s.as_bytes()[1].is_ascii_digit()
                && s.as_bytes()[3].is_ascii_digit()
                && s.as_bytes()[4].is_ascii_digit()
                && s[..2].parse::<u8>().map(|h| h < 24).unwrap_or(false)
                && s[3..].parse::<u8>().map(|m| m < 60).unwrap_or(false);
            if !valid {
                return Err(bad("应为 HH:MM（24 小时制）"));
            }
            Ok(serde_json::to_string(s)?)
        }
        KEY_BACKUP_RETENTION => {
            let n = value.as_u64().ok_or_else(|| bad("必须是整数"))?;
            if !(1..=30).contains(&n) {
                return Err(bad("取值范围 1..=30"));
            }
            Ok(serde_json::to_string(&(n as u32))?)
        }
        KEY_BACKUP_TYPE => {
            let s = value.as_str().ok_or_else(|| bad("必须是字符串"))?;
            if !matches!(s, "metadata" | "full") {
                return Err(bad("取值应为 metadata 或 full"));
            }
            Ok(serde_json::to_string(s)?)
        }
        _ => Err(AppError::new(
            "unknown_setting",
            format!("未知设置项: {key}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;

    fn conn() -> rusqlite::Connection {
        let mut c = rusqlite::Connection::open_in_memory().expect("db");
        run_migrations(&mut c).expect("migrations");
        c
    }

    #[test]
    fn defaults_are_returned_when_empty() {
        let c = conn();
        let s = load_settings(&c).expect("load");
        assert_eq!(s.general.default_import_mode, "managed");
        assert_eq!(s.appearance.theme_mode, "system");
        assert_eq!(s.backup.retention_count, 7);
        assert_eq!(s.terminal.font_size, 14);
        assert_eq!(s.terminal.cursor_style, "bar");
        assert_eq!(s.terminal.theme, "campbell");
    }

    #[test]
    fn terminal_settings_update_and_persist() {
        let c = conn();
        let s = update_setting(&c, KEY_TERMINAL_FONT_SIZE, Value::from(18)).expect("update font");
        assert_eq!(s.terminal.font_size, 18);
        let s = update_setting(&c, KEY_TERMINAL_CURSOR_STYLE, Value::String("block".into()))
            .expect("update cursor");
        assert_eq!(s.terminal.cursor_style, "block");
        let s = update_setting(&c, KEY_TERMINAL_THEME, Value::String("dracula".into()))
            .expect("update theme");
        assert_eq!(s.terminal.theme, "dracula");
        let reloaded = load_settings(&c).expect("reload");
        assert_eq!(reloaded.terminal.font_size, 18);
        assert_eq!(reloaded.terminal.cursor_style, "block");
        assert_eq!(reloaded.terminal.theme, "dracula");
    }

    #[test]
    fn invalid_terminal_values_are_rejected() {
        let c = conn();
        assert!(update_setting(&c, KEY_TERMINAL_FONT_SIZE, Value::from(8)).is_err());
        assert!(update_setting(&c, KEY_TERMINAL_FONT_SIZE, Value::from(30)).is_err());
        assert!(
            update_setting(&c, KEY_TERMINAL_CURSOR_STYLE, Value::String("dash".into())).is_err()
        );
        assert!(update_setting(&c, KEY_TERMINAL_THEME, Value::String("monokai".into())).is_err());
    }

    #[test]
    fn reset_terminal_category_restores_defaults() {
        let c = conn();
        let _ = update_setting(&c, KEY_TERMINAL_FONT_SIZE, Value::from(20));
        let _ = update_setting(
            &c,
            KEY_TERMINAL_CURSOR_STYLE,
            Value::String("underline".into()),
        );
        let s = reset_category(&c, "terminal").expect("reset");
        assert_eq!(s.terminal.font_size, 14);
        assert_eq!(s.terminal.cursor_style, "bar");
        assert_eq!(s.terminal.theme, "campbell");
    }

    #[test]
    fn update_persists_and_returns_new_value() {
        let c = conn();
        let s = update_setting(&c, KEY_THEME_MODE, Value::String("dark".into())).expect("update");
        assert_eq!(s.appearance.theme_mode, "dark");
        let reloaded = load_settings(&c).expect("reload");
        assert_eq!(reloaded.appearance.theme_mode, "dark");
    }

    #[test]
    fn invalid_values_are_rejected() {
        let c = conn();
        assert!(update_setting(&c, KEY_THEME_MODE, Value::String("blue".into())).is_err());
        assert!(update_setting(&c, KEY_PREVIEW_SIZE_LIMIT_MB, Value::from(0)).is_err());
        assert!(update_setting(&c, KEY_BACKUP_RUN_TIME, Value::String("25:00".into())).is_err());
        assert!(update_setting(&c, KEY_BACKUP_RUN_TIME, Value::String("12:99".into())).is_err());
    }

    #[test]
    fn minimize_to_tray_defaults_true_and_updates() {
        let c = conn();
        let s = load_settings(&c).expect("load");
        assert!(s.general.minimize_to_tray);
        let s = update_setting(&c, KEY_MINIMIZE_TO_TRAY, Value::Bool(false)).expect("update");
        assert!(!s.general.minimize_to_tray);
        let reloaded = load_settings(&c).expect("reload");
        assert!(!reloaded.general.minimize_to_tray);
    }

    #[test]
    fn minimize_to_tray_rejects_non_bool() {
        let c = conn();
        assert!(update_setting(&c, KEY_MINIMIZE_TO_TRAY, Value::from(1)).is_err());
        assert!(update_setting(&c, KEY_MINIMIZE_TO_TRAY, Value::String("yes".into())).is_err());
    }

    #[test]
    fn reset_general_restores_minimize_to_tray_default() {
        let c = conn();
        let _ = update_setting(&c, KEY_MINIMIZE_TO_TRAY, Value::Bool(false));
        let s = reset_category(&c, "general").expect("reset");
        assert!(s.general.minimize_to_tray);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let c = conn();
        assert!(update_setting(&c, "evil.key", Value::Bool(true)).is_err());
    }

    #[test]
    fn custom_rules_are_validated() {
        let c = conn();
        let good = serde_json::json!([
            { "kind": "name", "pattern": "*.tmp", "enabled": true }
        ]);
        let s = update_setting(&c, KEY_CUSTOM_RULES, good).expect("good rules");
        assert_eq!(s.ignore.custom_rules.len(), 1);

        let bad_kind = serde_json::json!([
            { "kind": "regex", "pattern": "x", "enabled": true }
        ]);
        assert!(update_setting(&c, KEY_CUSTOM_RULES, bad_kind).is_err());

        let empty_pattern = serde_json::json!([
            { "kind": "name", "pattern": "", "enabled": true }
        ]);
        assert!(update_setting(&c, KEY_CUSTOM_RULES, empty_pattern).is_err());
    }

    #[test]
    fn reset_category_restores_defaults() {
        let c = conn();
        let _ = update_setting(&c, KEY_THEME_MODE, Value::String("dark".into()));
        let _ = update_setting(&c, KEY_DENSITY, Value::String("compact".into()));
        let s = reset_category(&c, "appearance").expect("reset");
        assert_eq!(s.appearance.theme_mode, "system");
        assert_eq!(s.appearance.density, "comfortable");
    }

    #[test]
    fn unknown_category_is_rejected() {
        let c = conn();
        assert!(reset_category(&c, "nope").is_err());
    }
}
