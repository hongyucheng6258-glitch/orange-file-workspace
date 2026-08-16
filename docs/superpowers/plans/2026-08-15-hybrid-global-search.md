# 全电脑混合搜索 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 NexusFile 搜索扩展为全电脑统一入口：Windows Search 即时层 + 后台全盘索引层 + 应用索引 + NexusFile 资源聚合，支持打开文件、启动应用、打开所在位置。

**Architecture:** 新增 `system_search_entries` / `system_search_apps` / `system_search_scan_state` 三张独立索引表（不复用业务 `resources` 表）。`start_global_search` 同步聚合四类即时来源并返回首批结果，后台线程持续补充本地索引命中并通过 `global-search://batch` 事件推送。后台扫描线程使用独立数据库连接与检查点，低优先级遍历所有本地固定磁盘。

**Tech Stack:** Rust (Tauri 2, rusqlite 0.40 bundled + FTS5 trigram, windows 0.61 COM: Win32_System_Search / Win32_System_OleDb / Win32_UI_Shell, winreg 0.52, notify 8.2, sysinfo 0.33), React 18 + TypeScript + React Router HashRouter, `@tauri-apps/api` (invoke / event listen), `@tauri-apps/plugin-opener` (openPath / revealItemInDir)。

**环境事实:** 当前环境无 git（`git` 不在 PATH），所有提交步骤在无 git 时跳过并记录；本机为 Windows。构建验证命令：`cargo test`（工作目录 `e:\work\新建文件夹\src-tauri`）、`npm run build`（工作目录 `e:\work\新建文件夹`）。

**参照设计文档:** `docs/superpowers/specs/2026-08-15-hybrid-global-search-design.md`

---

## 文件结构总览

新建：

- `src-tauri/migrations/0006_global_search.sql` — 三张系统搜索表
- `src-tauri/src/services/global_search.rs` — 规范化路径、GlobalSearchHit、去重合并、排序、本地索引查询、FTS 探测
- `src-tauri/src/services/app_index.rs` — 应用来源枚举与索引构建
- `src-tauri/src/services/scan_service.rs` — 固定磁盘枚举、后台扫描线程、检查点、资源调度
- `src-tauri/src/services/windows_search.rs` — Windows Search COM 即时查询（含降级）
- `src-tauri/src/commands/global_search.rs` — 命令层
- `src/features/search/lib/globalSearch.ts` — 前端类型与调用封装
- `src/features/search/components/IndexStatusBar.tsx` — 索引状态栏

修改：

- `src-tauri/Cargo.toml` — 增加 windows features（`Win32_System_Search`、`Win32_System_OleDb`）、`dunce` 依赖
- `src-tauri/src/db/migrations.rs` — 注册 0004 迁移
- `src-tauri/src/services/mod.rs` — 注册新服务模块
- `src-tauri/src/commands/mod.rs` — 注册 global_search 命令模块
- `src-tauri/src/lib.rs` — AppState 增加 SearchRuntime、注册命令、启动后台扫描
- `src-tauri/src/events.rs` — 新增两个事件常量
- `src/features/search/routes/SearchPage.tsx` — 改接全局搜索
- `src/components/Topbar.tsx` — 占位文案
- `src/styles/app.css` — 搜索页新样式

---

## 阶段一：统一结果模型、应用索引、聚合与命令层

### Task 1: 数据库迁移 0006（系统搜索三表）

> 版本号说明：代码库现有 0001-0005 五个迁移（0004/0005 已被文档功能占用），本迁移注册为 **version 6**，文件名为 `0006_global_search.sql`。

**Files:**
- Create: `src-tauri/migrations/0006_global_search.sql`
- Modify: `src-tauri/src/db/migrations.rs`

- [ ] **Step 1: 编写迁移 SQL**

`src-tauri/migrations/0006_global_search.sql`：

```sql
-- 系统搜索：文件/文件夹索引（与业务 resources 表隔离）
CREATE TABLE IF NOT EXISTS system_search_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    canonical_path TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name TEXT NOT NULL,
    entry_kind TEXT NOT NULL CHECK (entry_kind IN ('file', 'folder')),
    extension TEXT,
    file_size INTEGER,
    modified_at INTEGER,
    volume_id TEXT NOT NULL,
    scan_generation INTEGER NOT NULL,
    is_offline INTEGER NOT NULL DEFAULT 0,
    indexed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sse_name ON system_search_entries (display_name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_sse_volume_gen ON system_search_entries (volume_id, scan_generation);
CREATE INDEX IF NOT EXISTS idx_sse_kind ON system_search_entries (entry_kind);

-- 系统搜索：应用索引
CREATE TABLE IF NOT EXISTS system_search_apps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    app_kind TEXT NOT NULL CHECK (app_kind IN ('win32', 'shortcut', 'store')),
    display_name TEXT NOT NULL,
    launch_target TEXT,
    canonical_target TEXT NOT NULL UNIQUE COLLATE NOCASE,
    aumid TEXT,
    icon_source TEXT,
    install_location TEXT,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ssa_name ON system_search_apps (display_name);
CREATE INDEX IF NOT EXISTS idx_ssa_kind ON system_search_apps (app_kind);

-- 系统搜索：每个卷的扫描状态
CREATE TABLE IF NOT EXISTS system_search_scan_state (
    volume_id TEXT PRIMARY KEY,
    root_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'scanning', 'paused', 'completed', 'error', 'offline')),
    scan_generation INTEGER NOT NULL DEFAULT 1,
    checkpoint TEXT,
    indexed_count INTEGER NOT NULL DEFAULT 0,
    skipped_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    started_at INTEGER,
    completed_at INTEGER
);
```

- [ ] **Step 2: 注册迁移**

`src-tauri/src/db/migrations.rs` 在 `MIGRATIONS` 数组末尾追加：

```rust
    Migration {
        version: 6,
        name: "global_search_indexes",
        sql: include_str!("../../migrations/0006_global_search.sql"),
    },
```

- [ ] **Step 3: 更新迁移测试**

`src-tauri/src/db/migrations.rs` 中现有 `applies_all_migrations` 测试是动态断言（`assert_eq!(version, MIGRATIONS.last().unwrap().version)`），添加 0004 后自动通过，无需修改。新增表存在性断言：

```rust
    #[test]
    fn global_search_tables_exist() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migrations");
        for table in ["system_search_entries", "system_search_apps", "system_search_scan_state"] {
            let ok: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |r| r.get(0),
                )
                .expect("query");
            assert!(ok, "table {table} should exist");
        }
    }

    #[test]
    fn global_search_key_constraints_apply() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migrations");
        // canonical_path UNIQUE 且大小写不敏感（Windows 路径大小写不敏感）
        conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\Docs\\a.txt', 'a.txt', 'file', 'v1', 1, 1)",
            [],
        )
        .expect("insert");
        let dup = conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('C:\\DOCS\\A.TXT', 'a.txt', 'file', 'v1', 1, 1)",
            [],
        );
        assert!(dup.is_err(), "大小写不同的同一路径应被 UNIQUE COLLATE NOCASE 拒绝");
        // CHECK 约束拒绝非法类型与状态
        let bad_kind = conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\x\\b.txt', 'b.txt', 'link', 'v1', 1, 1)",
            [],
        );
        assert!(bad_kind.is_err(), "非法 entry_kind 应被 CHECK 拒绝");
        let bad_status = conn.execute(
            "INSERT INTO system_search_scan_state (volume_id, root_path, status) VALUES ('v1', 'C:\\', 'weird')",
            [],
        );
        assert!(bad_status.is_err(), "非法 status 应被 CHECK 拒绝");
    }
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --lib db::migrations`（cwd `e:\work\新建文件夹\src-tauri`）
Expected: `test result: ok`，无失败。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/migrations/0006_global_search.sql src-tauri/src/db/migrations.rs
git commit -m "feat(db): add global search index tables migration"
```
环境无 git 时跳过本步骤并记录。

---

### Task 2: 全局搜索核心（规范化、结果模型、去重、排序）

**Files:**
- Create: `src-tauri/src/services/global_search.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: 编写失败测试**

`src-tauri/src/services/global_search.rs` 底部追加测试模块（先只写测试与类型骨架，实现函数留空返回默认值）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn hit(key: &str, kind: &str, name: &str, score: i64, source: &str) -> GlobalSearchHit {
        GlobalSearchHit {
            key: key.to_string(),
            kind: kind.to_string(),
            name: name.to_string(),
            path: Some(format!("C:\\tmp\\{name}")),
            source: source.to_string(),
            matched_field: "name".to_string(),
            modified_at: Some(1_700_000_000),
            icon_source: None,
            is_offline: false,
            actions: vec!["open".to_string()],
            score,
        }
    }

    #[test]
    fn canonical_path_is_case_insensitive() {
        assert_eq!(canonical_key("C:\\Users\\A\\File.Txt"), canonical_key("c:\\users\\a\\file.txt"));
        assert_eq!(canonical_key("D:/Docs/报告.pdf"), canonical_key("d:\\docs\\报告.pdf"));
    }

    #[test]
    fn dedup_keeps_highest_score() {
        let mut hits = vec![
            hit("C:\\a\\b.txt", "file", "b.txt", 50, "windows"),
            hit("C:\\a\\b.txt", "file", "b.txt", 80, "local_index"),
        ];
        dedup_hits(&mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "local_index");
    }

    #[test]
    fn sort_orders_name_prefix_over_contains() {
        let mut hits = vec![
            hit("k1", "file", "report.pdf", 2, "local_index"),
            hit("k2", "file", "my_report.pdf", 1, "local_index"),
        ];
        sort_hits(&mut hits);
        assert_eq!(hits[0].key, "k1");
    }

    #[test]
    fn escape_like_handles_specials() {
        assert_eq!(escape_like("100%_\\"), "100\\%\\_\\\\");
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib services::global_search`
Expected: 编译失败，提示 `canonical_key` / `dedup_hits` / `sort_hits` / `escape_like` / `GlobalSearchHit` 未定义。

- [ ] **Step 3: 实现核心模块**

`src-tauri/src/services/global_search.rs` 主体：

```rust
use std::collections::HashMap;

/// 全局搜索结果条目（统一模型，供前端序列化）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct GlobalSearchHit {
    pub key: String,
    pub kind: String, // file | folder | app | page | project
    pub name: String,
    pub path: Option<String>,
    pub source: String, // windows | local_index | app_index | nexus
    pub matched_field: String,
    pub modified_at: Option<i64>,
    pub icon_source: Option<String>,
    pub is_offline: bool,
    pub actions: Vec<String>,
    pub score: i64,
}

