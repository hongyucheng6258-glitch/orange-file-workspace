# 应用设置中心实施计划 · 设置核心

对应设计文档 `docs/superpowers/specs/2026-08-16-application-settings-design.md` 的第 2、3、4、7 节与本计划阶段 1-3。本计划交付可运行的"设置基础设施 + 外观与通用设置 + 忽略规则"。

## 前置说明

- 工作区：`e:\work\新建文件夹`（`src` 为 React 前端，`src-tauri` 为 Rust 后端）。
- 环境限制：本机无 git 可执行程序，所有「提交」步骤在 git 可用时执行，否则跳过并记录。
- 验证命令：Rust 用 `cargo test --manifest-path src-tauri/Cargo.toml`；前端用 `npm run build`（无 git 环境时以 TypeScript 编译为验证手段）。
- 现有可复用资产：`src-tauri/src/db/repositories.rs` 已提供 `get_setting` / `set_setting`；`app_settings` 表已存在（key/value_json/updated_at）；`src/lib/tauri.ts` 的 `call` 统一封装 `invoke`。

## 目录结构约定（本计划新增/修改文件）

```text
src-tauri/src/services/settings_service.rs   # 新建：设置领域模型、默认值、校验、读写
src-tauri/src/commands/settings.rs           # 新建：get_settings / update_setting / reset_settings_category
src-tauri/src/commands/mod.rs                # 修改：声明 settings
src-tauri/src/services/mod.rs                # 修改：声明 settings_service
src-tauri/src/lib.rs                         # 修改：注册 settings 命令
src-tauri/src/services/import_service.rs     # 修改：忽略规则与重复策略接入
src-tauri/src/services/preview_service.rs    # 修改：预览上限参数化
src-tauri/src/commands/previews.rs           # 修改：get_text_preview 读取设置

src/features/settings/stores/settingsStore.ts # 新建：前端设置状态
src/lib/theme.ts                              # 新建：主题应用与系统监听
src/main.tsx                                  # 修改：启动时应用主题
src/styles/tokens.css                         # 修改：data-theme 作用域
src/features/settings/components/*.tsx        # 新建：标签、通用、外观、忽略规则、存储、关于、备份子组件
src/features/settings/routes/SettingsPage.tsx # 修改：标签化重构
src/styles/app.css                            # 修改：设置页新样式
vitest.config.ts                              # 新建：前端测试配置（最小化）
src/features/settings/stores/settingsStore.test.ts  # 新建：store 单元测试
src/lib/theme.test.ts                         # 新建：主题工具测试
```

---

## 阶段 1：设置后端基础设施

### 任务 1.1 新建 `settings_service.rs` 领域模型与默认值

文件：`src-tauri/src/services/settings_service.rs`

```rust
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AppearanceSettings {
    /// system | light | dark
    pub theme_mode: String,
    /// comfortable | compact
    pub density: String,
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
pub const KEY_THEME_MODE: &str = "appearance.theme_mode";
pub const KEY_DENSITY: &str = "appearance.density";
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
        },
        appearance: AppearanceSettings {
            theme_mode: "system".into(),
            density: "comfortable".into(),
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
        ]),
        "appearance" => Some(&[KEY_THEME_MODE, KEY_DENSITY]),
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
    s.general.default_import_mode =
        read_str(conn, KEY_DEFAULT_IMPORT_MODE, &s.general.default_import_mode)?;
    s.general.duplicate_policy =
        read_str(conn, KEY_DUPLICATE_POLICY, &s.general.duplicate_policy)?;
    s.general.preview_size_limit_mb =
        read_u32(conn, KEY_PREVIEW_SIZE_LIMIT_MB, s.general.preview_size_limit_mb)?;
    s.appearance.theme_mode = read_str(conn, KEY_THEME_MODE, &s.appearance.theme_mode)?;
    s.appearance.density = read_str(conn, KEY_DENSITY, &s.appearance.density)?;
    s.ignore.custom_rules = read_rules(conn)?;
    s.backup.enabled = read_bool(conn, KEY_BACKUP_ENABLED, s.backup.enabled)?;
    s.backup.frequency = read_str(conn, KEY_BACKUP_FREQUENCY, &s.backup.frequency)?;
    s.backup.run_time = read_str(conn, KEY_BACKUP_RUN_TIME, &s.backup.run_time)?;
    s.backup.retention_count =
        read_u32(conn, KEY_BACKUP_RETENTION, s.backup.retention_count)?;
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
    let Some(raw) = raw else { return Ok(Vec::new()) };
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
        KEY_CUSTOM_RULES => {
            let arr = value.as_array().ok_or_else(|| bad("必须是数组"))?;
            let mut rules = Vec::with_capacity(arr.len());
            for (i, item) in arr.iter().enumerate() {
                let obj = item.as_object().ok_or_else(|| bad(&format!("第 {i} 项不是对象")))?;
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
                rules.push(IgnoreRule { kind, pattern, enabled });
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
        _ => Err(AppError::new("unknown_setting", format!("未知设置项: {key}"))),
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
```

### 任务 1.2 新建 `commands/settings.rs`

文件：`src-tauri/src/commands/settings.rs`

