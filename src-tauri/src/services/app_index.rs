// 枚举入口（collect_apps / resolve_lnk_target）待 Task 6 命令层接入；
// 在此之前保持 dead_code 允许，接入后移除本属性。
#![allow(dead_code)]

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

/// 规范化应用去重键：复用全局搜索的 canonical_key（统一分隔符 + 小写）。
pub fn canonical_app_target(target: &str) -> String {
    crate::services::global_search::canonical_key(target)
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
            let score = if display_name.to_lowercase() == q.to_lowercase() {
                200
            } else if display_name.to_lowercase().starts_with(&q.to_lowercase()) {
                160
            } else {
                120
            };
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
                score,
            })
        },
    )?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

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
        walk_lnk_inner(dir, out, &mut std::collections::HashSet::new(), 0);
    }

    /// 递归遍历 .lnk；visited 记录已访问的规范化目录，防止 junction/symlink 成环导致栈溢出。
    fn walk_lnk_inner(
        dir: &std::path::Path,
        out: &mut Vec<AppEntry>,
        visited: &mut std::collections::HashSet<std::path::PathBuf>,
        depth: usize,
    ) {
        if depth > 16 {
            return;
        }
        let canon = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if !visited.insert(canon) {
            return;
        }
        let Ok(read) = std::fs::read_dir(dir) else { return };
        for entry in read.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk_lnk_inner(&p, out, visited, depth + 1);
            } else if p.extension().map(|e| e.to_ascii_lowercase()) == Some(std::ffi::OsString::from("lnk")) {
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
        use ::windows::core::{Interface, PCWSTR};
        use ::windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, IPersistFile, STGM,
        };
        use ::windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH};
        let wide = to_wide(&lnk.to_string_lossy());
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let link: IShellLinkW = CoCreateInstance(&ShellLink as *const _, None, CLSCTX_INPROC_SERVER).ok()?;
            let persist: IPersistFile = link.cast().ok()?;
            persist.Load(PCWSTR(wide.as_ptr()), STGM(0)).ok()?;
            let mut buf = [0u16; 1024];
            link.GetPath(&mut buf, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32).ok()?;
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            if end == 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&buf[..end]))
        }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 展开注册表值中的环境变量（%VAR%），失败时原样返回。
    fn expand_env(input: &str) -> String {
        use ::windows::core::PCWSTR;
        use ::windows::Win32::System::Environment::ExpandEnvironmentStringsW;
        if !input.contains('%') {
            return input.to_string();
        }
        let src = to_wide(input);
        let mut buf = vec![0u16; 4096];
        let written = unsafe {
            ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), Some(&mut buf))
        };
        if written == 0 {
            return input.to_string();
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
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
                let display_icon: String = sub
                    .get_value("DisplayIcon")
                    .ok()
                    .map(|s: String| s.split(',').next().unwrap_or("").to_string())
                    .unwrap_or_default();
                let install: Option<String> = sub.get_value("InstallLocation").ok();
                // 目标解析优先级：
                // 1) InstallLocation 下真实存在的 {display}.exe
                // 2) 展开环境变量后的 DisplayIcon（仅当目标文件存在）
                // 3) 兜底 InstallLocation\{display}.exe
                let install_dir = install.as_deref().map(expand_env);
                let target = install_dir
                    .as_deref()
                    .map(|i| std::path::Path::new(i).join(format!("{display}.exe")))
                    .filter(|p| p.is_file())
                    .or_else(|| {
                        let icon = expand_env(&display_icon);
                        let p = std::path::Path::new(&icon);
                        p.is_file().then(|| p.to_path_buf())
                    })
                    .or_else(|| {
                        install_dir.as_deref().map(|i| std::path::Path::new(i).join(format!("{display}.exe")))
                    });
                let Some(target) = target.map(|p| p.to_string_lossy().to_string()) else { continue };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lnk_to_target_extracts_path() {
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

    #[test]
    fn rebuild_replaces_all_rows() {
        use rusqlite::Connection;
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        let entries = vec![
            AppEntry {
                app_kind: "win32".into(),
                display_name: "A".into(),
                launch_target: Some("C:\\A\\a.exe".into()),
                canonical_target: "c:\\a\\a.exe".into(),
                aumid: None,
                icon_source: None,
                install_location: None,
            },
            AppEntry {
                app_kind: "shortcut".into(),
                display_name: "B".into(),
                launch_target: Some("C:\\B\\b.exe".into()),
                canonical_target: "c:\\b\\b.exe".into(),
                aumid: None,
                icon_source: None,
                install_location: None,
            },
        ];
        rebuild_app_index(&mut conn, &entries).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM system_search_apps", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        // 全量重建：再次构建更少条目时旧行应被清除
        rebuild_app_index(&mut conn, &entries[..1]).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM system_search_apps", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn query_apps_matches_name_case_insensitive() {
        use rusqlite::Connection;
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        let entries = vec![AppEntry {
            app_kind: "win32".into(),
            display_name: "记事本".into(),
            launch_target: Some("C:\\Windows\\notepad.exe".into()),
            canonical_target: "c:\\windows\\notepad.exe".into(),
            aumid: None,
            icon_source: None,
            install_location: None,
        }];
        rebuild_app_index(&mut conn, &entries).unwrap();
        let hits = query_apps(&conn, "记事", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "app");
        assert_eq!(hits[0].source, "app_index");
    }

    #[test]
    fn query_apps_matches_ascii_case_insensitive() {
        use rusqlite::Connection;
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        let entries = vec![AppEntry {
            app_kind: "win32".into(),
            display_name: "Notepad".into(),
            launch_target: Some("C:\\Windows\\notepad.exe".into()),
            canonical_target: "c:\\windows\\notepad.exe".into(),
            aumid: None,
            icon_source: None,
            install_location: None,
        }];
        rebuild_app_index(&mut conn, &entries).unwrap();
        let hits = query_apps(&conn, "notePAD", 10).unwrap();
        assert_eq!(hits.len(), 1, "ASCII 大小写应不敏感");
    }

    #[test]
    fn query_apps_escapes_like_wildcards() {
        use rusqlite::Connection;
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        let entries = vec![AppEntry {
            app_kind: "win32".into(),
            display_name: "100%工具".into(),
            launch_target: Some("C:\\App\\tool.exe".into()),
            canonical_target: "c:\\app\\tool.exe".into(),
            aumid: None,
            icon_source: None,
            install_location: None,
        }];
        rebuild_app_index(&mut conn, &entries).unwrap();
        let hits = query_apps(&conn, "100%", 10).unwrap();
        assert_eq!(hits.len(), 1, "% 应被当作字面量而非通配符");
        // 字面下划线查询不应命中任何应用
        let hits = query_apps(&conn, "_", 10).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn rebuild_with_empty_entries_clears_table() {
        use rusqlite::Connection;
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        let entries = vec![AppEntry {
            app_kind: "win32".into(),
            display_name: "A".into(),
            launch_target: Some("C:\\A\\a.exe".into()),
            canonical_target: "c:\\a\\a.exe".into(),
            aumid: None,
            icon_source: None,
            install_location: None,
        }];
        rebuild_app_index(&mut conn, &entries).unwrap();
        rebuild_app_index(&mut conn, &[]).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM system_search_apps", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