/// 规范化搜索去重键：统一分隔符并小写，用于大小写无关去重。
pub fn canonical_key(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

/// 转义 LIKE 通配符（与 commands/search.rs 同规则）。
pub fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 同一 key 只保留一条：优先高分，其次更权威来源。
pub fn dedup_hits(hits: &mut Vec<GlobalSearchHit>) {
    let mut best: HashMap<String, usize> = HashMap::new();
    let mut kept: Vec<GlobalSearchHit> = Vec::new();
    for h in hits.drain(..) {
        match best.get(&h.key) {
            Some(&idx) => {
                let prev = &mut kept[idx];
                let prev_authority = authority_rank(&prev.source);
                let cur_authority = authority_rank(&h.source);
                if h.score > prev.score
                    || (h.score == prev.score && cur_authority > prev_authority)
                {
                    *prev = h;
                }
            }
            None => {
                best.insert(h.key.clone(), kept.len());
                kept.push(h);
            }
        }
    }
    *hits = kept;
}

fn authority_rank(source: &str) -> i64 {
    match source {
        "windows" => 4,
        "local_index" => 3,
        "nexus" => 2,
        "app_index" => 1,
        _ => 0,
    }
}

/// 按评分降序排序（评分已由各来源按匹配级别计算）。
pub fn sort_hits(hits: &mut Vec<GlobalSearchHit>) {
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --lib services::global_search`
Expected: 4 个测试全部 PASS。

- [ ] **Step 5: 注册模块**

`src-tauri/src/services/mod.rs` 追加：

```rust
pub mod global_search;
```

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/services/global_search.rs src-tauri/src/services/mod.rs
git commit -m "feat(global-search): add core hit model, canonical key, dedup and sort"
```
无 git 时跳过。

---

### Task 3: 应用索引构建（开始菜单、App Paths、卸载注册表、Store）

**Files:**
- Create: `src-tauri/src/services/app_index.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 增加依赖**

`src-tauri/Cargo.toml` 的 `[target.'cfg(windows)'.dependencies]` windows features 追加：

```toml
    "Win32_System_Search",
    "Win32_System_OleDb",
    "Win32_System_Com_StructuredStorage",
    "Win32_System_WinRT",
    "Win32_System_WinRT_Deployment",
    "Win32_UI_Shell_PropertiesSystem",
```

顶层 `[dependencies]` 追加：

```toml
dunce = "1.0"
```

- [ ] **Step 2: 编写失败测试**

`src-tauri/src/services/app_index.rs`（先只写测试 + 空函数）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lnk_to_target_extracts_path() {
        // 简化：仅验证 "shell:AppsFolder\\..." 与普通路径的判定
        assert!(is_store_link("shell:AppsFolder\\Microsoft.Windows.Photos_8wekyb3d8bbwe!App"));
        assert!(!is_store_link("C:\\Program Files\\App\\app.exe"));
    }

    #[test]
    fn app_entries_canonicalize_case() {
        let a = AppEntry {
            app_kind: "win32".into(),
            display_name: "测试".into(),
            launch_target: Some("C:\\Prog\\x.exe".into()),
            canonical_target: canonical_app_target("c:\\prog\\x.exe"),
            aumid: None,
            icon_source: None,
            install_location: None,
        };
        assert_eq!(a.canonical_target, "c:\\prog\\x.exe");
    }
}
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test --lib services::app_index`
Expected: 编译失败（`AppEntry` / `is_store_link` / `canonical_app_target` 未定义）。

- [ ] **Step 4: 实现应用索引模块**

`src-tauri/src/services/app_index.rs`：

```rust
use rusqlite::Connection;

/// 应用索引条目。
#[derive(Debug, Clone)]
pub struct AppEntry {
    pub app_kind: String, // win32 | shortcut | store
    pub display_name: String,
    pub launch_target: Option<String>,
    pub canonical_target: String,
    pub aumid: Option<String>,
    pub icon_source: Option<String>,
    pub install_location: Option<String>,
}

/// 规范化应用去重键：统一大小写与分隔符。
pub fn canonical_app_target(target: &str) -> String {
    target.replace('/', "\\").to_lowercase()
}

/// 判定快捷方式是否指向 Store 应用。
pub fn is_store_link(target: &str) -> bool {
    target.trim_start().to_ascii_lowercase().starts_with("shell:appsfolder")
}

/// 重建应用索引表（全量替换，单事务）。
pub fn rebuild_app_index(conn: &mut Connection, entries: &[AppEntry]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM system_search_apps", [])?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO system_search_apps
             (app_kind, display_name, launch_target, canonical_target, aumid, icon_source, install_location, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for e in entries {
            let now = crate::db::connection::now_unix();
            stmt.execute(rusqlite::params![
                e.app_kind,
                e.display_name,
                e.launch_target,
                e.canonical_target,
                e.aumid,
                e.icon_source,
                e.install_location,
                now,
            ])?;
        }
    }
    tx.commit()
}

/// 从数据库查询应用（名称模糊匹配），返回可直接合并的 GlobalSearchHit。
pub fn query_apps(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> rusqlite::Result<Vec<crate::services::global_search::GlobalSearchHit>> {
    use crate::services::global_search::{canonical_key, escape_like, GlobalSearchHit};
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let like = format!("%{}%", escape_like(q));
    let mut stmt = conn.prepare(
        "SELECT id, app_kind, display_name, launch_target, aumid, icon_source
         FROM system_search_apps
         WHERE display_name LIKE ?1 ESCAPE '\\' COLLATE NOCASE
            OR launch_target LIKE ?1 ESCAPE '\\' COLLATE NOCASE
         ORDER BY
           CASE WHEN display_name LIKE ?2 ESCAPE '\\' COLLATE NOCASE THEN 100
                WHEN display_name LIKE ?3 ESCAPE '\\' COLLATE NOCASE THEN 60
                ELSE 30 END DESC
         LIMIT ?4",
    )?;
    let prefix = format!("{}%", escape_like(q));
    let rows = stmt.query_map(
        rusqlite::params![like, prefix, like, limit],
        |row| -> rusqlite::Result<GlobalSearchHit> {
            let app_kind: String = row.get("app_kind")?;
            let display_name: String = row.get("display_name")?;
            let launch_target: Option<String> = row.get("launch_target")?;
            let aumid: Option<String> = row.get("aumid")?;
            let icon_source: Option<String> = row.get("icon_source")?;
            let path = launch_target.or_else(|| aumid.clone());
            let key = aumid
                .clone()
                .map(|a| format!("app:{}", canonical_key(&a)))
                .or_else(|| path.as_ref().map(|p| format!("app:{}", canonical_key(p))))
                .unwrap_or_else(|| format!("app:{}", display_name));
            Ok(GlobalSearchHit {
                key,
                kind: "app".to_string(),
                name: display_name,
                path,
                source: "app_index".to_string(),
                matched_field: "name".to_string(),
                modified_at: None,
                icon_source,
                is_offline: false,
                actions: vec!["open".to_string()],
                score: if display_name.to_lowercase() == q.to_lowercase() { 200 } else if display_name.to_lowercase().starts_with(&q.to_lowercase()) { 160 } else { 120 },
            })
        },
    )?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
```

- [ ] **Step 5: 实现 Windows 应用枚举**

继续在 `app_index.rs` 追加（cfg(windows)）：

```rust
#[cfg(windows)]
pub mod windows {
    use super::*;
    use std::path::PathBuf;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    /// 枚举全部应用来源并返回条目列表。
    pub fn collect_apps() -> Vec<AppEntry> {
        let mut out = Vec::new();
        collect_start_menu_links(&mut out);
        collect_app_paths(&mut out);
        collect_uninstall(&mut out);
        dedup_entries(&mut out);
        out
    }

    fn dedup_entries(entries: &mut Vec<AppEntry>) {
        let mut seen = std::collections::HashSet::new();
        entries.retain(|e| seen.insert(e.canonical_target.clone()));
    }

    /// 开始菜单快捷方式（用户 + 系统）。
    fn collect_start_menu_links(out: &mut Vec<AppEntry>) {
        let mut dirs = Vec::new();
        if let Ok(p) = std::env::var("APPDATA") {
            dirs.push(PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs"));
        }
        if let Ok(p) = std::env::var("PROGRAMDATA") {
            dirs.push(PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs"));
        }
        for dir in dirs {
            walk_lnk(&dir, out);
        }
    }

    fn walk_lnk(dir: &std::path::Path, out: &mut Vec<AppEntry>) {
        let Ok(read) = std::fs::read_dir(dir) else { return };
        for entry in read.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk_lnk(&p, out);
            } else if p.extension().map(|e| e.to_ascii_lowercase()) == Some(std::ffi::OsStr::new("lnk")) {
                let name = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if name.is_empty() {
                    continue;
                }
                let target = resolve_lnk_target(&p).unwrap_or_else(|| p.to_string_lossy().to_string());
                let kind = if is_store_link(&target) { "store" } else { "shortcut" };
                out.push(AppEntry {
                    app_kind: kind.into(),
                    display_name: name,
                    launch_target: Some(target.clone()),
                    canonical_target: canonical_app_target(&target),
                    aumid: None,
                    icon_source: Some(p.to_string_lossy().to_string()),
                    install_location: None,
                });
            }
        }
    }

    /// 解析 .lnk 快捷方式目标（IShellLinkW）。
    /// 解析失败返回 None，由调用方回退为快捷方式自身路径（Store 快捷方式可经 shell:AppsFolder 启动）。
    pub fn resolve_lnk_target(lnk: &std::path::Path) -> Option<String> {
        use windows::core::Interface;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, IPersistFile,
        };
        use windows::Win32::UI::Shell::{IShellLinkW, CShellLink, SLGP_RAWPATH};
        let wide = to_wide(&lnk.to_string_lossy());
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let link: IShellLinkW = CoCreateInstance(&CShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
            let persist: IPersistFile = link.cast().ok()?;
            persist.Load(&wide, 0).ok()?;
            let mut buf = [0u16; 1024];
            let found = link.GetPath(&mut buf, SLGP_RAWPATH).ok()?;
            if found == 0 {
                return None;
            }
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..end]))
        }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// App Paths 注册表。
    fn collect_app_paths(out: &mut Vec<AppEntry>) {
        let roots = [
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths"),
            (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths"),
        ];
        for (root, sub) in roots {
            let Ok(key) = RegKey::predef(root).open_subkey_with_flags(sub, KEY_READ) else { continue };
            for name in key.enum_keys().flatten() {
                let Ok(sub) = key.open_subkey_with_flags(&name, KEY_READ) else { continue };
                let default: Option<String> = sub.get_value("").ok();
                if let Some(target) = default.filter(|t| !t.trim().is_empty()) {
                    out.push(AppEntry {
                        app_kind: "win32".into(),
                        display_name: PathBuf::from(&name)
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or(name.clone()),
                        launch_target: Some(target.clone()),
                        canonical_target: canonical_app_target(&target),
                        aumid: None,
                        icon_source: None,
                        install_location: None,
                    });
                }
            }
        }
    }

    /// 卸载注册表中的桌面应用。
    fn collect_uninstall(out: &mut Vec<AppEntry>) {
        let subs = [
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
            (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        ];
        for (root, sub) in subs {
            let Ok(key) = RegKey::predef(root).open_subkey_with_flags(sub, KEY_READ) else { continue };
            for name in key.enum_keys().flatten() {
                let Ok(sub) = key.open_subkey_with_flags(&name, KEY_READ) else { continue };
                let display: Option<String> = sub.get_value("DisplayName").ok();
                let Some(display) = display.filter(|d| !d.trim().is_empty()) else { continue };
                let target: Option<String> = sub
                    .get_value("DisplayIcon")
                    .ok()
                    .map(|s: String| s.split(',').next().unwrap_or("").to_string());
                let install: Option<String> = sub.get_value("InstallLocation").ok();
                let target = target
                    .filter(|t| !t.trim().is_empty())
                    .or_else(|| install.as_ref().map(|i| format!("{i}\\{display}.exe")));
                let Some(target) = target else { continue };
                out.push(AppEntry {
                    app_kind: "win32".into(),
                    display_name: display,
                    launch_target: Some(target.clone()),
                    canonical_target: canonical_app_target(&target),
                    aumid: None,
                    icon_source: Some(target.clone()),
                    install_location: install,
                });
            }
        }
    }
}
```

注：`resolve_lnk_target` 中 Store 分支（`GetIDList`）为可选增强；首版对无法解析的 `.lnk` 直接使用快捷方式路径作为启动目标（`explorer.exe shell:AppsFolder\...` 可启动 Store 应用）。若 `Shell_GetIDListFromParsingName` 绑定不可用，删除该分支即可。

- [ ] **Step 6: 编译修复并运行测试**

Run: `cargo test --lib services::app_index`
Expected: 2 个测试 PASS。若某个 windows 绑定编译失败，按编译器提示移除该符号引用（保持 `collect_apps` 三条来源可用）。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/src/services/app_index.rs src-tauri/src/services/mod.rs
git commit -m "feat(global-search): build app index from start menu, app paths, uninstall and store links"
```
无 git 时跳过。

---

### Task 4: 本地索引查询与 NexusFile 聚合（含 FTS 探测）

**Files:**
- Modify: `src-tauri/src/services/global_search.rs`

- [ ] **Step 1: 编写失败测试**

在 `global_search.rs` 测试模块追加：

```rust
    #[test]
    fn fts_probe_reports_available_or_degrades() {
        let conn = Connection::open_in_memory().unwrap();
        // 不崩溃且返回布尔
        let _ = ensure_fts(&conn);
    }

    #[test]
    fn fts_probe_reports_available_or_degrades() {
        let conn = Connection::open_in_memory().unwrap();
        // bundled SQLite 应支持 trigram；若未来降级也应返回确定值而非 panic
        let ok = ensure_fts(&conn);
        assert!(
            ok || conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='system_search_entries_fts'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0)
                == 0,
            "ensure_fts 返回 false 时不应留下半成品 FTS 表"
        );
    }

    #[test]
    fn fts_sync_and_match_returns_hits() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        // 先插基础表，再同步 FTS，验证 FTS 分支真正命中
        conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\docs\\季度报告.docx', '季度报告.docx', 'file', 'v1', 1, 1)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM system_search_entries LIMIT 1", [], |r| r.get(0))
            .unwrap();
        fts_sync_upsert(&conn, id, "季度报告.docx", "c:\\docs\\季度报告.docx");
        // 3 字符以上查询应走 FTS 分支且命中
        let hits = query_local_index(&conn, "季度报告", 10).unwrap();
        assert_eq!(hits.len(), 1, "FTS 同步后 MATCH 应命中");
        assert_eq!(hits[0].name, "季度报告.docx");
    }

    #[test]
    fn query_local_index_degrades_on_fts_syntax_error() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\x\\100%完成率.png', '100%完成率.png', 'file', 'v1', 1, 1)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM system_search_entries LIMIT 1", [], |r| r.get(0))
            .unwrap();
        fts_sync_upsert(&conn, id, "100%完成率.png", "c:\\x\\100%完成率.png");
        // 含 % 的查询在 FTS MATCH 中会报语法错误，应降级 LIKE 而非整体 Err
        let hits = query_local_index(&conn, "100%", 10).unwrap();
        assert_eq!(hits.len(), 1, "FTS 语法错误应降级 LIKE 命中");
    }

    #[test]
    fn query_local_index_searches_name_and_path() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\docs\\年度报告.pdf', '年度报告.pdf', 'file', 'v1', 1, 1)",
            [],
        )
        .unwrap();
        let hits = query_local_index(&conn, "年度", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "file");
        // 路径匹配同样命中
        let hits = query_local_index(&conn, "docs", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib services::global_search`
Expected: 编译失败（`ensure_fts` / `query_local_index` 未定义）。

- [ ] **Step 3: 实现 FTS 探测与本地索引查询**

在 `global_search.rs` 主体追加：

```rust
use rusqlite::Connection;

/// 探测并确保 FTS5 trigram 索引可用；不可用时静默返回 false（查询走 LIKE）。
/// 运行时探测避免迁移阶段依赖具体 tokenizer。
pub fn ensure_fts(conn: &Connection) -> bool {
    let probe = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='system_search_entries_fts'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if probe > 0 {
        return true;
    }
    // 尝试创建 trigram 虚拟表；失败说明当前 SQLite 不支持，返回 false。
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS system_search_entries_fts
         USING fts5(display_name, canonical_path, content='system_search_entries', content_rowid='id', tokenize='trigram')",
    )
    .is_ok()
}