```rust
use serde_json::Value;
use tauri::State;

use crate::AppState;
use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::settings_service::{self, AppSettings};

fn lock_db<'a>(state: &'a AppState) -> std::sync::MutexGuard<'a, rusqlite::Connection> {
    state.conn.lock().expect("db lock poisoned")
}

/// 读取完整设置（storage 附带当前目录信息）。
#[tauri::command]
pub fn get_settings(state: State<AppState>) -> CommandResult<AppSettings> {
    let conn = lock_db(&state);
    let mut s = settings_service::load_settings(&conn)?;
    s.storage.data_dir = state.data_dir.to_string_lossy().to_string();
    s.storage.managed_dir = state.managed_dir.to_string_lossy().to_string();
    Ok(s)
}

/// 更新单个设置项，返回最新完整设置。
#[tauri::command]
pub fn update_setting(
    state: State<AppState>,
    key: String,
    value: Value,
) -> CommandResult<AppSettings> {
    let conn = lock_db(&state);
    let mut s = settings_service::update_setting(&conn, &key, value)?;
    s.storage.data_dir = state.data_dir.to_string_lossy().to_string();
    s.storage.managed_dir = state.managed_dir.to_string_lossy().to_string();
    Ok(s)
}

/// 重置某个分类为默认值，返回最新完整设置。
#[tauri::command]
pub fn reset_settings_category(
    state: State<AppState>,
    category: String,
) -> CommandResult<AppSettings> {
    let conn = lock_db(&state);
    let mut s = settings_service::reset_category(&conn, &category)?;
    s.storage.data_dir = state.data_dir.to_string_lossy().to_string();
    s.storage.managed_dir = state.managed_dir.to_string_lossy().to_string();
    Ok(s)
}

#[allow(dead_code)]
fn _lint(_e: &AppError) -> String {
    _e.to_string()
}
```

### 任务 1.3 注册模块与命令

修改 `src-tauri/src/commands/mod.rs`：加入 `pub mod settings;`。

修改 `src-tauri/src/services/mod.rs`：加入 `pub mod settings_service;`。

修改 `src-tauri/src/lib.rs` 的 `invoke_handler` 列表，追加三项（放在 `commands::backups::app_environment` 之后）：

```rust
            commands::settings::get_settings,
            commands::settings::update_setting,
            commands::settings::reset_settings_category,
```

### 任务 1.4 运行后端测试

运行 `cargo test --manifest-path src-tauri/Cargo.toml settings_service`，预期 `settings_service` 模块 8 个测试全部通过。

---

## 阶段 2：前端设置基础、外观与通用设置

### 任务 2.1 新建前端 `settingsStore.ts`

文件：`src/features/settings/stores/settingsStore.ts`

```ts
import { create } from "zustand";
import { call } from "../../../lib/tauri";

export interface IgnoreRule {
  kind: "name" | "path";
  pattern: string;
  enabled: boolean;
}

export interface AppSettings {
  general: {
    launch_behavior: "home" | "files";
    default_import_mode: "managed" | "external";
    duplicate_policy: "skip" | "keep_both";
    preview_size_limit_mb: number;
  };
  appearance: {
    theme_mode: "system" | "light" | "dark";
    density: "comfortable" | "compact";
  };
  ignore: { custom_rules: IgnoreRule[] };
  backup: {
    enabled: boolean;
    frequency: "daily" | "weekly";
    run_time: string;
    retention_count: number;
    backup_type: "metadata" | "full";
  };
  storage: { data_dir: string; managed_dir: string };
}

interface SaveState {
  saving: boolean;
  error: string | null;
  okAt: number | null;
}

interface SettingsState {
  settings: AppSettings | null;
  loading: boolean;
  saveState: SaveState;
  load: () => Promise<void>;
  update: (key: string, value: unknown) => Promise<void>;
  resetCategory: (category: string) => Promise<void>;
  clearError: () => void;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: null,
  loading: false,
  saveState: { saving: false, error: null, okAt: null },

  load: async () => {
    set({ loading: true });
    try {
      const settings = await call<AppSettings>("get_settings", {});
      set({ settings, loading: false });
    } catch (e) {
      set({ loading: false, saveState: { saving: false, error: (e as Error).message, okAt: null } });
    }
  },

  update: async (key, value) => {
    const prev = get().settings;
    if (!prev) return;
    // 乐观更新
    const next = applyPatch(prev, key, value);
    set({ settings: next, saveState: { saving: true, error: null, okAt: null } });
    try {
      const settings = await call<AppSettings>("update_setting", { key, value });
      set({ settings, saveState: { saving: false, error: null, okAt: Date.now() } });
    } catch (e) {
      // 失败回滚
      set({ settings: prev, saveState: { saving: false, error: (e as Error).message, okAt: null } });
    }
  },

  resetCategory: async (category) => {
    const prev = get().settings;
    set({ saveState: { saving: true, error: null, okAt: null } });
    try {
      const settings = await call<AppSettings>("reset_settings_category", { category });
      set({ settings, saveState: { saving: false, error: null, okAt: Date.now() } });
    } catch (e) {
      set({ settings: prev, saveState: { saving: false, error: (e as Error).message, okAt: null } });
    }
  },

  clearError: () => set((s) => ({ saveState: { ...s.saveState, error: null } })),
}));

/** 按 "分类.字段" 键路径打补丁，返回新对象（不修改原对象）。 */
function applyPatch(prev: AppSettings, key: string, value: unknown): AppSettings {
  const [section, field] = key.split(".") as [keyof AppSettings, string];
  const target = prev[section] as Record<string, unknown>;
  return { ...prev, [section]: { ...target, [field]: value } };
}
```

