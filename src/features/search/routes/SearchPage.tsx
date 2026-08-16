import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { IndexStatusBar } from "../components/IndexStatusBar";

const KIND_LABEL: Record<string, string> = {
  file: "文件",
  folder: "文件夹",
  app: "应用",
  page: "页面",
  project: "项目",
};

// 结果来源标签：与结果类型（kind）不同维度，标识命中来自哪个索引源。
// 取值与后端 GlobalSearchHit.source / globalSearch.ts 类型一致。
const SOURCE_LABEL: Record<GlobalSearchHit["source"], string> = {
  windows: "系统搜索",
  local_index: "本地索引",
  app_index: "应用",
  nexus: "资源库",
};

function sourceLabel(source: GlobalSearchHit["source"]): string {
  return SOURCE_LABEL[source];
}

// 可执行文件扩展名：打开前需用户确认（首启安全防护）
const EXEC_EXTS = ["exe", "bat", "cmd", "com", "msi"];

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
  const [drive, setDrive] = useState("");
  const [ext, setExt] = useState("");
  const [results, setResults] = useState<GlobalSearchHit[]>([]);
  const [searched, setSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [indexing, setIndexing] = useState(false);
  // 键盘导航当前选中项索引（-1 表示未选中）
  const [activeIdx, setActiveIdx] = useState(-1);
  const seqRef = useRef(0);
  const searchIdRef = useRef<number | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);
  const seenKeysRef = useRef<Set<string>>(new Set());
  // 最新筛选值：URL 变化触发的防抖搜索读取它，避免 chip 高亮与结果不一致
  const kindRef = useRef(kind);
  // 卸载标志：异步订阅/搜索返回后若组件已卸载则立即清理，防止监听泄漏
  const disposedRef = useRef(false);

  useEffect(() => {
    kindRef.current = kind;
  }, [kind]);

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
      setActiveIdx(-1); // 新搜索开始时重置键盘选中
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
        if (seq !== seqRef.current || disposedRef.current) return;
        searchIdRef.current = batch.search_id;
        batch.hits.forEach((h) => seenKeysRef.current.add(h.key));
        const filtered = k ? batch.hits.filter((h) => filterMatch(h.kind, k)) : batch.hits;
        setResults(filtered);
        setSearched(true);
        setIndexing(batch.index_incomplete);
        if (batch.index_incomplete) {
          const unlisten = await onSearchBatch(batch.search_id, (hits) => {
            if (seq !== seqRef.current || disposedRef.current) return;
            const fresh = hits.filter(
              (h) => !seenKeysRef.current.has(h.key) && (k ? filterMatch(h.kind, k) : true),
            );
            if (!fresh.length) return;
            fresh.forEach((h) => seenKeysRef.current.add(h.key));
            setResults((prev) => {
              const next = [...prev, ...fresh];
              return next.length > 500 ? next.slice(0, 500) : next;
            });
          });
          if (seq !== seqRef.current || disposedRef.current) {
            unlisten();
            return;
          }
          unlistenRef.current = unlisten;
        }
      } catch (e) {
        if (seq === seqRef.current) {
          setError((e as Error).message);
          setResults([]);
          setSearched(true);
          setIndexing(false);
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

  // 磁盘/扩展名筛选是纯展示层过滤：不触发重新搜索、不影响补批/去重，
  // 仅在渲染前对已收集的 results 应用额外过滤层。
  // 盘符从当前结果路径推导（去重、按字母排序），"全部磁盘" 表示不过滤。
  const drives = useMemo(() => {
    const set = new Set<string>();
    for (const r of results) {
      const m = r.path?.match(/^([A-Za-z]):/);
      if (m) set.add(m[1].toUpperCase());
    }
    return [...set].sort();
  }, [results]);

  const filteredResults = useMemo(() => {
    let out = results;
    if (drive) {
      out = out.filter((r) => r.path?.toUpperCase().startsWith(drive));
    }
    const extClean = ext.trim().replace(/^\.+/, "").toLowerCase();
    if (extClean) {
      out = out.filter((r) => (r.path?.toLowerCase() ?? "").endsWith(`.${extClean}`));
    }
    return out;
  }, [results, drive, ext]);

  // URL 关键词变化 → 同步输入框并触发搜索（200ms 防抖；读取最新筛选值）
  useEffect(() => {
    setQuery(urlQ);
    const timer = window.setTimeout(() => {
      if (urlQ) doSearch(urlQ, kindRef.current);
    }, 200);
    return () => window.clearTimeout(timer);
  }, [urlQ, doSearch]);

  // 卸载：置标志后统一清理（取消搜索 + 取消事件订阅）
  useEffect(() => {
    disposedRef.current = false;
    return () => {
      disposedRef.current = true;
      cleanup();
    };
  }, [cleanup]);

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
    // 可执行文件首次运行确认：取 path 最后一段小写，命中扩展名先弹确认框
    const last = (hit.path ?? "").split(/[\\/]/).pop()?.toLowerCase() ?? "";
    if (EXEC_EXTS.some((ext) => last.endsWith(`.${ext}`))) {
      if (!window.confirm(`确定要运行「${hit.name}」吗？`)) return;
    }
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
    if (!hit.path) return;
    try {
      await navigator.clipboard.writeText(hit.path);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  // 键盘导航：方向键移动选中、Enter 打开选中项、Escape 清除选中
  // 结果为空时只响应 Escape，忽略方向键/Enter（基于过滤后数组，保证索引一致）
  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      setActiveIdx(-1);
      return;
    }
    if (filteredResults.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIdx((i) => Math.min(i + 1, filteredResults.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      // 防越界：新搜索结果变少时 activeIdx 可能已失效
      if (activeIdx >= 0 && activeIdx < filteredResults.length) {
        e.preventDefault();
        activate(filteredResults[activeIdx]);
      }
    }
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
            onKeyDown={onKeyDown}
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
          <select
            className="filter-select"
            value={drive}
            onChange={(e) => {
              setDrive(e.target.value);
              setActiveIdx(-1);
            }}
            title="按磁盘筛选（仅当前结果中出现的盘符）"
          >
            <option value="">全部磁盘</option>
            {drives.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
          <input
            className="filter-ext-input"
            value={ext}
            placeholder="扩展名"
            spellCheck={false}
            onChange={(e) => {
              setExt(e.target.value);
              setActiveIdx(-1);
            }}
            title="按扩展名筛选（不带点，如 pdf）"
          />
        </div>
      </div>

      <IndexStatusBar />

      {indexing && !loading && (
        <div className="system-note">后台索引仍在补充中，结果会持续更新</div>
      )}
      {error && <div className="system-error">{error}</div>}
      {results.length >= 500 && (
        <div className="system-note">结果较多，已展示前 500 条，请通过筛选缩小范围</div>
      )}

      <div className="search-results">
        {searched && filteredResults.length === 0 && !loading && (
          <div className="empty-state">
            <span>没有找到匹配的结果</span>
          </div>
        )}
        {filteredResults.map((r, i) => (
          <div
            key={r.key}
            className={`search-result ${i === activeIdx ? "active" : ""}`}
            onDoubleClick={() => activate(r)}
            title={r.path ?? ""}
          >
            <span className="search-kind-icon">{kindIcon(r.kind)}</span>
            <div className="search-result-body">
              <div className="search-result-name">
                {r.name}
                {sourceLabel(r.source) && <span className="search-source-tag">{sourceLabel(r.source)}</span>}
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