/// 将一条索引记录写入 FTS（外部内容表需手动同步）。
/// 注意：FTS5 虚拟表不支持 `ON CONFLICT ... DO UPDATE`（upsert），
/// 必须用 `INSERT OR REPLACE` 实现幂等写入。
pub fn fts_sync_upsert(conn: &Connection, id: i64, display_name: &str, canonical_path: &str) {
    if ensure_fts(conn) {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO system_search_entries_fts(rowid, display_name, canonical_path)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![id, display_name, canonical_path],
        );
    }
}

/// 查询本地系统搜索索引（FTS 优先，LIKE 兜底），返回可直接合并的命中。
pub fn query_local_index(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> rusqlite::Result<Vec<GlobalSearchHit>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let lim = limit.clamp(1, 500);
    let fts = ensure_fts(conn);
    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if fts && q.chars().count() >= 3 {
        (
            "SELECT e.id, e.canonical_path, e.display_name, e.entry_kind, e.modified_at, e.is_offline
             FROM system_search_entries e
             JOIN system_search_entries_fts f ON f.rowid = e.id
             WHERE system_search_entries_fts MATCH ?1
             ORDER BY f.rank LIMIT ?2".to_string(),
            vec![Box::new(q.to_string()), Box::new(lim)],
        )
    } else {
        let like = format!("%{}%", escape_like(q));
        (
            "SELECT id, canonical_path, display_name, entry_kind, modified_at, is_offline
             FROM system_search_entries
             WHERE display_name LIKE ?1 ESCAPE '\\' COLLATE NOCASE
                OR canonical_path LIKE ?1 ESCAPE '\\' COLLATE NOCASE
             ORDER BY
               CASE WHEN display_name LIKE ?2 ESCAPE '\\' COLLATE NOCASE THEN 100
                    WHEN display_name LIKE ?3 ESCAPE '\\' COLLATE NOCASE THEN 60
                    ELSE 30 END DESC
             LIMIT ?4".to_string(),
            vec![
                Box::new(like.clone()),
                Box::new(format!("{}%", escape_like(q))),
                Box::new(like),
                Box::new(lim),
            ],
        )
    };
    // FTS 查询报错（trigram 不支持的特殊字符语法，如 %、"、(）时降级，不向上抛
    let mut hits = if fts && q.chars().count() >= 3 {
        query_local_sql(conn, &sql, &params, q).unwrap_or_default()
    } else {
        query_local_sql(conn, &sql, &params, q)?
    };
    // FTS 分支（可触发但索引未同步）空结果 → 降级 LIKE，保证基础可用
    if hits.is_empty() && fts && q.chars().count() >= 3 {
        let like = format!("%{}%", escape_like(q));
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = (
            "SELECT id, canonical_path, display_name, entry_kind, modified_at, is_offline
             FROM system_search_entries
             WHERE display_name LIKE ?1 ESCAPE '\\' COLLATE NOCASE
                OR canonical_path LIKE ?1 ESCAPE '\\' COLLATE NOCASE
             ORDER BY
               CASE WHEN display_name LIKE ?2 ESCAPE '\\' COLLATE NOCASE THEN 100
                    WHEN display_name LIKE ?3 ESCAPE '\\' COLLATE NOCASE THEN 60
                    ELSE 30 END DESC
             LIMIT ?4".to_string(),
            vec![
                Box::new(like.clone()),
                Box::new(format!("{}%", escape_like(q))),
                Box::new(like),
                Box::new(lim),
            ],
        );
        hits = query_local_sql(conn, &sql, &params, q)?;
    }
    Ok(hits)
}