### 任务 2.2 主题应用 `src/lib/theme.ts`

```ts
export type ThemeMode = "system" | "light" | "dark";

/** 将主题模式应用到 <html data-theme>。system 时移除属性，回退到 CSS media query。 */
export function applyTheme(mode: ThemeMode): void {
  const el = document.documentElement;
  if (mode === "system") {
    delete el.dataset.theme;
  } else {
    el.dataset.theme = mode;
  }
}

/** 注册系统主题变化监听，返回取消函数。仅在 system 模式时生效。 */
export function listenSystemTheme(onChange: () => void): () => void {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const handler = () => onChange();
  mq.addEventListener("change", handler);
  return () => mq.removeEventListener("change", handler);
}
```

### 任务 2.3 `tokens.css` 增加 data-theme 作用域

在 `:root` 浅色块之后追加：

```css
/* 手动主题覆盖：data-theme 显式指定时优先于系统偏好 */
:root[data-theme="light"] {
  --bg: #f7f8fa;
  --surface: #ffffff;
  --surface-hover: #f2f4f7;
  --surface-active: #e8ecf3;
  --border: #e2e6ec;
  --border-strong: #c9d1dc;
  --text: #16202e;
  --text-secondary: #5c6b7f;
  --text-tertiary: #8b98a9;
  --text-inverse: #ffffff;
  --primary: #2563eb;
  --primary-hover: #1d4ed8;
  --primary-soft: #eff6ff;
  --primary-border: #bfdbfe;
  --primary-text: #1e3a8a;
  --danger: #dc2626;
  --danger-soft: #fef2f2;
  --warning: #d97706;
  --warning-soft: #fffbeb;
  --success: #16a34a;
  --success-soft: #f0fdf4;
}

:root[data-theme="dark"] {
  --bg: #10151d;
  --surface: #171d27;
  --surface-hover: #1e2632;
  --surface-active: #253042;
  --border: #2a3442;
  --border-strong: #3b4759;
  --text: #e6ebf2;
  --text-secondary: #9aa7b8;
  --text-tertiary: #6b7889;
  --text-inverse: #ffffff;
  --primary: #3b82f6;
  --primary-hover: #60a5fa;
  --primary-soft: #17243f;
  --primary-border: #2b3a5c;
  --primary-text: #bfdbfe;
  --danger: #f87171;
  --danger-soft: #3a1f24;
  --warning: #fbbf24;
  --warning-soft: #3a2f1a;
  --success: #4ade80;
  --success-soft: #17301f;
}
```

`@media (prefers-color-scheme: dark)` 块保持不变，作为 system 模式的回退。

### 任务 2.4 `main.tsx` 启动时应用主题

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./app/App";
import "./styles/tokens.css";
import "./styles/app.css";
import { applyTheme, listenSystemTheme } from "./lib/theme";

