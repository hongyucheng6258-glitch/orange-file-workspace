//! Windows Search 即时层。
//!
//! 通过 COM 绑定 Windows Search 索引（`CSearchManager` → `SystemIndex` catalog →
//! `ISearchQueryHelper`），把用户 AQS 查询编译为结构化 SQL。
//!
//! 降级策略（分层）：
//! 1. COM 绑定不可用（如本机未启用 Search 服务）→ 返回 `Err`，调用方降级到本地索引层；
//! 2. SQL 执行依赖 OLE DB（`Search.CollatorDSO` provider），windows crate 未提供
//!    完整 OLE DB 绑定时 `query_windows_search_ole_db` 返回 `Err`，同样触发降级。
//!
//! 首版按"探测 + 优雅降级"实现：AQS→SQL 的编译链路走通即证明索引可达，
//! 结果查询留待后续按 OLE DB provider 文档补齐（约 300 行 COM 调用）。

use crate::services::global_search::GlobalSearchHit;

/// Windows Search 即时查询。COM 绑定不可用时返回 Err，由调用方降级。
#[cfg(windows)]
pub fn search(query: &str, limit: u32) -> Result<Vec<GlobalSearchHit>, String> {
    use ::windows::core::{PCWSTR, PWSTR};
    use ::windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED,
    };
    use ::windows::Win32::System::Search::{CSearchManager, ISearchCatalogManager, ISearchManager};

    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    unsafe {
        // 幂等初始化 COM（多线程套间）；失败不阻断后续调用。
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mgr: ISearchManager =
            CoCreateInstance(&CSearchManager as *const _, None, CLSCTX_LOCAL_SERVER)
                .map_err(|e| format!("CoCreateInstance(SearchManager): {e}"))?;
        let catalog: ISearchCatalogManager = mgr
            .GetCatalog(PCWSTR(wide("SystemIndex").as_ptr()))
            .map_err(|e| format!("GetCatalog: {e}"))?;
        let helper = catalog
            .GetQueryHelper()
            .map_err(|e| format!("GetQueryHelper: {e}"))?;
        // 生成 SQL（AQS → 结构化查询）。0.61 绑定中该方法返回 Result<PWSTR>
        // （COM 分配、需 CoTaskMemFree），而非旧版文档的输出参数形式。
        let sql: PWSTR = helper
            .GenerateSQLFromUserQuery(PCWSTR(wide(query).as_ptr()))
            .map_err(|e| format!("GenerateSQLFromUserQuery: {e}"))?;
        let sql_string = if sql.is_null() {
            String::new()
        } else {
            let s = PCWSTR::from_raw(sql.as_ptr())
                .to_string()
                .unwrap_or_default();
            CoTaskMemFree(Some(sql.as_ptr() as *const _));
            s
        };
        // SQL 通过 OLE DB 执行；绑定不可用时向调用方传播 Err（触发上层降级到本地索引）。
        // 错误串携带探测证据（生成的 SQL 字节数），便于调用方区分「索引链路正常但 OLE DB 未绑定」
        // 与「索引不可达」：只有整条 COM 探测链成功后才可能到达这里。
        let result = query_windows_search_ole_db(&sql_string, limit)
            .map_err(|_| format!("ole_db_not_bound: sql_generated {} bytes", sql_string.len()))?;
        drop(sql_string);
        Ok(result)
    }
}

/// 把 `&str` 编码为以 NUL 结尾的 UTF-16 序列。
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 通过 OLE DB 执行 Windows Search SQL（Search.CollatorDSO）。
/// windows crate 未提供完整 OLE DB 绑定时，本函数返回 Err 触发降级。
fn query_windows_search_ole_db(_sql: &str, _limit: u32) -> Result<Vec<GlobalSearchHit>, String> {
    // 首版降级：Windows Search 索引用于即时层可选增强。
    // 完整实现需 IDBInitialize + ICommandText + IRowset（约 300 行 COM 调用），
    // 若后续需要，可在本函数内按 OLE DB provider 文档补齐。
    Err("ole_db_not_bound".into())
}

/// 非 Windows 平台：直接降级（空查询仍返回空，保持语义一致）。
#[cfg(not(windows))]
pub fn search(query: &str, _limit: u32) -> Result<Vec<GlobalSearchHit>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    Err("windows_search_unavailable".into())
}

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
        #[cfg(not(windows))]
        {
            let r = search("", 10);
            assert!(
                r.is_ok() && r.unwrap().is_empty(),
                "空查询在任何平台都应返回空"
            );
        }
    }

    #[test]
    fn ole_db_fallback_degrades_gracefully() {
        let r = query_windows_search_ole_db("SELECT 1", 10);
        assert!(r.is_err(), "未绑定 OLE DB 时应降级而非崩溃");
    }

    #[test]
    fn wide_encoder_appends_nul() {
        let v = wide("abc");
        assert_eq!(v, vec![97, 98, 99, 0]);
    }

    /// 集成探测：只有 CoCreateInstance→GetCatalog→GetQueryHelper→GenerateSQLFromUserQuery
    /// 全部成功才会走到 OLE DB 占位错误。需本机运行 Windows Search 服务时手工执行。
    #[test]
    #[cfg(windows)]
    #[ignore = "requires Windows Search service running"]
    fn com_probe_reaches_sql_generation() {
        let r = search("filetype:txt", 10);
        let err = r.unwrap_err();
        assert!(
            err.starts_with("ole_db_not_bound"),
            "探测链应在 OLE DB 占位处失败，实际错误：{err}"
        );
    }
}