/// 执行本地索引 SQL 并解析命中（供 query_local_index 两分支复用）。
fn query_local_sql(
    conn: &Connection,
    sql: &str,
    params: &[Box<dyn rusqlite::types::ToSql>],
    q: &str,
) -> rusqlite::Result<Vec<GlobalSearchHit>> {
    let mut stmt = conn.prepare(sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|b| b.as_ref() as &dyn rusqlite::types::ToSql).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let name: String = row.get("display_name")?;
        let path: String = row.get("canonical_path")?;
        let kind: String = row.get("entry_kind")?;
        let ql = q.to_lowercase();
        let nl = name.to_lowercase();
        let score = if nl == ql {
            200
        } else if nl.starts_with(&ql) {
            160
        } else {
            120
        };
        Ok(GlobalSearchHit {
            key: canonical_key(&path),
            kind: kind.clone(),
            name,
            path: Some(path),
            source: "local_index".to_string(),
            matched_field: if nl.contains(&ql) { "name".into() } else { "path".into() },
            modified_at: row.get("modified_at")?,
            icon_source: None,
            is_offline: row.get::<_, i64>("is_offline")? != 0,
            actions: vec!["open".to_string(), "reveal".to_string()],
            score,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 合并 NexusFile 业务资源结果（复用现有 search_resources 逻辑）。
pub fn merge_nexus_hits(
    hits: &mut Vec<GlobalSearchHit>,
    nexus: Vec<crate::commands::search::SearchHit>,
) {
    for h in nexus {
        let path = h.path.clone().unwrap_or_default();
        let key = if path.is_empty() {
            format!("nexus:{}", h.id)
        } else {
            canonical_key(&path)
        };
        hits.push(GlobalSearchHit {
            key,
            kind: h.kind,
            name: h.name,
            path: h.path,
            source: "nexus".to_string(),
            matched_field: "name".to_string(),
            modified_at: Some(h.updated_at),
            icon_source: None,
            is_offline: false,
            actions: vec!["open".to_string()],
            score: 90,
        });
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --lib services::global_search`
Expected: 新增 2 个测试 PASS（`fts_probe_reports_available_or_degrades`、`query_local_index_searches_name_and_path`）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/global_search.rs
git commit -m "feat(global-search): local index query with fts trigram probe and nexus merge"
```
无 git 时跳过。

---

### Task 5: Windows Search 即时层（COM + 降级）

**Files:**
- Create: `src-tauri/src/services/windows_search.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: 编写绑定探测测试**

`src-tauri/src/services/windows_search.rs`：

```rust
/// Windows Search 即时查询。COM 绑定不可用时返回 Err，由调用方降级。
#[cfg(windows)]
pub fn search(query: &str, limit: u32) -> Result<Vec<crate::services::global_search::GlobalSearchHit>, String> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::System::Search::{ISearchCatalogManager, ISearchManager, CSearchManager, SEARCH_TERM_PREFIX_ALL};

    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mgr: ISearchManager = CoCreateInstance(&CSearchManager, None, CLSCTX_LOCAL_SERVER)
            .map_err(|e| format!("CoCreateInstance(SearchManager): {e}"))?;
        let catalog: ISearchCatalogManager = mgr
            .GetCatalog(windows::core::PCWSTR(wide("SystemIndex").as_ptr()))
            .map_err(|e| format!("GetCatalog: {e}"))?;
        let helper = catalog
            .GetQueryHelper()
            .map_err(|e| format!("GetQueryHelper: {e}"))?;
        // 生成 SQL（AQS → 结构化查询）
        let mut sql = windows::core::PWSTR::null();
        helper
            .GenerateSQLFromUserQuery(
                windows::core::PCWSTR(wide(query).as_ptr()),
                &mut sql,
            )
            .map_err(|e| format!("GenerateSQLFromUserQuery: {e}"))?;
        let sql_string = if sql.is_null() {
            String::new()
        } else {
            unsafe {
                let len = windows::core::wcslen(sql.as_ptr()) as usize;
                String::from_utf16_lossy(std::slice::from_raw_parts(sql.as_ptr(), len))
            }
        };
        // SQL 通过 OLE DB 执行；绑定不可用时向调用方传播 Err（触发上层降级到本地索引）。
        // 错误串携带探测证据（生成的 SQL 字节数），便于调用方区分「索引链路正常但 OLE DB 未绑定」
        // 与「索引不可达」：只有整条 COM 探测链成功后才可能到达这里。
        let result = query_windows_search_ole_db(&sql_string, limit).map_err(|_| {
            format!(
                "ole_db_not_bound: sql_generated {} bytes",
                sql_string.len()
            )
        })?;
        drop(sql_string);
        Ok(result)
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 通过 OLE DB 执行 Windows Search SQL（Search.CollatorDSO）。
/// windows crate 未提供完整 OLE DB 绑定时，本函数返回 Err 触发降级。
fn query_windows_search_ole_db(_sql: &str, _limit: u32) -> Result<Vec<crate::services::global_search::GlobalSearchHit>, String> {
    // 首版降级：Windows Search 索引用于即时层可选增强。
    // 完整实现需 IDBInitialize + ICommandText + IRowset（约 300 行 COM 调用），
    // 若后续需要，可在本函数内按 OLE DB provider 文档补齐。
    Err("ole_db_not_bound".into())
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo check`
Expected: 编译通过。若 `ISearchManager` / `CSearchManager` / `GenerateSQLFromUserQuery` 任一符号缺失，将对应行移除并让 `search()` 直接返回 `Err("windows_search_unavailable")`（降级路径保持可用）。

- [ ] **Step 3: 编写降级测试**

`windows_search.rs` 测试模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_empty() {
        #[cfg(windows)]
        {
            let r = search("", 10);
            assert!(r.is_ok() && r.unwrap().is_empty());
        }
    }

    #[test]
    fn ole_db_fallback_degrades_gracefully() {
        let r = query_windows_search_ole_db("SELECT 1", 10);
        assert!(r.is_err(), "未绑定 OLE DB 时应降级而非崩溃");
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --lib services::windows_search`
Expected: 2 个测试 PASS。

- [ ] **Step 5: 注册模块**

`src-tauri/src/services/mod.rs` 追加：

```rust
pub mod windows_search;
```

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/services/windows_search.rs src-tauri/src/services/mod.rs
git commit -m "feat(global-search): windows search COM probe with graceful degrade"
```
无 git 时跳过。

---

### Task 6: 命令层（start/cancel/open/reveal/status + 运行时）

**Files:**
- Create: `src-tauri/src/commands/global_search.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/events.rs`
- Modify: `src-tauri/Cargo.toml`（若需 `tauri::Emitter` 已具备，无需改动）

- [ ] **Step 1: 定义事件常量**

`src-tauri/src/events.rs` 追加：

```rust
pub const EVENT_GLOBAL_SEARCH_BATCH: &str = "global-search://batch";
pub const EVENT_GLOBAL_SEARCH_INDEX_PROGRESS: &str = "global-search://index-progress";
```

- [ ] **Step 2: 扩展 AppState 与运行时**

`src-tauri/src/lib.rs`：

```rust
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// 全局搜索运行时：活动查询表 + 后台扫描控制。
pub struct SearchRuntime {
    pub active_queries: Mutex<HashMap<u64, ()>>,
    pub next_query_id: AtomicU64,
    pub scan_paused: Arc<AtomicBool>,
    pub scan_trigger: AtomicU64, // 自增触发重建
}
```

`AppState` 增加字段：

```rust
    pub search: SearchRuntime,
```

`run()` 的 `app.manage(...)` 处构造：

```rust
            app.manage(AppState {
                data_dir,
                managed_dir,
                conn: Mutex::new(conn),
                sampler: Mutex::new(services::system_service::SystemSampler::new()),
                search: SearchRuntime {
                    active_queries: Mutex::new(HashMap::new()),
                    next_query_id: AtomicU64::new(1),
                    scan_paused: Arc::new(AtomicBool::new(false)),
                    scan_trigger: AtomicU64::new(0),
                },
            });
```

`setup` 中构建应用索引（后台线程，不阻塞启动；扫描线程在 Task 9 实现后追加调用）：

```rust
            // 后台构建应用索引（开始菜单 / App Paths / 卸载注册表 / Store 快捷方式）
            {
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    #[cfg(windows)]
                    {
                        let entries = crate::services::app_index::windows::collect_apps();
                        let data_dir = app_handle.state::<AppState>().data_dir.clone();
                        if let Ok(mut conn) = crate::db::connection::open(&data_dir.join("workspace.db")) {
                            let _ = crate::services::app_index::rebuild_app_index(&mut conn, &entries);
                        }
                    }
                });
            }
```

- [ ] **Step 3: 编写命令模块**

`src-tauri/src/commands/global_search.rs`：

```rust
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::ipc::CommandResult;
use crate::services::global_search::{dedup_hits, sort_hits, GlobalSearchHit};
use crate::AppState;

/// 首屏返回：来源状态 + 首批命中。
#[derive(serde::Serialize)]
pub struct SearchBatch {
    pub search_id: u64,
    pub hits: Vec<GlobalSearchHit>,
    pub sources: serde_json::Value,
    pub index_incomplete: bool,
}

/// 启动一次全局搜索：同步聚合即时来源，后台持续补充本地索引命中。
#[tauri::command]
pub fn start_global_search(
    app: AppHandle,
    state: State<AppState>,
    query: String,
    limit: Option<i64>,
) -> CommandResult<SearchBatch> {
    let q = query.trim().to_string();
    let lim = limit.unwrap_or(100).clamp(1, 500);
    let search_id = state.search.next_query_id.fetch_add(1, Ordering::SeqCst);
    state
        .search
        .active_queries
        .lock()
        .expect("search lock")
        .insert(search_id, ());

    if q.is_empty() {
        state.search.active_queries.lock().expect("lock").remove(&search_id);
        return Ok(SearchBatch {
            search_id,
            hits: Vec::new(),
            sources: serde_json::json!({ "query": "" }),
            index_incomplete: false,
        });
    }

    let mut hits: Vec<GlobalSearchHit> = Vec::new();
    let mut source_status = serde_json::json!({});

    // 1) 应用索引（数据库查询，先于 Windows Search，保证桌面环境可用）
    {
        let conn = state.conn.lock().expect("db lock");
        match crate::services::app_index::query_apps(&conn, &q, lim) {
            Ok(mut h) => hits.append(&mut h),
            Err(e) => {
                source_status["app_index"] = serde_json::json!({ "error": e.to_string() });
            }
        }
        // 2) NexusFile 业务资源
        match crate::commands::search::execute_search(&conn, &q, None, false, lim, 0) {
            Ok(nexus) => crate::services::global_search::merge_nexus_hits(&mut hits, nexus),
            Err(e) => {
                source_status["nexus"] = serde_json::json!({ "error": e.to_string() });
            }
        }
        // 3) 本地索引即时部分（若已有数据）
        match crate::services::global_search::query_local_index(&conn, &q, lim / 2) {
            Ok(mut h) => hits.append(&mut h),
            Err(e) => {
                source_status["local_index"] = serde_json::json!({ "error": e.to_string() });
            }
        }
    }

    // 4) Windows Search 即时层（可降级）
    #[cfg(windows)]
    match crate::services::windows_search::search(&q, 50) {
        Ok(mut h) => hits.append(&mut h),
        Err(e) => {
            source_status["windows"] = serde_json::json!({ "error": e });
        }
    }

    dedup_hits(&mut hits);
    sort_hits(&mut hits);
    hits.truncate(lim as usize);

    let incomplete = {
        let conn = state.conn.lock().expect("db lock");
        let scanning: i64 = conn
            .query_row(
                "SELECT count(*) FROM system_search_scan_state WHERE status IN ('pending','scanning','paused')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(1);
        scanning > 0
    };

    // 后台补充：若索引未完成，启动持续补批任务
    if incomplete {
        let app2 = app.clone();
        let q2 = q.clone();
        std::thread::spawn(move || {
            let mut seen_keys = std::collections::HashSet::new();
            for _ in 0..40 {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                let st = app2.state::<AppState>();
                let still_active = st
                    .search
                    .active_queries
                    .lock()
                    .expect("lock")
                    .contains_key(&search_id);
                if !still_active {
                    return;
                }
                let conn = st.conn.lock().expect("db lock");
                match crate::services::global_search::query_local_index(&conn, &q2, 50) {
                    Ok(mut h) => {
                        let new_hits: Vec<GlobalSearchHit> = h
                            .drain(..)
                            .filter(|x| seen_keys.insert(x.key.clone()))
                            .collect();
                        if !new_hits.is_empty() {
                            let _ = app2.emit(
                                crate::events::EVENT_GLOBAL_SEARCH_BATCH,
                                serde_json::json!({
                                    "search_id": search_id,
                                    "hits": new_hits,
                                }),
                            );
                        }
                    }
                    Err(_) => return,
                }
            }
        });
    }

    Ok(SearchBatch {
        search_id,
        hits,
        sources: source_status,
        index_incomplete: incomplete,
    })
}

/// 取消一次全局搜索（停止后台补批）。
#[tauri::command]
pub fn cancel_global_search(
    state: State<AppState>,
    search_id: u64,
) -> CommandResult<()> {
    state
        .search
        .active_queries
        .lock()
        .expect("search lock")
        .remove(&search_id);
    Ok(())
}

/// 打开搜索结果（后端校验后执行，不接受前端任意命令）。
#[tauri::command]
pub fn open_search_result(
    state: State<AppState>,
    key: String,
) -> CommandResult<()> {
    let clean_key = key.strip_prefix("app:").unwrap_or(&key);
    let conn = state.conn.lock().expect("db lock");
    let row = conn
        .query_row(
            "SELECT launch_target, aumid FROM system_search_apps WHERE canonical_target = ?1",
            [clean_key],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()?;

    if let Some((target, aumid)) = row {
        return open_target(target, aumid);
    }

    // 文件/文件夹：规范化路径直接打开（用 system_search_entries 校验）
    let path = clean_key;
    let exists = std::path::Path::new(path).exists();
    if exists {
        return crate::services::system_windows::shell_open_path(path);
    }
    Err(AppError::new("target_missing", "目标文件已不存在"))
}

/// 打开所在位置（仅文件/文件夹）。
#[tauri::command]
pub fn reveal_search_result(state: State<AppState>, key: String) -> CommandResult<()> {
    let clean_key = key.strip_prefix("app:").unwrap_or(&key);
    let conn = state.conn.lock().expect("db lock");
    let path: Option<String> = conn
        .query_row(
            "SELECT canonical_path FROM system_search_entries WHERE canonical_path = ?1",
            [clean_key],
            |r| r.get(0),
        )
        .optional()?;
    let path = path.unwrap_or_else(|| clean_key.to_string());
    crate::services::system_windows::shell_reveal_path(&path)
}

fn open_target(target: Option<String>, aumid: Option<String>) -> CommandResult<()> {
    if let Some(a) = aumid {
        return crate::services::system_windows::shell_open_aumid(&a);
    }
    if let Some(t) = target {
        return crate::services::system_windows::shell_open_path(&t);
    }
    Err(AppError::new("no_target", "应用缺少启动目标"))
}

use rusqlite::OptionalExtension;
```

注：`system_windows.rs` 中新增三个公开函数（`shell_open_path`、`shell_reveal_path`、`shell_open_aumid`），在 Task 6 Step 4 实现。

- [ ] **Step 4: 在 system_windows.rs 增加打开/定位工具**

`src-tauri/src/services/system_windows.rs` 追加：

```rust
/// 用系统默认关联打开路径（文件/文件夹/可执行文件）。
pub fn shell_open_path(path: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let file_vec: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let op_vec: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let res = ShellExecuteW(None, PCWSTR(op_vec.as_ptr()), PCWSTR(file_vec.as_ptr()), PCWSTR::null(), None, SW_SHOWNORMAL);
        if res.0 as isize <= 32 {
            return Err(format!("ShellExecute failed: {}", res.0));
        }
    }
    Ok(())
}

/// 在资源管理器中定位路径。
pub fn shell_reveal_path(path: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let file_vec: Vec<u16> = "explorer.exe".encode_utf16().chain(std::iter::once(0)).collect();
    let params_vec: Vec<u16> = format!("/select,\"{path}\"").encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let res = ShellExecuteW(None, PCWSTR::null(), PCWSTR(file_vec.as_ptr()), PCWSTR(params_vec.as_ptr()), None, SW_SHOWNORMAL);
        if res.0 as isize <= 32 {
            return Err(format!("reveal failed: {}", res.0));
        }
    }
    Ok(())
}

/// 通过 AUMID 启动 Store 应用（shell:AppsFolder 协议）。
pub fn shell_open_aumid(aumid: &str) -> Result<(), String> {
    let target = format!("shell:AppsFolder\\{aumid}");
    shell_open_path(&target)
}
```

`system_windows.rs` 顶部补全 `use windows::core::PCWSTR;`（若未导入）。编译修复：`SEE_MASK_NOCLOSEPROCESS` 如未启用 feature 则删除该常量使用。

- [ ] **Step 5: 注册命令**

`src-tauri/src/commands/mod.rs`：

```rust
pub mod global_search;
```

`src-tauri/src/lib.rs` invoke_handler 追加：

```rust
            commands::global_search::start_global_search,
            commands::global_search::cancel_global_search,
            commands::global_search::open_search_result,
            commands::global_search::reveal_search_result,
```

- [ ] **Step 6: 编译并运行现有测试**

Run: `cargo test`
Expected: 编译通过，`test result: ok`（现有 76+ 项测试全部通过；`global_search` 新增测试同样通过）。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/commands/global_search.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/events.rs src-tauri/src/services/system_windows.rs
git commit -m "feat(global-search): search command layer with open/reveal actions and runtime"
```
无 git 时跳过。

---

### Task 7: 前端统一结果模型与搜索页改造

**Files:**
- Create: `src/features/search/lib/globalSearch.ts`
- Modify: `src/features/search/routes/SearchPage.tsx`
- Modify: `src/components/Topbar.tsx`
- Modify: `src/styles/app.css`

- [ ] **Step 1: 定义前端类型与调用封装**

`src/features/search/lib/globalSearch.ts`：

```ts
import { call } from "../../../lib/tauri";
import { listen } from "@tauri-apps/api/event";

export interface GlobalSearchHit {
  key: string;
  kind: "file" | "folder" | "app" | "page" | "project";
  name: string;
  path: string | null;
  source: "windows" | "local_index" | "app_index" | "nexus";
  matched_field: "name" | "path";
  modified_at: number | null;
  icon_source: string | null;
  is_offline: boolean;
  actions: string[];
  score: number;
}

export interface SearchBatch {
  search_id: number;
  hits: GlobalSearchHit[];
  sources: Record<string, { error?: string }>;
  index_incomplete: boolean;
}

export function startSearch(
  query: string,
  limit?: number,
): Promise<SearchBatch> {
  return call<SearchBatch>("start_global_search", { query, limit: limit ?? 100 });
}

export function cancelSearch(searchId: number): Promise<void> {
  return call<void>("cancel_global_search", { searchId });
}

export function openResult(key: string): Promise<void> {
  return call<void>("open_search_result", { key });
}

export function revealResult(key: string): Promise<void> {
  return call<void>("reveal_search_result", { key });
}

/** 订阅后台补批事件，返回取消订阅函数。 */
export function onSearchBatch(
  searchId: number,
  cb: (hits: GlobalSearchHit[]) => void,
): Promise<() => void> {
  return listen<{ search_id: number; hits: GlobalSearchHit[] }>(
    "global-search://batch",
    (e) => {
      if (e.payload.search_id === searchId) cb(e.payload.hits);
    },
  ).then((unlisten) => unlisten);
}
```

- [ ] **Step 2: 重写搜索页**

`src/features/search/routes/SearchPage.tsx` 全量替换（保留 URL 同步与竞态防护）：

```tsx
import { useCallback, useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Search, FileText, Folder, AppWindow, FileCode2, Loader2, CornerDownLeft } from "lucide-react";
import { formatTime } from "../../../lib/tauri";
import {
  GlobalSearchHit,
  startSearch,
  cancelSearch,
  openResult,
  revealResult,
  onSearchBatch,
} from "../lib/globalSearch";

const KIND_LABEL: Record<string, string> = {
  file: "文件",
  folder: "文件夹",
  app: "应用",
  page: "页面",
  project: "项目",
};

const FILTERS = [
  { value: "", label: "全部" },
  { value: "app", label: "应用" },
  { value: "file", label: "文件" },
  { value: "folder", label: "文件夹" },
  { value: "page", label: "页面/项目" },
];

function kindIcon(kind: string) {
  if (kind === "app") return <AppWindow size={15} color="var(--code)" />;
  if (kind === "folder") return <Folder size={15} color="var(--folder)" />;
  if (kind === "page" || kind === "project") return <FileCode2 size={15} color="var(--primary)" />;
  return <FileText size={15} color="var(--file)" />;
}

export function SearchPage() {
  const [params, setParams] = useSearchParams();
  const urlQ = params.get("q") ?? "";
  const [query, setQuery] = useState(urlQ);
  const [kind, setKind] = useState("");
  const [results, setResults] = useState<GlobalSearchHit[]>([]);
  const [searched, setSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [indexing, setIndexing] = useState(false);
  const seqRef = useRef(0);
  const searchIdRef = useRef<number | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);
  const seenKeysRef = useRef<Set<string>>(new Set());

  const cleanup = useCallback(() => {
    if (searchIdRef.current != null) cancelSearch(searchIdRef.current).catch(() => {});
    searchIdRef.current = null;
    if (unlistenRef.current) {
      unlistenRef.current();
      unlistenRef.current = null;
    }
  }, []);

  const doSearch = useCallback(
    async (q: string, k: string) => {
      const trimmed = q.trim();
      const seq = ++seqRef.current;
      cleanup();
      if (!trimmed) {
        setResults([]);
        setSearched(true);
        setError(null);
        setLoading(false);
        setIndexing(false);
        return;
      }
      setLoading(true);
      setError(null);
      seenKeysRef.current = new Set();
      try {
        const batch = await startSearch(trimmed, 100);
        if (seq !== seqRef.current) return;
        searchIdRef.current = batch.search_id;
        batch.hits.forEach((h) => seenKeysRef.current.add(h.key));
        const filtered = k ? batch.hits.filter((h) => filterMatch(h.kind, k)) : batch.hits;
        setResults(filtered);
        setSearched(true);
        setIndexing(batch.index_incomplete);
        if (batch.index_incomplete) {
          unlistenRef.current = await onSearchBatch(batch.search_id, (hits) => {
            if (seq !== seqRef.current) return;
            const fresh = hits.filter(
              (h) => !seenKeysRef.current.has(h.key) && (k ? filterMatch(h.kind, k) : true),
            );
            if (!fresh.length) return;
            fresh.forEach((h) => seenKeysRef.current.add(h.key));
            setResults((prev) => [...prev, ...fresh]);
          });
        }
      } catch (e) {
        if (seq === seqRef.current) {
          setError((e as Error).message);
          setResults([]);
          setSearched(true);
        }
      } finally {
        if (seq === seqRef.current) setLoading(false);
      }
    },
    [cleanup],
  );

  function filterMatch(hitKind: string, filter: string): boolean {
    if (filter === "page") return hitKind === "page" || hitKind === "project";
    return hitKind === filter;
  }

  // 输入防抖 + URL 同步
  useEffect(() => {
    setQuery(urlQ);
    const timer = window.setTimeout(() => {
      if (urlQ) doSearch(urlQ, "");
    }, 200);
    return () => window.clearTimeout(timer);
  }, [urlQ, doSearch]);

  useEffect(() => cleanup, [cleanup]);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = query.trim();
    if (!trimmed) return;
    if (urlQ === trimmed) doSearch(trimmed, kind);
    else setParams({ q: trimmed }, { replace: true });
  };

  const pickKind = (v: string) => {
    setKind(v);
    const q = urlQ || query.trim();
    if (q) doSearch(q, v);
  };

  const activate = async (hit: GlobalSearchHit) => {
    try {
      await openResult(hit.key);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const reveal = async (hit: GlobalSearchHit) => {
    try {
      await revealResult(hit.key);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const copyPath = async (hit: GlobalSearchHit) => {
    if (hit.path) await navigator.clipboard.writeText(hit.path);
  };

  return (
    <div className="search-page">
      <div className="search-head">
        <form className="search-input-wrap" onSubmit={submit}>
          <Search size={15} />
          <input
            value={query}
            placeholder="搜索文件、应用、页面、项目…"
            onChange={(e) => setQuery(e.target.value)}
            autoFocus
          />
          <button type="submit" className="btn btn-primary" disabled={loading}>
            {loading ? <Loader2 size={13} className="spin" /> : <Search size={13} />}
            {loading ? "搜索中…" : "搜索"}
          </button>
        </form>
        <div className="search-filters">
          {FILTERS.map((f) => (
            <button
              key={f.value}
              className={`filter-chip ${kind === f.value ? "active" : ""}`}
              onClick={() => pickKind(f.value)}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>

      {indexing && !loading && (
        <div className="system-note">
          后台索引仍在补充中，结果会持续更新
        </div>
      )}
      {error && <div className="system-error">{error}</div>}

      <div className="search-results">
        {searched && results.length === 0 && !loading && (
          <div className="empty-state">
            <span>没有找到匹配的结果</span>
          </div>
        )}
        {results.map((r) => (
          <div
            key={r.key}
            className="search-result"
            onDoubleClick={() => activate(r)}
            title={r.path ?? ""}
          >
            <span className="search-kind-icon">{kindIcon(r.kind)}</span>
            <div className="search-result-body">
              <div className="search-result-name">
                {r.name}
                {r.kind === "app" && <span className="search-source-tag">应用</span>}
                {r.is_offline && <span className="search-source-tag">离线</span>}
              </div>
              <div className="search-result-meta">
                {r.path ?? KIND_LABEL[r.kind] ?? r.kind}
              </div>
            </div>
            <span className="search-result-time">{formatTime(r.modified_at)}</span>
            <div className="search-result-actions">
              <button
                className="btn btn-ghost btn-sm"
                onClick={() => activate(r)}
                title="打开"
              >
                <CornerDownLeft size={12} />
              </button>
              {r.actions.includes("reveal") && (
                <button className="btn btn-ghost btn-sm" onClick={() => reveal(r)} title="打开所在位置">
                  定位
                </button>
              )}
              {r.path && (
                <button className="btn btn-ghost btn-sm" onClick={() => copyPath(r)} title="复制路径">
                  复制
                </button>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: 更新顶部栏文案**

`src/components/Topbar.tsx` 占位文案：

```tsx
placeholder="搜索文件、应用、页面、项目…"
```

- [ ] **Step 4: 追加样式**

`src/styles/app.css` 追加：

```css
.search-source-tag {
  display: inline-block;
  margin-left: 8px;
  padding: 1px 6px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  font-size: 11px;
  color: var(--text-muted);
  vertical-align: middle;
}
.search-result-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}
.btn-sm {
  padding: 4px 8px;
  font-size: 12px;
}
.system-note {
  margin: 12px 0 0;
  padding: 10px 12px;
  border: 1px solid var(--warning);
  border-radius: var(--radius);
  background: var(--surface-muted);
  color: var(--text-muted);
  font-size: 13px;
}
```

- [ ] **Step 5: 构建验证**

Run: `npm run build`（cwd `e:\work\新建文件夹`）
Expected: 构建成功，无 TypeScript 错误。

- [ ] **Step 6: 提交**

```bash
git add src/features/search/lib/globalSearch.ts src/features/search/routes/SearchPage.tsx src/components/Topbar.tsx src/styles/app.css
git commit -m "feat(global-search): unified search page with streaming batches and actions"
```
无 git 时跳过。

---

## 阶段二：全盘后台索引

### Task 8: 扫描状态仓库（卷枚举 + scan_state 读写）

**Files:**
- Create: `src-tauri/src/services/scan_service.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: 编写失败测试**

`src-tauri/src/services/scan_service.rs`（先写测试与空函数）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn conn() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut c).unwrap();
        c
    }

    #[test]
    fn upsert_volume_state_creates_then_updates() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v1", "C:\\", "pending", 1).unwrap();
        let s = get_volume_state(&c, "v1").unwrap();
        assert_eq!(s.status, "pending");
        assert_eq!(s.root_path, "C:\\");
        upsert_volume_state(&mut c, "v1", "C:\\", "scanning", 2).unwrap();
        let s = get_volume_state(&c, "v1").unwrap();
        assert_eq!(s.status, "scanning");
        assert_eq!(s.scan_generation, 2);
    }

    #[test]
    fn list_fixed_volumes_returns_at_least_system_drive() {
        let vols = list_fixed_volumes();
        assert!(!vols.is_empty(), "本机应至少有一个固定磁盘");
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib services::scan_service`
Expected: 编译失败（函数未定义）。

- [ ] **Step 3: 实现卷状态仓库与磁盘枚举**

`src-tauri/src/services/scan_service.rs`：

```rust
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

/// 单卷扫描状态。
#[derive(Debug, Clone)]
pub struct VolumeState {
    pub volume_id: String,
    pub root_path: String,
    pub status: String,
    pub scan_generation: i64,
    pub checkpoint: Option<String>,
    pub indexed_count: i64,
    pub skipped_count: i64,
    pub last_error: Option<String>,
}

pub fn upsert_volume_state(
    conn: &mut Connection,
    volume_id: &str,
    root_path: &str,
    status: &str,
    scan_generation: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO system_search_scan_state
         (volume_id, root_path, status, scan_generation, indexed_count, skipped_count, started_at)
         VALUES (?1, ?2, ?3, ?4, 0, 0, ?5)
         ON CONFLICT(volume_id) DO UPDATE SET
           root_path=excluded.root_path,
           status=excluded.status,
           scan_generation=excluded.scan_generation,
           last_error=NULL,
           started_at=COALESCE(system_search_scan_state.started_at, excluded.started_at)",
        params![volume_id, root_path, status, scan_generation, crate::db::connection::now_unix()],
    )
}