async function bootstrap() {
  // 尽力读取主题设置；失败时保持 system 默认。
  try {
    const { get_settings } = await import("@tauri-apps/api/core");
    const { invoke } = await import("@tauri-apps/api/core");
    const settings = await invoke<{
      appearance: { theme_mode: "system" | "light" | "dark" };
    }>("get_settings");
    const mode = settings.appearance.theme_mode;
    applyTheme(mode);
    if (mode === "system") {
      listenSystemTheme(() => {
        // system 模式无需改写 data-theme，移除即可；此处仅确保属性干净
        applyTheme("system");
      });
    }
  } catch {
    applyTheme("system");
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void bootstrap();
```

说明：主题在渲染前应用，避免闪烁。`get_settings` 调用失败不影响应用启动。

### 任务 2.5 前端设置组件与页面重构

新建 `src/features/settings/components/SettingsTabs.tsx`：

```tsx
import { Settings, Palette, HardDrive, Filter, Archive, Info } from "lucide-react";

export type SettingsTab = "general" | "appearance" | "storage" | "ignore" | "backup" | "about";

const TABS: { id: SettingsTab; label: string; icon: React.ReactNode }[] = [
  { id: "general", label: "通用", icon: <Settings size={14} /> },
  { id: "appearance", label: "外观", icon: <Palette size={14} /> },
  { id: "storage", label: "存储", icon: <HardDrive size={14} /> },
  { id: "ignore", label: "忽略规则", icon: <Filter size={14} /> },
  { id: "backup", label: "备份与恢复", icon: <Archive size={14} /> },
  { id: "about", label: "关于", icon: <Info size={14} /> },
];

export function SettingsTabs({
  active,
  onChange,
}: {
  active: SettingsTab;
  onChange: (tab: SettingsTab) => void;
}) {
  return (
    <div className="settings-tabs scrollable">
      {TABS.map((t) => (
        <button
          key={t.id}
          className={`settings-tab-btn${active === t.id ? " active" : ""}`}
          onClick={() => onChange(t.id)}
        >
          {t.icon}
          {t.label}
        </button>
      ))}
    </div>
  );
}
```

新建 `src/features/settings/components/SettingRow.tsx`（通用设置行）：

```tsx
import { CheckCircle2, AlertCircle, Loader2 } from "lucide-react";
import type { SaveState } from "../stores/settingsStore";

export function SaveBadge({ state }: { state: SaveState }) {
  if (state.saving) {
    return (
      <span className="save-badge saving">
        <Loader2 size={12} className="spin" /> 保存中
      </span>
    );
  }
  if (state.error) {
    return (
      <span className="save-badge error">
        <AlertCircle size={12} /> {state.error}
      </span>
    );
  }
  if (state.okAt) {
    return (
      <span className="save-badge ok">
        <CheckCircle2 size={12} /> 已保存
      </span>
    );
  }
  return null;
}

export function SettingRow({
  label,
  desc,
  children,
}: {
  label: string;
  desc?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="setting-row">
      <div className="setting-row-label">
        <span className="setting-row-name">{label}</span>
        {desc && <span className="setting-row-desc">{desc}</span>}
      </div>
      <div className="setting-row-control">{children}</div>
    </div>
  );
}
```

新建 `src/features/settings/components/GeneralSettings.tsx`：

```tsx
import { useSettingsStore } from "../stores/settingsStore";
import { SaveBadge, SettingRow } from "./SettingRow";

export function GeneralSettings() {
  const { settings, saveState, update } = useSettingsStore();
  if (!settings) return null;
  const g = settings.general;

  return (
    <div className="settings-section">
      <h3>通用</h3>
      <SaveBadge state={saveState} />

      <SettingRow label="启动后展示" desc="应用启动后默认进入的页面">
        <select
          className="input"
          value={g.launch_behavior}
          onChange={(e) => update("general.launch_behavior", e.target.value)}
        >
          <option value="home">首页</option>
          <option value="files">文件</option>
        </select>
      </SettingRow>

      <SettingRow label="默认导入方式" desc="导入文件时默认选择的模式">
        <select
          className="input"
          value={g.default_import_mode}
          onChange={(e) => update("general.default_import_mode", e.target.value)}
        >
          <option value="managed">复制到仓库</option>
          <option value="external">保留原位置</option>
        </select>
      </SettingRow>

      <SettingRow label="重复文件策略" desc="导入路径与已有文件相同时的处理方式">
        <select
          className="input"
          value={g.duplicate_policy}
          onChange={(e) => update("general.duplicate_policy", e.target.value)}
        >
          <option value="skip">跳过重复文件</option>
          <option value="keep_both">保留两者</option>
        </select>
      </SettingRow>

      <SettingRow label="文本预览上限" desc="文本类文件预览的最大体积（MB）">
        <input
          className="input"
          type="number"
          min={1}
          max={1024}
          value={g.preview_size_limit_mb}
          onChange={(e) => {
            const n = Number(e.target.value);
            if (Number.isFinite(n) && n >= 1 && n <= 1024) {
              update("general.preview_size_limit_mb", n);
            }
          }}
        />
      </SettingRow>

      <div className="settings-actions">
        <button
          className="btn"
          onClick={() => useSettingsStore.getState().resetCategory("general")}
        >
          恢复默认
        </button>
      </div>
    </div>
  );
}
```

新建 `src/features/settings/components/AppearanceSettings.tsx`：

```tsx
import { useSettingsStore } from "../stores/settingsStore";
import { applyTheme } from "../../../lib/theme";
import { SaveBadge, SettingRow } from "./SettingRow";

export function AppearanceSettings() {
  const { settings, saveState, update } = useSettingsStore();
  if (!settings) return null;
  const a = settings.appearance;

  const setTheme = (mode: "system" | "light" | "dark") => {
    applyTheme(mode); // 立即生效
    update("appearance.theme_mode", mode);
  };

  return (
    <div className="settings-section">
      <h3>外观</h3>
      <SaveBadge state={saveState} />

      <SettingRow label="主题模式" desc="选择界面配色">
        <div className="seg">
          <button
            className={a.theme_mode === "system" ? "active" : ""}
            onClick={() => setTheme("system")}
          >
            跟随系统
          </button>
          <button
            className={a.theme_mode === "light" ? "active" : ""}
            onClick={() => setTheme("light")}
          >
            浅色
          </button>
          <button
            className={a.theme_mode === "dark" ? "active" : ""}
            onClick={() => setTheme("dark")}
          >
            深色
          </button>
        </div>
      </SettingRow>

      <SettingRow label="界面紧凑度" desc="列表与工具栏的间距密度">
        <select
          className="input"
          value={a.density}
          onChange={(e) => update("appearance.density", e.target.value)}
        >
          <option value="comfortable">舒适</option>
          <option value="compact">紧凑</option>
        </select>
      </SettingRow>

      <div className="settings-actions">
        <button
          className="btn"
          onClick={() => {
            const defaults = { theme_mode: "system", density: "comfortable" } as const;
            applyTheme(defaults.theme_mode);
            useSettingsStore.getState().resetCategory("appearance");
          }}
        >
          恢复默认
        </button>
      </div>
    </div>
  );
}
```

新建 `src/features/settings/components/StorageSettings.tsx`（阶段 1 只读展示；目录迁移见计划 3）：

```tsx
import { useSettingsStore } from "../stores/settingsStore";

export function StorageSettings() {
  const { settings } = useSettingsStore();
  if (!settings) return null;
  const s = settings.storage;
  return (
    <div className="settings-section">
      <h3>存储</h3>
      <div className="settings-rows">
        <div className="settings-row">
          <span>数据目录</span>
          <span className="mono">{s.data_dir || "-"}</span>
        </div>
        <div className="settings-row">
          <span>托管文件目录</span>
          <span className="mono">{s.managed_dir || "-"}</span>
        </div>
      </div>
      <p className="settings-note">
        数据目录与托管目录的调整需要迁移现有数据，将在后续版本中提供。
      </p>
    </div>
  );
}
```

新建 `src/features/settings/components/AboutSettings.tsx`（保留现有 app_environment 信息）：

```tsx
import { useEffect, useState } from "react";
import { Database, HardDrive, Package } from "lucide-react";
import { call } from "../../../lib/tauri";

interface Env {
  name: string;
  version: string;
  data_dir: string;
  managed_dir: string;
  db_path: string;
}

export function AboutSettings() {
  const [env, setEnv] = useState<Env | null>(null);
  useEffect(() => {
    call<Env>("app_environment", {}).then(setEnv).catch(() => {});
  }, []);
  return (
    <div className="settings-section">
      <h3>关于</h3>
      {env && (
        <div className="settings-rows">
          <div className="settings-row">
            <span>
              <Package size={13} /> 名称
            </span>
            <span>{env.name}</span>
          </div>
          <div className="settings-row">
            <span>版本</span>
            <span>{env.version}</span>
          </div>
          <div className="settings-row">
            <span>
              <HardDrive size={13} /> 数据目录
            </span>
            <span className="mono">{env.data_dir}</span>
          </div>
          <div className="settings-row">
            <span>
              <HardDrive size={13} /> 托管文件目录
            </span>
            <span className="mono">{env.managed_dir}</span>
          </div>
          <div className="settings-row">
            <span>
              <Database size={13} /> 数据库
            </span>
            <span className="mono">{env.db_path}</span>
          </div>
        </div>
      )}
    </div>
  );
}
```

新建 `src/features/settings/components/BackupSettings.tsx`：把现有 `SettingsPage` 的备份与恢复逻辑整体搬入（含 `create_backup` / `list_backups` / `restore_backup` 状态），后续计划 2 再增强。

重构 `src/features/settings/routes/SettingsPage.tsx`：

```tsx
import { useEffect, useState } from "react";
import { useSettingsStore } from "../stores/settingsStore";
import { SettingsTabs, type SettingsTab } from "../components/SettingsTabs";
import { GeneralSettings } from "../components/GeneralSettings";
import { AppearanceSettings } from "../components/AppearanceSettings";
import { StorageSettings } from "../components/StorageSettings";
import { BackupSettings } from "../components/BackupSettings";
import { AboutSettings } from "../components/AboutSettings";

export function SettingsPage() {
  const [tab, setTab] = useState<SettingsTab>("general");
  const { load, loading, saveState } = useSettingsStore();

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="settings-page">
      <div className="settings-head">
        <h2>设置</h2>
        {saveState.error && (
          <div className="settings-error" role="alert">
            {saveState.error}
          </div>
        )}
      </div>
      <SettingsTabs active={tab} onChange={setTab} />
      {loading ? (
        <div className="empty-state">加载设置中…</div>
      ) : (
        <div className="settings-body">
          {tab === "general" && <GeneralSettings />}
          {tab === "appearance" && <AppearanceSettings />}
          {tab === "storage" && <StorageSettings />}
          {tab === "backup" && <BackupSettings />}
          {tab === "about" && <AboutSettings />}
        </div>
      )}
    </div>
  );
}
```

说明：`tab === "ignore"` 的内容在阶段 3 加入；此处先不渲染，避免空标签。

### 任务 2.6 设置页样式

在 `src/styles/app.css` 的「设置页」段落追加：

```css
.settings-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.settings-head h2 {
  margin: 0 0 16px;
}

.settings-error {
  background: var(--danger-soft);
  color: var(--danger);
  border: 1px solid var(--border);
  border-radius: var(--radius-m);
  padding: 8px 14px;
  font-size: 13px;
  max-width: 480px;
}

.settings-tabs {
  display: flex;
  gap: 2px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--border);
  overflow-x: auto;
}

