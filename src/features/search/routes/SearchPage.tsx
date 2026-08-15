import { useCallback, useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Search, FileText, Folder, Code2, Star, Loader2 } from "lucide-react";
import { call, formatTime } from "../../../lib/tauri";

interface SearchHit {
  id: string;
  kind: string;
  name: string;
  parent_id: string | null;
  is_favorite: boolean;
  updated_at: number;
  path: string | null;
  source_type: string | null;
}

const KIND_LABEL: Record<string, string> = {
  file: "文件",
  folder: "文件夹",
  page: "页面",
  project: "项目",
};

export function SearchPage() {
  const [params, setParams] = useSearchParams();
  const urlQ = params.get("q") ?? "";
  const [query, setQuery] = useState(urlQ);
  const [kind, setKind] = useState("");
  const [results, setResults] = useState<SearchHit[]>([]);
  const [searched, setSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  // 请求序号：只采纳最后一次搜索的响应，避免旧结果覆盖新结果。
  const seqRef = useRef(0);

  const doSearch = useCallback(async (q: string, k: string) => {
    const trimmed = q.trim();
    const seq = ++seqRef.current;
    if (!trimmed) {
      setResults([]);
      setSearched(true);
      setError(null);
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const hits = await call<SearchHit[]>("search_resources", {
        query: trimmed,
        kinds: k ? [k] : null,
        favoriteOnly: false,
        limit: 100,
        offset: 0,
      });
      if (seq === seqRef.current) {
        setResults(hits);
        setSearched(true);
      }
    } catch (e) {
      if (seq === seqRef.current) {
        setError((e as Error).message);
        setResults([]);
        setSearched(true);
      }
    } finally {
      if (seq === seqRef.current) {
        setLoading(false);
      }
    }
  }, []);

  // URL 关键词变化 → 同步输入框并触发搜索（支持顶部栏二次搜索与刷新保留）。
  useEffect(() => {
    setQuery(urlQ);
    if (urlQ) {
      doSearch(urlQ, "");
    }
  }, [urlQ, doSearch]);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = query.trim();
    if (!trimmed) {
      seqRef.current += 1;
      setResults([]);
      setSearched(true);
      setError(null);
      return;
    }
    if (urlQ === trimmed) {
      // 关键词未变：直接按当前类型筛选重新搜索。
      doSearch(trimmed, kind);
    } else {
      setParams({ q: trimmed }, { replace: true });
    }
  };

  const pickKind = (v: string) => {
    setKind(v);
    const q = urlQ || query.trim();
    if (q) doSearch(q, v);
  };

  return (
    <div className="search-page">
      <div className="search-head">
        <form className="search-input-wrap" onSubmit={submit}>
          <Search size={15} />
          <input
            value={query}
            placeholder="搜索名称或路径…"
            onChange={(e) => setQuery(e.target.value)}
            autoFocus
          />
          <button type="submit" className="btn btn-primary" disabled={loading}>
            {loading ? <Loader2 size={13} className="spin" /> : <Search size={13} />}
            {loading ? "搜索中…" : "搜索"}
          </button>
        </form>
        <div className="search-filters">
          {[
            { value: "", label: "全部" },
            { value: "file", label: "文件" },
            { value: "folder", label: "文件夹" },
            { value: "page", label: "页面" },
            { value: "project", label: "项目" },
          ].map((f) => (
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

      {error && <div className="system-error">{error}</div>}

      <div className="search-results">
        {searched && results.length === 0 && !loading && (
          <div className="empty-state">
            <span>没有找到匹配的结果</span>
          </div>
        )}
        {results.map((r) => (
          <div key={r.id} className="search-result">
            <span className="search-kind-icon">
              {r.kind === "file" ? (
                <FileText size={14} color="var(--file)" />
              ) : r.kind === "folder" ? (
                <Folder size={14} color="var(--folder)" />
              ) : r.kind === "page" ? (
                <FileText size={14} color="var(--primary)" />
              ) : (
                <Code2 size={14} color="var(--code)" />
              )}
            </span>
            <div className="search-result-body">
              <div className="search-result-name">
                {r.name}
                {r.is_favorite && <Star size={11} color="var(--warning)" />}
              </div>
              <div className="search-result-meta">
                {r.path ?? KIND_LABEL[r.kind] ?? r.kind}
              </div>
            </div>
            <span className="search-result-time">{formatTime(r.updated_at)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