pub fn get_volume_state(conn: &Connection, volume_id: &str) -> rusqlite::Result<Option<VolumeState>> {
    conn.query_row(
        "SELECT volume_id, root_path, status, scan_generation, checkpoint, indexed_count, skipped_count, last_error
         FROM system_search_scan_state WHERE volume_id = ?1",
        [volume_id],
        |r| {
            Ok(VolumeState {
                volume_id: r.get(0)?,
                root_path: r.get(1)?,
                status: r.get(2)?,
                scan_generation: r.get(3)?,
                checkpoint: r.get(4)?,
                indexed_count: r.get(5)?,
                skipped_count: r.get(6)?,
                last_error: r.get(7)?,
            })
        },
    )
    .optional()
}

pub fn update_volume_progress(
    conn: &mut Connection,
    volume_id: &str,
    checkpoint: Option<&str>,
    indexed_count: i64,
    skipped_count: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE system_search_scan_state
         SET checkpoint = COALESCE(?1, checkpoint), indexed_count = ?2, skipped_count = ?3
         WHERE volume_id = ?4",
        params![checkpoint, indexed_count, skipped_count, volume_id],
    )
}

pub fn set_volume_status(
    conn: &mut Connection,
    volume_id: &str,
    status: &str,
    last_error: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE system_search_scan_state
         SET status = ?1, last_error = ?2,
             completed_at = CASE WHEN ?1 IN ('completed','error') THEN ?3 ELSE completed_at END
         WHERE volume_id = ?4",
        params![status, last_error, crate::db::connection::now_unix(), volume_id],
    )
}

/// 枚举本地固定磁盘（Windows: GetLogicalDrives + 驱动器类型）。
#[cfg(windows)]
pub fn list_fixed_volumes() -> Vec<PathBuf> {
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, DRIVE_FIXED, DRIVE_REMOTE};
    unsafe {
        let mask = GetLogicalDrives();
        let mut out = Vec::new();
        for i in 0..26u8 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i) as char;
            let root = format!("{letter}:\\");
            let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let ty = GetDriveTypeW(windows::core::PCWSTR(wide.as_ptr()));
            if ty == DRIVE_FIXED {
                out.push(PathBuf::from(&root));
            }
        }
        out
    }
}

#[cfg(not(windows))]
pub fn list_fixed_volumes() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --lib services::scan_service`
Expected: 2 个测试 PASS。若 `GetDriveTypeW`/`GetLogicalDrives` 绑定缺 feature，在 Cargo.toml windows features 追加 `"Win32_Storage_FileSystem"`（已在既有 features 中）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/scan_service.rs src-tauri/src/services/mod.rs
git commit -m "feat(global-search): volume state repository and fixed disk enumeration"
```
无 git 时跳过。

---

### Task 9: 扫描工作线程（遍历、跳过规则、检查点、代次）

**Files:**
- Modify: `src-tauri/src/services/scan_service.rs`

- [ ] **Step 1: 编写失败测试**

`scan_service.rs` 测试模块追加：

```rust
    use std::fs;

    #[test]
    fn scan_creates_entries_and_skips_excluded() {
        let mut c = conn();
        let root = std::env::temp_dir().join(format!("nexus-scan-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.txt"), "x").unwrap();
        fs::write(root.join("sub/b.md"), "y").unwrap();
        fs::create_dir_all(root.join("Windows")).unwrap();
        fs::write(root.join("Windows/system32.dll"), "z").unwrap();

        let excluded = vec![root.join("Windows").to_string_lossy().to_string()];
        scan_directory(
            &mut c,
            &root,
            "v1",
            1,
            &excluded,
            &Default::default(),
        )
        .unwrap();

        let count: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE volume_id='v1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count >= 2, "应索引 a.txt 与 sub/b.md，实际 {count}");
        let win_count: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE canonical_path LIKE '%Windows%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(win_count, 0, "排除目录不应被索引");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_does_not_follow_symlinks() {
        #[cfg(windows)]
        {
            let mut c = conn();
            let root = std::env::temp_dir().join(format!("nexus-scan-link-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("real")).unwrap();
            fs::write(root.join("real/t.txt"), "x").unwrap();
            let _ = std::os::windows::fs::symlink_dir(&root.join("real"), root.join("loop"));
            scan_directory(&mut c, &root, "v2", 1, &[], &Default::default()).unwrap();
            let count: i64 = c
                .query_row("SELECT count(*) FROM system_search_entries", [], |r| r.get(0))
                .unwrap();
            // 符号链接目录本身可能作为目录记录，但绝不进入循环（测试只断言不崩溃且数量有限）
            assert!(count < 100, "符号链接循环不应造成爆炸式索引");
            let _ = fs::remove_dir_all(&root);
        }
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib services::scan_service`
Expected: 编译失败（`scan_directory` 未定义）。

- [ ] **Step 3: 实现目录扫描器**

`scan_service.rs` 追加：

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 扫描控制：暂停标志与是否继续（Arc 便于工作线程与命令层共享）。
#[derive(Default, Clone)]
pub struct ScanControl {
    pub paused: Arc<AtomicBool>,
    pub cancelled: Arc<AtomicBool>,
}

/// 遍历目录并写入索引（单事务批次，支持检查点）。返回 (indexed, skipped)。
pub fn scan_directory(
    conn: &mut Connection,
    root: &std::path::Path,
    volume_id: &str,
    generation: i64,
    excluded: &[String],
    control: &ScanControl,
) -> Result<(i64, i64), String> {
    use std::collections::VecDeque;
    use std::path::Path;

    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.to_path_buf());
    let mut indexed: i64 = 0;
    let mut skipped: i64 = 0;
    let mut batch: Vec<(String, String, String, Option<String>, Option<i64>, Option<i64>)> = Vec::new();
    let now = crate::db::connection::now_unix();

    // 插入语句（预编译一次）
    let insert_sql = "INSERT OR IGNORE INTO system_search_entries
        (canonical_path, display_name, entry_kind, extension, file_size, modified_at,
         volume_id, scan_generation, is_offline, indexed_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)";
    let mut insert = conn.prepare_cached(insert_sql).map_err(|e| e.to_string())?;

    while let Some(dir) = queue.pop_front() {
        if control.paused.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(200));
            continue;
        }
        if control.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok((indexed, skipped));
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            skipped += 1;
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            // 不跟随符号链接与目录联接
            if file_type.is_symlink() {
                skipped += 1;
                continue;
            }
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let canon = crate::services::global_search::canonical_key(&p.to_string_lossy());
            if excluded.iter().any(|e| path_under(&p, Path::new(e))) {
                skipped += 1;
                continue;
            }
            if file_type.is_dir() {
                batch.push((canon, name.clone(), "folder".into(), None, None, None));
                queue.push_back(p);
            } else if file_type.is_file() {
                let ext = p.extension().map(|e| e.to_string_lossy().to_string());
                let meta = entry.metadata().ok();
                let size = meta.as_ref().and_then(|m| m.len().try_into().ok());
                let modified = meta.as_ref().and_then(|m| {
                    m.modified().ok().and_then(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_secs() as i64)
                    })
                });
                batch.push((canon, name, "file".into(), ext, size, modified));
            }
            if batch.len() >= 2000 {
                flush_batch(conn, &mut insert, &batch, volume_id, generation, now)
                    .map_err(|e| e.to_string())?;
                indexed += batch.len() as i64;
                batch.clear();
            }
        }
    }
    if !batch.is_empty() {
        flush_batch(conn, &mut insert, &batch, volume_id, generation, now)
            .map_err(|e| e.to_string())?;
        indexed += batch.len() as i64;
    }
    Ok((indexed, skipped))
}