.settings-tab-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  font-size: 13px;
  color: var(--text-secondary);
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  white-space: nowrap;
  flex-shrink: 0;
}

.settings-tab-btn:hover {
  color: var(--text);
  background: var(--surface-hover);
  border-radius: var(--radius-m) var(--radius-m) 0 0;
}

.settings-tab-btn.active {
  color: var(--primary);
  border-bottom-color: var(--primary);
  font-weight: 550;
}

.settings-body {
  max-width: 760px;
}

.settings-section h3 {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 0 14px;
  font-size: 14px;
  font-weight: 600;
}

.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 10px 0;
  border-bottom: 1px solid var(--border);
}

.setting-row:last-of-type {
  border-bottom: none;
}

.setting-row-label {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.setting-row-name {
  font-size: 13.5px;
  font-weight: 500;
}

.setting-row-desc {
  font-size: 12px;
  color: var(--text-tertiary);
}

.setting-row-control {
  flex: 0 0 auto;
}

.save-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12px;
  margin-bottom: 10px;
}

.save-badge.saving {
  color: var(--text-secondary);
}

.save-badge.ok {
  color: var(--success);
}

.save-badge.error {
  color: var(--danger);
}

.settings-section {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-l);
  padding: 16px 20px;
  margin-bottom: 16px;
}
```

### 任务 2.7 前端测试基础设施与单元测试

`package.json` 增加脚本与依赖（devDependencies 增加 `vitest`、`jsdom`；scripts 增加 `"test": "vitest run"`）：

```json
"scripts": {
  "dev": "vite",
  "build": "tsc && vite build",
  "preview": "vite preview",
  "tauri": "tauri",
  "test": "vitest run"
}
```

运行 `npm install` 安装 vitest 与 jsdom（网络可用时执行；若失败则跳过测试任务并记录）。

新建 `vitest.config.ts`：

```ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
```

新建 `src/lib/theme.test.ts`：

```ts
import { describe, expect, it, afterEach } from "vitest";
import { applyTheme } from "./theme";

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

describe("applyTheme", () => {
  it("sets data-theme for explicit modes", () => {
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    applyTheme("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("removes data-theme for system mode", () => {
    applyTheme("dark");
    applyTheme("system");
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});
```

新建 `src/features/settings/stores/settingsStore.test.ts`（不依赖真实 IPC；仅验证 applyPatch 行为不可行——applyPatch 未导出，因此测试改经 store 的乐观更新路径：由于 `call` 依赖 Tauri，本测试只验证 `resetCategory` 之外的纯函数；为可测性，在 `settingsStore.ts` 末尾导出 `applyPatch`）：

在 `settingsStore.ts` 中把 `function applyPatch` 改为 `export function applyPatch`。

```ts
import { describe, expect, it } from "vitest";
import { applyPatch, type AppSettings } from "./settingsStore";

const base: AppSettings = {
  general: {
    launch_behavior: "home",
    default_import_mode: "managed",
    duplicate_policy: "skip",
    preview_size_limit_mb: 256,
  },
  appearance: { theme_mode: "system", density: "comfortable" },
  ignore: { custom_rules: [] },
  backup: {
    enabled: false,
    frequency: "daily",
    run_time: "02:00",
    retention_count: 7,
    backup_type: "full",
  },
  storage: { data_dir: "", managed_dir: "" },
};

describe("applyPatch", () => {
  it("patches nested field without mutating input", () => {
    const next = applyPatch(base, "appearance.theme_mode", "dark");
    expect(next.appearance.theme_mode).toBe("dark");
    expect(base.appearance.theme_mode).toBe("system");
    expect(next).not.toBe(base);
    expect(next.appearance).not.toBe(base.appearance);
  });

  it("patches number fields", () => {
    const next = applyPatch(base, "general.preview_size_limit_mb", 512);
    expect(next.general.preview_size_limit_mb).toBe(512);
  });
});
```

运行 `npm test`，预期 theme 与 applyPatch 测试通过；随后运行 `npm run build` 验证 TypeScript 编译。

---

## 阶段 3：忽略规则与既有服务接入

### 任务 3.1 `import_service.rs` 忽略规则接入

修改 `src-tauri/src/services/import_service.rs`：

- 新增公开函数 `is_ignored(path: &Path, rules: &[settings_service::IgnoreRule]) -> bool`，内置目录列表与自定义 name/path 规则合并判断：

```rust
/// 判断路径是否应被忽略：内置目录 + 自定义规则（仅 enabled）。
pub fn is_ignored(path: &Path, rules: &[settings_service::IgnoreRule]) -> bool {
    if should_skip(path) {
        return true;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let path_lower = path.to_string_lossy().to_lowercase();
    for rule in rules {
        if !rule.enabled {
            continue;
        }
        let pat = rule.pattern.to_lowercase();
        let matched = match rule.kind.as_str() {
            "name" => name == pat || name.starts_with(&pat.trim_end_matches('*')),
            _ => path_lower.contains(&pat),
        };
        if matched {
            return true;
        }
    }
    false
}
```

- 在 `collect_tree` 的目录分支中，把 `should_skip(&p)` 替换为基于当前设置规则的判断。为最小侵入，新增导入参数 `ignore_rules: Vec<IgnoreRule>`，由 `run_import` 从设置读取后传入 `collect_imports` → `collect_tree`。修改函数签名：

```rust
fn collect_imports(
    paths: &[String],
    mode: SourceType,
    parent_id: Option<String>,
    managed_root: &Path,
    ignore_rules: &[settings_service::IgnoreRule],
    out: &mut Vec<PendingImport>,
    failures: &mut Vec<(PathBuf, String)>,
) { ... }

fn collect_tree(
    node: &Path,
    dest_parent: &Path,
    mode: SourceType,
    parent_resource_id: Option<String>,
    ignore_rules: &[settings_service::IgnoreRule],
    out: &mut Vec<PendingImport>,
    failures: &mut Vec<(PathBuf, String)>,
) {
    ...
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if is_ignored(&p, ignore_rules) {
                continue;
            }
            collect_tree(&p, &dest_dir, mode, Some(dir_id.clone()), ignore_rules, out, failures);
        } else if p.is_file() {
            if is_ignored(&p, ignore_rules) {
                continue;
            }
            match build_file(&p, &dest_dir, mode, Some(dir_id.clone())) { ... }
        }
    }
}
```

- `run_import` 开头读取设置：

```rust
let ignore_rules = {
    let conn = state.conn.lock().expect("db lock");
    settings_service::load_settings(&conn)
        .map(|s| s.ignore.custom_rules)
        .unwrap_or_default()
};
```

并把 `collect_imports(..., &ignore_rules, ...)` 传入。

- 既有测试中调用 `collect_imports` / `collect_tree` 的位置，追加 `&[]` 参数。原 `should_skip` 保持私有并由 `is_ignored` 调用。

### 任务 3.2 重复策略接入

在 `import_service.rs` 的 `flush_batch` 增加参数 `allow_duplicates: bool`；`run_import` 中按 `duplicate_policy == "keep_both"` 决定取值。修改 existing 检查分支：

```rust
fn flush_batch(
    conn: &mut rusqlite::Connection,
    batch: &[PendingImport],
    allow_duplicates: bool,
) -> Result<(), AppError> {
    ...
    if !allow_duplicates {
        if let Some((existing_id, res_exists)) = existing { ... 原逻辑 ... }
    } else {
        // keep_both：external 模式允许同一路径再次导入；managed 模式文件名唯一，天然共存
    }
    ...
}
```

同步更新 `flush_batch` 全部调用点（`run_import` 内 2 处 + 测试内调用），测试追加参数 `false`。

### 任务 3.3 预览上限接入

修改 `src-tauri/src/services/preview_service.rs`：

```rust
/// 读取文件前 N 字节并尝试解析为 UTF-8 文本预览。
/// limit_kb 为上限（KB），超过时在结尾追加截断提示。
pub fn read_text_preview(path: &Path, limit_kb: usize) -> Result<String, AppError> {
    let max_bytes = limit_kb.saturating_mul(1024).max(1024);
    let meta = std::fs::metadata(path)?;
    let read_len = (meta.len() as usize).min(max_bytes);
    ...
    if meta.len() as usize > max_bytes {
        Ok(format!("{text}\n\n… 内容过长，仅显示前 {} KB", limit_kb))
    } else {
        Ok(text)
    }
}
```

原常量 `PREVIEW_MAX_BYTES` 删除。测试调用改为 `read_text_preview(&tmp, 256)`。

修改 `src-tauri/src/commands/previews.rs` 的 `get_text_preview`：

```rust
#[tauri::command]
pub fn get_text_preview(
    state: State<AppState>,
    resource_id: String,
) -> CommandResult<String> {
    let conn = lock_db(&state);
    let locations = repo::list_locations(&conn, &resource_id)?;
    let Some(loc) = locations.first() else {
        return Err(AppError::new("location_missing", "资源缺少物理位置"));
    };
    let path = PathBuf::from(&loc.path);
    if !path.exists() {
        return Err(AppError::new("path_missing", "文件路径不可用"));
    }
    let limit_kb = settings_service::load_settings(&conn)
        .map(|s| s.general.preview_size_limit_mb.saturating_mul(1024) as usize)
        .unwrap_or(256 * 1024);
    preview_service::read_text_preview(&path, limit_kb)
}
```

### 任务 3.4 忽略规则前端组件

新建 `src/features/settings/components/IgnoreRulesSettings.tsx`：

```tsx
import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { useSettingsStore, type IgnoreRule } from "../stores/settingsStore";
import { SaveBadge, SettingRow } from "./SettingRow";

const BUILTIN_RULES = [
  "node_modules",
  ".git",
  "target",
  "dist",
  ".cache",
  "__pycache__",
  ".venv",
  "venv",
  ".idea",
  ".vscode",
  ".next",
  "build",
];

export function IgnoreRulesSettings() {
  const { settings, saveState, update } = useSettingsStore();
  const [pattern, setPattern] = useState("");
  const [kind, setKind] = useState<"name" | "path">("name");

  if (!settings) return null;
  const rules = settings.ignore.custom_rules;

  const saveRules = (next: IgnoreRule[]) => {
    update("ignore.custom_rules", next);
  };

  const addRule = () => {
    const trimmed = pattern.trim();
    if (!trimmed) return;
    saveRules([...rules, { kind, pattern: trimmed, enabled: true }]);
    setPattern("");
  };

  const removeRule = (index: number) => {
    saveRules(rules.filter((_, i) => i !== index));
  };

  const toggleRule = (index: number) => {
    saveRules(
      rules.map((r, i) => (i === index ? { ...r, enabled: !r.enabled } : r)),
    );
  };

  return (
    <div className="settings-section">
      <h3>忽略规则</h3>
      <SaveBadge state={saveState} />

      <SettingRow label="内置规则" desc="导入时始终跳过的常见构建与依赖目录">
        <div className="ignore-chips">
          {BUILTIN_RULES.map((r) => (
            <span key={r} className="ignore-chip">
              {r}
            </span>
          ))}
        </div>
      </SettingRow>

      <SettingRow label="自定义规则" desc="name 匹配文件名/目录名，path 匹配路径片段">
        <div className="ignore-add">
          <select
            className="input"
            value={kind}
            onChange={(e) => setKind(e.target.value as "name" | "path")}
          >
            <option value="name">名称</option>
            <option value="path">路径</option>
          </select>
          <input
            className="input"
            placeholder="例如 *.tmp 或 node_modules"
            value={pattern}
            onChange={(e) => setPattern(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addRule()}
          />
          <button className="btn btn-primary" onClick={addRule} disabled={!pattern.trim()}>
            <Plus size={14} /> 添加
          </button>
        </div>
      </SettingRow>

      {rules.length > 0 && (
        <div className="ignore-list">
          {rules.map((r, i) => (
            <div key={i} className="ignore-item">
              <label className="ignore-check">
                <input
                  type="checkbox"
                  checked={r.enabled}
                  onChange={() => toggleRule(i)}
                />
                <span className={`ignore-kind ${r.kind}`}>
                  {r.kind === "name" ? "名称" : "路径"}
                </span>
                <span className="ignore-pattern">{r.pattern}</span>
              </label>
              <button className="icon-btn" title="删除规则" onClick={() => removeRule(i)}>
                <Trash2 size={14} />
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="settings-actions">
        <button
          className="btn"
          onClick={() => useSettingsStore.getState().resetCategory("ignore")}
        >
          清空自定义规则
        </button>
      </div>
    </div>
  );
}
```

在 `SettingsPage.tsx` 中引入并渲染：

```tsx
import { IgnoreRulesSettings } from "../components/IgnoreRulesSettings";
...
{tab === "ignore" && <IgnoreRulesSettings />}
```

### 任务 3.5 忽略规则与默认导入模式前端接入

修改 `src/features/files/stores/fileStore.ts`：找到导入模式选择相关逻辑（拖拽导入条），把初始默认模式改为读取 `useSettingsStore` 的 `general.default_import_mode`。具体做法：在导入条组件（`ImportDropzone` 或等价组件，位于 `src/features/files/components/`）中：

```tsx
const defaultMode = useSettingsStore((s) => s.settings?.general.default_import_mode ?? "managed");
const [mode, setMode] = useState<"managed" | "external">(defaultMode);
// 导入完成后，若用户明确选择过，则保留；未选择时跟随设置变化：
useEffect(() => {
  setMode(defaultMode);
}, [defaultMode]);
```

（实现时以实际组件位置为准，文件页导入条是 `import-modes` 模态；改动目标是「导入条初始选中值 = 设置默认值」。）

### 任务 3.6 忽略规则样式

在 `app.css` 追加：

```css
.ignore-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  max-width: 380px;
  justify-content: flex-end;
}

.ignore-chip {
  font-family: var(--font-mono);
  font-size: 11.5px;
  padding: 2px 8px;
  border-radius: var(--radius-full);
  background: var(--surface-hover);
  border: 1px solid var(--border);
  color: var(--text-secondary);
}

.ignore-add {
  display: flex;
  gap: 6px;
  align-items: center;
}

.ignore-list {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.ignore-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 10px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius-m);
}

.ignore-check {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  cursor: pointer;
}

.ignore-check input {
  accent-color: var(--primary);
  flex: 0 0 auto;
}

.ignore-kind {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: var(--radius-full);
  background: var(--primary-soft);
  color: var(--primary-text);
  flex: 0 0 auto;
}

.ignore-pattern {
  font-family: var(--font-mono);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
```

### 任务 3.7 全量验证

1. `cargo test --manifest-path src-tauri/Cargo.toml`：全部 Rust 测试通过（含 import_service 修改后的既有测试）。
2. `npm test`：前端单元测试通过。
3. `npm run build`：TypeScript 编译通过。
4. 手动验证（`npm run tauri dev` 可用时）：
   - 设置页六个标签可切换，窄窗口下标签可横向滚动。
   - 主题切换立即生效，重启后保持；system 模式跟随系统。
   - 修改通用设置后显示「已保存」，重启后值保留。
   - 导入一个包含 `node_modules` 的目录，忽略规则生效；自定义 name 规则 `*.tmp` 生效。
   - 设置失败场景（如数据库只读）界面回滚原值并显示错误。

## 交付物核对

- 后端：`settings_service.rs`、`commands/settings.rs` 及注册；`import_service.rs` 忽略规则与重复策略；`preview_service.rs` 参数化；对应测试。
- 前端：`settingsStore.ts`、`theme.ts`、六个标签组件、`SettingsPage` 重构、样式；`main.tsx` 主题启动；文件页导入默认模式接入；单元测试与 vitest 配置。
- 本计划完成后，下一计划为 `2026-08-16-settings-backup.md`（备份管理完善与自动备份），再后为 `2026-08-16-settings-migration.md`（目录迁移）。