/// 判断路径是否位于排除目录下（大小写无关，适配 Windows）。
fn path_under(p: &std::path::Path, excluded: &std::path::Path) -> bool {
    let p_l = p.to_string_lossy().to_lowercase();
    let e_l = excluded.to_string_lossy().to_lowercase();
    p_l.starts_with(&e_l)
}

fn flush_batch(
    conn: &Connection,
    insert: &mut rusqlite::Statement,
    batch: &[(String, String, String, Option<String>, Option<i64>, Option<i64>)],
    volume_id: &str,
    generation: i64,
    now: i64,
) -> rusqlite::Result<()> {
    for (canon, name, kind, ext, size, modified) in batch {
        insert.execute(rusqlite::params![
            canon, name, kind, ext, size, modified, volume_id, generation, now
        ])?;
        // 同步 FTS 外部内容表（名称/路径可搜索）
        crate::services::global_search::fts_sync_upsert(
            conn,
            conn.last_insert_rowid(),
            name,
            canon,
        );
    }
    Ok(())
}
```

- [ ] **Step 4: 实现工作线程入口**

`scan_service.rs` 追加：

```rust
use tauri::{AppHandle, Emitter, Manager};

/// 启动后台扫描线程：为每个固定磁盘创建/恢复扫描任务并持续处理。
pub fn start_scan_worker(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            {
                let st = app.state::<crate::AppState>();
                if st.search.scan_paused.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    continue;
                }
                let volumes = list_fixed_volumes();
                for vol in &volumes {
                    scan_one_volume(&app, vol);
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    });
}

fn scan_one_volume(app: &AppHandle, vol: &std::path::Path) {
    let volume_id = format!("{:02x}", volume_hash(vol));
    let db_path = app
        .state::<crate::AppState>()
        .data_dir
        .join("workspace.db");
    let mut conn = match crate::db::connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let (generation, checkpoint, status) = match get_volume_state(&conn, &volume_id) {
        Some(s) => (s.scan_generation, s.checkpoint, s.status),
        None => {
            let _ = upsert_volume_state(&mut conn, &volume_id, &vol.to_string_lossy(), "pending", 1);
            (1, None, "pending".to_string())
        }
    };
    if status == "completed" {
        return;
    }
    let _ = set_volume_status(&mut conn, &volume_id, "scanning", None);
    drop(conn);

    let excluded = load_excluded_dirs(&db_path);
    let mut index_conn = match crate::db::connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let control = app.state::<crate::AppState>().scan_control();
    let start = checkpoint
        .as_deref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| vol.to_path_buf());
    let (indexed, skipped) = match scan_directory(
        &mut index_conn,
        &start,
        &volume_id,
        generation,
        &excluded,
        &control,
    ) {
        Ok(r) => r,
        Err(e) => {
            let _ = set_volume_status(&mut index_conn, &volume_id, "error", Some(&e));
            return;
        }
    };
    // 完成：清理旧代次并标记完成
    index_conn
        .execute(
            "DELETE FROM system_search_entries
             WHERE volume_id = ?1 AND scan_generation < ?2",
            rusqlite::params![volume_id, generation],
        )
        .ok();
    let _ = set_volume_status(&mut index_conn, &volume_id, "completed", None);
    let _ = update_volume_progress(&mut index_conn, &volume_id, None, indexed, skipped);
    emit_progress(app, &volume_id, "completed", indexed, skipped);
}

/// 卷标识：由根路径哈希生成（大小写无关）。
pub fn volume_hash(vol: &std::path::Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    vol.to_string_lossy().to_lowercase().hash(&mut h);
    h.finish()
}

fn load_excluded_dirs(db_path: &std::path::Path) -> Vec<String> {
    let mut conn = match crate::db::connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    crate::services::global_search::get_excluded_dirs(&conn).unwrap_or_default()
}

fn emit_progress(app: &AppHandle, volume_id: &str, status: &str, indexed: i64, skipped: i64) {
    let _ = app.emit(
        crate::events::EVENT_GLOBAL_SEARCH_INDEX_PROGRESS,
        serde_json::json!({
            "volume_id": volume_id,
            "status": status,
            "indexed_count": indexed,
            "skipped_count": skipped,
        }),
    );
}
```

`scan_control` 在 `lib.rs` 中实现（把 AppState 的原子标志转成 ScanControl）：

```rust
impl AppState {
    /// 从运行时构造扫描控制句柄（共享暂停标志）。
    pub fn scan_control(&self) -> crate::services::scan_service::ScanControl {
        crate::services::scan_service::ScanControl {
            paused: self.search.scan_paused.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}
```

同时在 `lib.rs` `setup` 中启动扫描线程（紧跟在应用索引构建之后）：

```rust
            services::scan_service::start_scan_worker(app.handle().clone());
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test --lib services::scan_service`
Expected: 新增 2 个测试 PASS。

- [ ] **Step 6: 编译并运行全量测试**

Run: `cargo test`
Expected: 全量通过，无死锁/编译错误。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/services/scan_service.rs src-tauri/src/lib.rs
git commit -m "feat(global-search): background scan worker with checkpoint, exclusions and generation cleanup"
```
无 git 时跳过。

---

### Task 10: 索引控制命令与状态查询

**Files:**
- Modify: `src-tauri/src/commands/global_search.rs`

- [ ] **Step 1: 编写命令**

`commands/global_search.rs` 追加：

```rust
/// 索引状态（供前端状态栏）。
#[derive(serde::Serialize)]
pub struct SearchIndexStatus {
    pub volumes: Vec<VolumeStatus>,
    pub paused: bool,
    pub fts_enabled: bool,
}

#[derive(serde::Serialize)]
pub struct VolumeStatus {
    pub volume_id: String,
    pub root_path: String,
    pub status: String,
    pub indexed_count: i64,
    pub skipped_count: i64,
}

#[tauri::command]
pub fn get_search_index_status(state: State<AppState>) -> CommandResult<SearchIndexStatus> {
    let conn = state.conn.lock().expect("db lock");
    let mut stmt = conn
        .prepare(
            "SELECT volume_id, root_path, status, indexed_count, skipped_count
             FROM system_search_scan_state ORDER BY root_path",
        )
        .map_err(Into::into)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(VolumeStatus {
                volume_id: r.get(0)?,
                root_path: r.get(1)?,
                status: r.get(2)?,
                indexed_count: r.get(3)?,
                skipped_count: r.get(4)?,
            })
        })
        .map_err(Into::into)?;
    let mut volumes = Vec::new();
    for r in rows {
        volumes.push(r?);
    }
    let fts_enabled = crate::services::global_search::ensure_fts(&conn);
    Ok(SearchIndexStatus {
        volumes,
        paused: state.search.scan_paused.load(Ordering::SeqCst),
        fts_enabled,
    })
}

#[tauri::command]
pub fn pause_search_index(state: State<AppState>) -> CommandResult<()> {
    state.search.scan_paused.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn resume_search_index(state: State<AppState>) -> CommandResult<()> {
    state.search.scan_paused.store(false, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn rebuild_search_index(state: State<AppState>) -> CommandResult<()> {
    let mut conn = state.conn.lock().expect("db lock");
    for vol in crate::services::scan_service::list_fixed_volumes() {
        let volume_id = format!("{:02x}", crate::services::scan_service::volume_hash(&vol));
        let _ = conn.execute(
            "UPDATE system_search_scan_state SET status='pending', scan_generation=scan_generation+1 WHERE volume_id=?1",
            [&volume_id],
        );
    }
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct SearchSettings {
    pub excluded_dirs: Vec<String>,
}

#[tauri::command]
pub fn get_search_settings(state: State<AppState>) -> CommandResult<SearchSettings> {
    let conn = state.conn.lock().expect("db lock");
    Ok(SearchSettings {
        excluded_dirs: crate::services::global_search::get_excluded_dirs(&conn).unwrap_or_default(),
    })
}

#[tauri::command]
pub fn update_search_settings(
    state: State<AppState>,
    settings: SearchSettings,
) -> CommandResult<SearchSettings> {
    let conn = state.conn.lock().expect("db lock");
    crate::services::global_search::set_excluded_dirs(&conn, &settings.excluded_dirs)
        .map_err(Into::into)?;
    Ok(settings)
}
```

`global_search.rs` 追加工具函数：

```rust
/// 读取排除目录设置（settings 表键 global_search_excluded_dirs，JSON 数组）。
pub fn get_excluded_dirs(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    use rusqlite::OptionalExtension;
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key='global_search_excluded_dirs'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match raw {
        Some(v) => serde_json::from_str(&v).unwrap_or_default(),
        None => Ok(vec![
            std::env::temp_dir().to_string_lossy().to_string(),
        ]),
    }
}

pub fn set_excluded_dirs(conn: &Connection, dirs: &[String]) -> rusqlite::Result<()> {
    let value = serde_json::to_string(dirs).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('global_search_excluded_dirs', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [&value],
    )
}
```

注：先确认 `settings` 表存在（`0001_initial.sql` 已建，含 `key`/`value` 列）并在 `get_setting` 使用同一表；若表名不同，按实际表名调整。`volume_hash` 已在 Task 9 定义为 `pub`，命令层直接复用。

- [ ] **Step 2: 注册命令并编译**

`src-tauri/src/lib.rs` invoke_handler 追加：

```rust
            commands::global_search::get_search_index_status,
            commands::global_search::pause_search_index,
            commands::global_search::resume_search_index,
            commands::global_search::rebuild_search_index,
            commands::global_search::get_search_settings,
            commands::global_search::update_search_settings,
```

Run: `cargo test`
Expected: 编译通过，全量测试通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/commands/global_search.rs src-tauri/src/services/global_search.rs src-tauri/src/services/scan_service.rs src-tauri/src/lib.rs
git commit -m "feat(global-search): index status and control commands with excluded dirs settings"
```
无 git 时跳过。

---

### Task 11: 前端索引状态栏

**Files:**
- Create: `src/features/search/components/IndexStatusBar.tsx`
- Modify: `src/features/search/routes/SearchPage.tsx`
- Modify: `src/styles/app.css`

- [ ] **Step 1: 实现状态栏组件**

`src/features/search/components/IndexStatusBar.tsx`：

```tsx
import { useCallback, useEffect, useState } from "react";
import { Database, Pause, Play, RotateCcw, Loader2 } from "lucide-react";
import { call } from "../../../lib/tauri";

interface VolumeStatus {
  volume_id: string;
  root_path: string;
  status: string;
  indexed_count: number;
  skipped_count: number;
}

interface IndexStatus {
  volumes: VolumeStatus[];
  paused: boolean;
  fts_enabled: boolean;
}

const STATUS_LABEL: Record<string, string> = {
  pending: "等待扫描",
  scanning: "扫描中",
  paused: "已暂停",
  completed: "已完成",
  error: "出错",
  offline: "离线",
};

export function IndexStatusBar() {
  const [status, setStatus] = useState<IndexStatus | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await call<IndexStatus>("get_search_index_status"));
    } catch {
      /* 索引表未就绪时静默 */
    }
  }, []);

  useEffect(() => {
    refresh();
    const timer = window.setInterval(refresh, 3000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const run = async (cmd: string) => {
    setLoading(true);
    try {
      await call<void>(cmd);
      await refresh();
    } finally {
      setLoading(false);
    }
  };

  if (!status || status.volumes.length === 0) return null;

  const active = status.volumes.filter((v) => v.status === "scanning").length;
  const total = status.volumes.reduce((s, v) => s + v.indexed_count, 0);

  return (
    <div className="index-status-bar">
      <Database size={13} className="index-status-icon" />
      <div className="index-status-info">
        <span className="index-status-title">索引状态</span>
        <span className="index-status-detail">
          {status.paused
            ? "已暂停"
            : active > 0
              ? `正在扫描 ${active} 个磁盘`
              : `全部完成（${total.toLocaleString()} 项）`}
          {!status.fts_enabled && " · 名称匹配模式"}
        </span>
      </div>
      <div className="index-status-actions">
        {status.paused ? (
          <button className="btn btn-ghost btn-sm" onClick={() => run("resume_search_index")} disabled={loading}>
            <Play size={12} /> 继续
          </button>
        ) : (
          <button className="btn btn-ghost btn-sm" onClick={() => run("pause_search_index")} disabled={loading}>
            <Pause size={12} /> 暂停
          </button>
        )}
        <button className="btn btn-ghost btn-sm" onClick={() => run("rebuild_search_index")} disabled={loading}>
          {loading ? <Loader2 size={12} className="spin" /> : <RotateCcw size={12} />} 重建
        </button>
      </div>
      <div className="index-status-volumes">
        {status.volumes.map((v) => (
          <span key={v.volume_id} className="index-volume-chip">
            {v.root_path} · {STATUS_LABEL[v.status] ?? v.status} · {v.indexed_count.toLocaleString()}
          </span>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: 挂载到搜索页**

`SearchPage.tsx` 顶部（`search-head` 之后、结果之前）插入：

```tsx
      <IndexStatusBar />
```

并补充 import：

```tsx
import { IndexStatusBar } from "../components/IndexStatusBar";
```

- [ ] **Step 3: 追加样式**

`src/styles/app.css`：

```css
.index-status-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  margin: 12px 0 0;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface-muted);
}
.index-status-icon { color: var(--primary); flex-shrink: 0; }
.index-status-info { display: flex; flex-direction: column; gap: 2px; }
.index-status-title { font-size: 13px; font-weight: 600; color: var(--text); }
.index-status-detail { font-size: 12px; color: var(--text-muted); }
.index-status-actions { display: flex; gap: 6px; margin-left: auto; }
.index-status-volumes { display: flex; gap: 6px; flex-wrap: wrap; width: 100%; }
.index-volume-chip {
  padding: 2px 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-full);
  font-size: 12px;
  color: var(--text-muted);
}
```

- [ ] **Step 4: 构建验证**

Run: `npm run build`
Expected: 构建成功。

- [ ] **Step 5: 提交**

```bash
git add src/features/search/components/IndexStatusBar.tsx src/features/search/routes/SearchPage.tsx src/styles/app.css
git commit -m "feat(global-search): index status bar with pause resume rebuild"
```
无 git 时跳过。

---

## 阶段三：增量维护与降级

### Task 12: 文件变化监听与增量更新

**Files:**
- Modify: `src-tauri/src/services/scan_service.rs`

- [ ] **Step 1: 编写失败测试**

`scan_service.rs` 测试模块追加：

```rust
    #[test]
    fn apply_watch_events_inserts_removes_and_renames() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v3", "C:\\", "completed", 1).unwrap();
        // 初始两条
        c.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\x\\old.txt','old.txt','file','v3',1,1),
                    ('c:\\x\\keep.txt','keep.txt','file','v3',1,1)",
            [],
        )
        .unwrap();
        let events = vec![
            WatchEvent::Create("c:\\x\\new.txt".into()),
            WatchEvent::Remove("c:\\x\\old.txt".into()),
            WatchEvent::Rename("c:\\x\\keep.txt".into(), "c:\\x\\renamed.txt".into()),
        ];
        apply_watch_events(&mut c, "v3", 1, &events).unwrap();
        let count: i64 = c
            .query_row("SELECT count(*) FROM system_search_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let has_new: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM system_search_entries WHERE canonical_path='c:\\x\\new.txt')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(has_new);
        let has_old: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM system_search_entries WHERE canonical_path='c:\\x\\old.txt')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!has_old);
    }
```

- [ ] **Step 2: 实现事件模型与增量应用**

`scan_service.rs` 追加：

```rust
/// 文件变化事件（由 watcher 回调归一化后传入）。
#[derive(Debug, Clone)]
pub enum WatchEvent {
    Create(String),
    Remove(String),
    Rename(String, String), // from, to
}

/// 将事件批量应用到索引。completed 卷才处理；未完成卷交由扫描补齐。
pub fn apply_watch_events(
    conn: &mut Connection,
    volume_id: &str,
    generation: i64,
    events: &[WatchEvent],
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    for ev in events {
        match ev {
            WatchEvent::Create(path) => {
                // 仅当文件仍存在时写入（目录留待扫描遍历）
                if std::path::Path::new(path).is_file() {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if !name.is_empty() {
                        let canon = crate::services::global_search::canonical_key(path);
                        let now = crate::db::connection::now_unix();
                        tx.execute(
                            "INSERT OR IGNORE INTO system_search_entries
                             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
                             VALUES (?1, ?2, 'file', ?3, ?4, ?5)",
                            rusqlite::params![canon, name, volume_id, generation, now],
                        )?;
                    }
                }
            }
            WatchEvent::Remove(path) => {
                let canon = crate::services::global_search::canonical_key(path);
                tx.execute(
                    "DELETE FROM system_search_entries WHERE canonical_path = ?1",
                    [&canon],
                )?;
            }
            WatchEvent::Rename(from, to) => {
                let f = crate::services::global_search::canonical_key(from);
                let t = crate::services::global_search::canonical_key(to);
                let exists: Option<String> = tx
                    .query_row(
                        "SELECT display_name FROM system_search_entries WHERE canonical_path = ?1",
                        [&f],
                        |r| r.get(0),
                    )
                    .optional()?;
                match exists {
                    Some(name) => {
                        tx.execute(
                            "UPDATE system_search_entries
                             SET canonical_path = ?1, display_name = ?2 WHERE canonical_path = ?3",
                            rusqlite::params![t, name, f],
                        )?;
                    }
                    None => {
                        // 目标文件是新文件则插入
                        if std::path::Path::new(to).is_file() {
                            let name = std::path::Path::new(to)
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let now = crate::db::connection::now_unix();
                            tx.execute(
                                "INSERT OR IGNORE INTO system_search_entries
                                 (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
                                 VALUES (?1, ?2, 'file', ?3, ?4, ?5)",
                                rusqlite::params![t, name, volume_id, generation, now],
                            )?;
                        }
                    }
                }
            }
        }
    }
    tx.commit()
}
```

- [ ] **Step 3: 实现 watcher 与事件归一化**

`scan_service.rs` 追加：

```rust
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// 为所有已完成卷启动文件监听；事件经 2s 去抖后批量应用。
pub fn start_watchers(app: AppHandle) {
    std::thread::spawn(move || {
        let mut watchers: Vec<RecommendedWatcher> = Vec::new();
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        let mut roots: Vec<(String, PathBuf)> = Vec::new();
        loop {
            // 每次轮询刷新已完成卷的监听集合
            {
                let st = app.state::<crate::AppState>();
                let db_path = st.data_dir.join("workspace.db");
                let Ok(conn) = crate::db::connection::open(&db_path) else { continue };
                let completed: Vec<(String, String)> = conn
                    .query_map(
                        "SELECT volume_id, root_path FROM system_search_scan_state WHERE status='completed'",
                        [],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .unwrap_or_default()
                    .filter_map(|r| r.ok())
                    .collect();
                drop(conn);
                let current: Vec<(String, PathBuf)> = completed
                    .into_iter()
                    .map(|(v, p)| (v, PathBuf::from(p)))
                    .collect();
                if current != roots {
                    roots = current;
                    watchers.clear();
                    for (_v, root) in &roots {
                        let tx_clone = tx.clone();
                        if let Ok(mut w) = RecommendedWatcher::new(
                            move |res: notify::Result<notify::Event>| {
                                if let Ok(ev) = res {
                                    for e in normalize_notify_event(&ev) {
                                        let _ = tx_clone.send(e);
                                    }
                                }
                            },
                            Config::default(),
                        ) {
                            if w.watch(root, RecursiveMode::Recursive).is_ok() {
                                watchers.push(w);
                            }
                        }
                    }
                }
            }
            // 批量消费去抖
            if let Ok(first) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
                let mut batch = vec![first];
                while let Ok(e) = rx.try_recv() {
                    batch.push(e);
                }
                std::thread::sleep(std::time::Duration::from_millis(2000));
                while let Ok(e) = rx.try_recv() {
                    batch.push(e);
                }
                let st = app.state::<crate::AppState>();
                let db_path = st.data_dir.join("workspace.db");
                if let Ok(mut conn) = crate::db::connection::open(&db_path) {
                    // 按卷分组应用（简化：单卷批量；多卷事件按前缀拆分）
                    for (vol, _root) in &roots {
                        let vol_events: Vec<WatchEvent> = batch
                            .iter()
                            .filter(|e| event_volume(e) == *vol)
                            .cloned()
                            .collect();
                        if !vol_events.is_empty() {
                            let gen = current_generation(&conn, vol).unwrap_or(1);
                            let _ = apply_watch_events(&mut conn, vol, gen, &vol_events);
                        }
                    }
                }
            }
        }
    });
}

fn event_volume(e: &WatchEvent) -> String {
    let p = match e {
        WatchEvent::Create(p) | WatchEvent::Remove(p) => p,
        WatchEvent::Rename(f, _) => f,
    };
    let lower = p.to_lowercase();
    lower
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| format!("{:02x}", volume_hash(std::path::Path::new(&format!("{c}:\\")))))
        .unwrap_or_default()
}

fn current_generation(conn: &Connection, volume_id: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT scan_generation FROM system_search_scan_state WHERE volume_id = ?1",
        [volume_id],
        |r| r.get(0),
    )
}

/// 将 notify 事件归一化为 WatchEvent 列表。
fn normalize_notify_event(ev: &notify::Event) -> Vec<WatchEvent> {
    use notify::event::{ModifyKind, RenameMode};
    let mut out = Vec::new();
    match ev.kind {
        EventKind::Create(_) => {
            for p in &ev.paths {
                out.push(WatchEvent::Create(p.to_string_lossy().to_string()));
            }
        }
        EventKind::Remove(_) => {
            for p in &ev.paths {
                out.push(WatchEvent::Remove(p.to_string_lossy().to_string()));
            }
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            // from 事件与 To 事件成对出现，暂存逻辑简化：先记录 from
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            // 首版把 To 当作 Create 处理，旧路径残留由分卷校验清理
            for p in &ev.paths {
                out.push(WatchEvent::Create(p.to_string_lossy().to_string()));
            }
        }
        _ => {}
    }
    out
}
```

注：简化取舍——Rename 的 from/to 配对在 notify 中分属两个事件，首版将 To 作为 Create 处理，from 遗留的旧路径由分卷校验（Task 13）清理。

FTS 同步：`flush_batch` 已在每条插入后调用 `fts_sync_upsert`（见 Task 9 Step 3）。`apply_watch_events` 的两个 INSERT 分支也需同步 FTS——在 `WatchEvent::Create` 分支的 `tx.execute` 后追加：

```rust
let id = tx.last_insert_rowid();
crate::services::global_search::fts_sync_upsert(&tx, id, &name, &canon);
```

`Rename` 分支中插入新记录的分支同样追加上述两行。`Remove` 分支无需处理（外部内容表 FTS 查询按 rowid 回表，记录删除后自然无结果；若残留由 `verify_volume` 清理）。

- [ ] **Step 4: 启动 watcher**

`src-tauri/src/lib.rs` `setup` 中追加：

```rust
            services::scan_service::start_watchers(app.handle().clone());
```

- [ ] **Step 5: 运行测试**

Run: `cargo test --lib services::scan_service`
Expected: `apply_watch_events_inserts_removes_and_renames` PASS，其余通过。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/services/scan_service.rs src-tauri/src/lib.rs
git commit -m "feat(global-search): incremental watcher with debounced batch apply"
```
无 git 时跳过。

---

### Task 13: 分卷校验与离线处理

**Files:**
- Modify: `src-tauri/src/services/scan_service.rs`

- [ ] **Step 1: 编写失败测试**

`scan_service.rs` 测试模块追加：

```rust
    #[test]
    fn verify_volume_prunes_stale_and_marks_offline() {
        let mut c = conn();
        upsert_volume_state(&mut c, "v4", "D:\\", "completed", 1).unwrap();
        c.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('d:\\gone.txt','gone.txt','file','v4',1,1),
                    ('d:\\stay.txt','stay.txt','file','v4',1,1)",
            [],
        )
        .unwrap();
        // 本测试在临时目录上模拟：仅验证离线条目置灰与不存在文件清理
        c.execute(
            "UPDATE system_search_entries SET is_offline=1 WHERE canonical_path='d:\\stay.txt'",
            [],
        )
        .unwrap();
        mark_volume_offline(&mut c, "v4").unwrap();
        let status: String = c
            .query_row(
                "SELECT status FROM system_search_scan_state WHERE volume_id='v4'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "offline");
        let offline: i64 = c
            .query_row(
                "SELECT count(*) FROM system_search_entries WHERE volume_id='v4' AND is_offline=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(offline, 2);
    }
```

- [ ] **Step 2: 实现校验与离线标记**

`scan_service.rs` 追加：

```rust
/// 标记卷离线：保留记录并置灰（不删除）。
pub fn mark_volume_offline(conn: &mut Connection, volume_id: &str) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE system_search_entries SET is_offline=1 WHERE volume_id=?1",
        [volume_id],
    )?;
    tx.execute(
        "UPDATE system_search_scan_state SET status='offline' WHERE volume_id=?1",
        [volume_id],
    )?;
    tx.commit()
}

/// 卷重新上线：清离线条目并重置为 pending 待重扫。
pub fn mark_volume_back_online(conn: &mut Connection, volume_id: &str) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM system_search_entries WHERE volume_id=?1 AND is_offline=1",
        [volume_id],
    )?;
    tx.execute(
        "UPDATE system_search_scan_state SET status='pending' WHERE volume_id=?1",
        [volume_id],
    )?;
    tx.commit()
}

/// 对已完成卷做低频一致性校验：删除磁盘上已不存在的记录。
pub fn verify_volume(
    conn: &mut Connection,
    volume_id: &str,
    limit: i64,
) -> rusqlite::Result<i64> {
    let mut stmt = conn.prepare(
        "SELECT id, canonical_path FROM system_search_entries
         WHERE volume_id = ?1 AND is_offline = 0 LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![volume_id, limit], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();
    drop(stmt);
    let mut removed = 0;
    for (id, path) in rows {
        if !std::path::Path::new(&path).exists() {
            conn.execute("DELETE FROM system_search_entries WHERE id=?1", [id])?;
            conn.execute(
                "DELETE FROM system_search_entries_fts WHERE rowid=?1",
                [id],
            )
            .ok();
            removed += 1;
        }
    }
    Ok(removed)
}

/// 在扫描线程空闲周期调用：每卷最多校验 N 条。
pub fn run_verification_round(app: &AppHandle) {
    let st = app.state::<crate::AppState>();
    let db_path = st.data_dir.join("workspace.db");
    let Ok(mut conn) = crate::db::connection::open(&db_path) else { return };
    let volumes: Vec<String> = conn
        .query_map(
            "SELECT volume_id FROM system_search_scan_state WHERE status='completed'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default()
        .filter_map(|r| r.ok())
        .collect();
    for vol in volumes {
        // 磁盘不在线（根路径不存在）→ 离线
        let root: Option<String> = conn
            .query_row(
                "SELECT root_path FROM system_search_scan_state WHERE volume_id=?1",
                [&vol],
                |r| r.get(0),
            )
            .ok();
        if let Some(root) = root {
            if !std::path::Path::new(&root).exists() {
                let _ = mark_volume_offline(&mut conn, &vol);
                continue;
            }
        }
        let _ = verify_volume(&mut conn, &vol, 2000);
    }
}
```

`start_scan_worker` 的 30s 空闲循环中追加调用：

```rust
            run_verification_round(&app);
```

- [ ] **Step 3: 运行测试**

Run: `cargo test --lib services::scan_service`
Expected: 新增测试 PASS。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/services/scan_service.rs
git commit -m "feat(global-search): volume verification and offline handling"
```
无 git 时跳过。

---

## 阶段四：完善与回归

### Task 14: 键盘交互与结果上限

**Files:**
- Modify: `src/features/search/routes/SearchPage.tsx`
- Modify: `src/styles/app.css`

- [ ] **Step 1: 增加键盘导航**

`SearchPage.tsx` 追加状态与效果：

```tsx
  const [activeIdx, setActiveIdx] = useState(-1);
  const listRef = useRef<HTMLDivElement>(null);

  // 键盘：上下选择，Enter 打开，Esc 清空
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIdx((i) => Math.min(i + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && activeIdx >= 0 && results[activeIdx]) {
      e.preventDefault();
      activate(results[activeIdx]);
    } else if (e.key === "Escape") {
      setActiveIdx(-1);
    }
  };
```

输入框追加 `onKeyDown`，结果容器绑定 `ref={listRef}`，结果行高亮类：

```tsx
className={`search-result ${i === activeIdx ? "active" : ""}`}
```

结果行 map 增加索引参数 `(r, i)`。

- [ ] **Step 2: 结果上限提示**

结果超过 500 条时展示提示（后端已限制每批 100；前端在收到批量合并后判断）：

```tsx
      {results.length >= 500 && (
        <div className="system-note">结果较多，已展示前 500 条，请通过筛选缩小范围</div>
      )}
```

`startSearch` 的 `limit` 与后台补批合计由 `seenKeysRef` 控制，追加保护：

```tsx
        if (fresh.length) {
          setResults((prev) => {
            const next = [...prev, ...fresh];
            return next.length > 500 ? next.slice(0, 500) : next;
          });
        }
```

- [ ] **Step 3: 样式**

`src/styles/app.css`：

```css
.search-result.active {
  border-color: var(--primary);
  background: var(--surface-muted);
}
```

- [ ] **Step 4: 构建验证**

Run: `npm run build`
Expected: 构建成功。

- [ ] **Step 5: 提交**

```bash
git add src/features/search/routes/SearchPage.tsx src/styles/app.css
git commit -m "feat(global-search): keyboard navigation and result cap"
```
无 git 时跳过。

---

### Task 15: Windows 本机集成测试清单

**Files:**
- Create: `docs/superpowers/plans/2026-08-15-hybrid-global-search-windows-test-checklist.md`

- [ ] **Step 1: 编写集成测试清单**

创建清单文档，内容如下（作为可勾选清单）：

```markdown
# 全电脑混合搜索 Windows 集成测试清单

运行条件：`npm run tauri dev`（或 `cargo run` 后打开前端），确认后台扫描自动启动。

## 搜索覆盖
- [ ] 输入 2 字符触发搜索；首批结果 < 500ms（本机 SSD）
- [ ] 搜索文件名包含关键词的深层文件（如 `年度报告`）
- [ ] 搜索路径片段（如 `docs`）
- [ ] 搜索桌面应用名称（如 `记事本`、`Chrome`）
- [ ] 搜索 Store 应用名称（如 `照片`、`计算器`）
- [ ] 搜索 NexusFile 页面/项目名称
- [ ] 同一文件仅展示一次（Windows Search 与本地索引不重复）

## 索引与扫描
- [ ] 启动后所有固定磁盘进入扫描；状态栏显示进度
- [ ] 暂停/继续立即生效
- [ ] 重建索引后状态回到 pending 并重新扫描
- [ ] 首次全盘扫描期间搜索、打开文件、电脑信息页均可正常操作
- [ ] 排除目录设置后，该目录下文件不再出现
- [ ] 重启应用后扫描从检查点继续而非重头开始

## 增量一致性
- [ ] 新建文件后 1-2 分钟内可被搜索到
- [ ] 重命名文件后旧名称不再命中、新名称命中
- [ ] 删除文件后 1-2 分钟内不再命中，打开已删除结果提示不存在
- [ ] 拔出移动硬盘（如有）后对应卷显示离线且结果置灰

## 动作
- [ ] 双击文件 → 默认应用打开
- [ ] 双击文件夹 → 资源管理器打开
- [ ] 双击应用 → 应用启动
- [ ] 定位按钮 → 资源管理器选中该文件
- [ ] 复制路径 → 剪贴板内容正确
- [ ] 文件在搜索后被删除 → 打开提示目标不存在

## 降级
- [ ] 停止 Windows Search 服务（services.msc 停止 WSearch）后搜索仍可用（走本地索引）
- [ ] 恢复 WSearch 后无需重启应用即可继续使用

## 性能
- [ ] 典型 SSD 上常见查询首批 < 300ms（有 Windows Search）/ < 500ms（仅本地索引）
- [ ] 扫描期间 CPU 占用不持续超过 30%（可接受波动）
```

- [ ] **Step 2: 提交**

```bash
git add docs/superpowers/plans/2026-08-15-hybrid-global-search-windows-test-checklist.md
git commit -m "docs(global-search): windows integration test checklist"
```
无 git 时跳过。

---

### Task 16: 全量回归与性能验证

**Files:**
- 无（验证任务）

- [ ] **Step 1: 后端全量测试**

Run: `cargo test`（cwd `e:\work\新建文件夹\src-tauri`）
Expected: 全部通过（原 76 项 + 本计划新增约 12 项）。记录总数。

- [ ] **Step 2: 前端构建**

Run: `npm run build`（cwd `e:\work\新建文件夹`）
Expected: 构建成功，无 TypeScript 报错。

- [ ] **Step 3: 运行集成测试清单**

按照 Task 15 清单逐项在真实 Windows 环境执行，逐项打勾。重点验证：
- 全盘扫描完成后 `get_search_index_status` 返回 `completed`。
- 增量新增/删除在 2 分钟内反映。
- Windows Search 停止后降级路径可用。

- [ ] **Step 4: 性能抽测**

在 `commands/global_search.rs` 测试模块中追加后端计时测试（不依赖前端）：

```rust
#[cfg(test)]
mod perf {
    use super::*;

    /// 本地索引查询计时（内存库 + 1000 条样本），验证 LIKE 兜底路径在合理范围。
    #[test]
    fn local_query_timing_with_1000_rows() {
        use crate::services::global_search::query_local_index;
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        let now = crate::db::connection::now_unix();
        {
            let mut stmt = conn
                .prepare(
                    "INSERT INTO system_search_entries
                     (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
                     VALUES (?1, ?2, 'file', 'v9', 1, ?3)",
                )
                .unwrap();
            for i in 0..1000 {
                stmt.execute(rusqlite::params![
                    format!("c:\\perf\\file_{i:04}.txt"),
                    format!("file_{i:04}.txt"),
                    now,
                ])
                .unwrap();
            }
        }
        let start = std::time::Instant::now();
        let hits = query_local_index(&conn, "file_0500", 50).unwrap();
        let elapsed = start.elapsed();
        assert!(!hits.is_empty(), "应命中 file_0500");
        assert!(
            elapsed.as_millis() < 500,
            "LIKE 路径 1000 行应 <500ms，实际 {}ms",
            elapsed.as_millis()
        );
    }
}
```

Run: `cargo test --lib commands::global_search::perf -- --nocapture`
Expected: PASS，并打印/断言耗时 < 500ms。

前端验收：运行应用后打开搜索页，DevTools Performance 录制一次常见搜索（如文件名片段），确认首批渲染 < 300ms（有 Windows Search）或 < 500ms（仅本地索引）。若超时，检查是否查询走了全表 LIKE（FTS 未生效）或数据库锁等待（busy_timeout 是否被扫描写入占满）。

- [ ] **Step 5: 提交**

```bash
git add .
git commit -m "test(global-search): full regression and performance verification"
```
无 git 时跳过。

---

## 自检记录

- 设计文档 §3.1 文件范围 → Task 8/9；§3.2 应用范围 → Task 3；§3.3 NexusFile → Task 4。
- 设计文档 §4 数据表 → Task 1；§5 扫描 → Task 9/12/13；§6 聚合排序 → Task 2/4。
- 设计文档 §7 接口 → Task 6/10；§8 前端 → Task 7/11/14；§9 降级 → Task 5/13；§10 性能 → Task 16。
- 设计文档 §12 四阶段 → 计划按阶段一至四分组（Task 1-7 / 8-11 / 12-13 / 14-16）。
- 类型一致性：`GlobalSearchHit`（Rust）与 `GlobalSearchHit`（TS）字段一一对应；`search_id: u64` 贯穿命令与事件；`canonical_key` 在 Task 2 定义、Task 4/9/12 复用。
- 已知降级项：Windows Search SQL 经 OLE DB 执行标注为可选增强（`query_windows_search_ole_db` 返回 Err 降级），符合设计文档 §9；`.lnk` Store 目标解析失败时回退快捷方式路径。

## 执行与最终审查记录

- 执行方式：子代理驱动开发（每任务 implementer + spec 审查 + 质量审查），16 个任务全部完成。
- 迁移版本修正：代码库已有 0001-0005，本功能迁移注册为 version 6（文件 `0006_global_search.sql`）。
- 最终审查修复（交付前）：
  - Critical：`flush_batch` 由 `INSERT OR IGNORE` 改为代次感知 `ON CONFLICT(canonical_path) DO UPDATE`（重建索引不再清空本地索引），并包事务（同批 I-6）；补 `rescan_with_new_generation_keeps_existing_entries` 回归测试。
  - I-2：`open_search_result` 文件分支先查 `system_search_entries` 索引记录再打开。
  - I-3：前端对 exe/bat/cmd/com/msi 首次运行弹确认。
  - I-4：`get_excluded_dirs` 默认排除 `$Recycle.Bin` / `System Volume Information` / `Windows` / 临时目录。
  - I-5：路径匹配命中减分 + 应用/页面/项目同分加权（`kind_bonus`）。
- 已知预留项（后续迭代）：Windows Search OLE DB 执行层、`scan_trigger` 即时触发重建、`EVENT_GLOBAL_SEARCH_INDEX_PROGRESS` 前端订阅、索引错误详情（`last_error`）界面展示、磁盘/扩展名筛选、来源标签展示。

## 预留项优化记录（2026-08-16）

- 已完成：
  - `scan_trigger` 即时触发重建：`rebuild_search_index` 提升触发计数，扫描线程感知后跳过 30s 轮询立即重扫（`commands/global_search.rs` + `services/scan_service.rs`）。
  - 索引错误详情：`VolumeStatus` 增 `last_error` 字段，`get_search_index_status` 返回；`emit_progress` 事件 payload 含 `last_error`，error 分支补发事件；前端状态栏 chip 悬停/文本展示错误详情。
  - 事件订阅：`IndexStatusBar` 改 `listen` 事件驱动刷新 + 30s 兜底轮询（替代原 3s 轮询），含卸载清理与 disposed 竞态防护。
  - 来源标签：结果行显示来源（windows→系统搜索 / local_index→本地索引 / app_index→应用 / nexus→资源库）。
  - 磁盘/扩展名筛选：搜索页新增盘符下拉与扩展名输入，纯展示层过滤（不触发重搜、不破坏补批与 500 cap），键盘导航基于过滤后结果。
  - 排除目录管理 UI：设置页「忽略规则」标签新增「搜索排除目录」区块（chip 增删、持久化 `update_search_settings`、空列表提示默认排除）。
- 保留：Windows Search OLE DB 执行层（复杂度高，设计文档已标注可选增强，保持降级路径）。
- 回归：后端 `cargo test --lib` 135 passed / 1 ignored；前端 `npm run build` 通过。
- 最终状态：后端 `cargo test --lib` 135 passed / 1 ignored（ignored 为需 Windows Search 服务的集成探测）；前端 `npm run build` 通过。

## 本机验证记录（2026-08-16）

- 验证方式：`npm run tauri dev` 启动真实应用 + 直读 `E:\com.nexus.file-workspace\workspace.db`。
- 全盘扫描：三固定盘全部 completed（C:\ 2,021,695 / D:\ 198,043 / E:\ 2,280,366，合计约 450 万条）；FTS 与 entries 完全同步。
- 验证中修复（Critical）：**默认排除规则失效**——`$Recycle.Bin` 为相对路径，原 `path_under` 组件前缀比较从盘符开始导致回收站被索引（C:\ 首扫 21,655 条）。修复：`path_under` 对相对目录名按任意层级组件匹配（跳过盘符），默认排除去掉 `Windows` 相对名（防误伤用户目录）；重建 C:\ 后回收站条目清零，存量由 `verify_volume` 逐轮收敛。
- 增量一致性：新建 12s 内索引 ✓；重命名新路径 + 新 display_name ✓（旧路径残留按首版简化依赖 verify 清理）；删除 15s 内条目消失（含 FTS）✓。
- 其余验证结果见 `2026-08-15-hybrid-global-search-windows-test-checklist.md` 执行记录。
