import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Search, FileText, Folder, Code2, Star } from "lucide-react";
import { call } from "../../../lib/tauri";
import { formatTime } from "../../../lib/tauri";

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
  const [params] = useSearchParams();
  const [query, setQuery] = useState(params.get("q") ?? "");
  const [kind, setKind] = useState<string>("");
  const [results, setResults] = useState<SearchHit[]>([]);
  const [searched, setSearched] = useState(false);

  const doSearch = async (q: string, k: string) => {
    const hits = await call<SearchHit[]>("search_resources", {
      query: q,
      kinds: k ? [k] : null,
      favoriteOnly: false,
      limit: 100,
      offset: 0,
    });
    setResults(hits);
    setSearched(true);
  };

  useEffect(() => {
    const q = params.get("q") ?? "";
    if (q) doSearch(q, "");
  }, [params]);

  return (
    <div className="search-page">
      <div className="search-head">
        <div className="search-input-wrap">
          <Search size={15} />
          <input
            value={query}
            placeholder="搜索名称或路径…"
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && doSearch(query, kind)}
            autoFocus
          />
          <button className="btn btn-primary" onClick={() => doSearch(query, kind)}>
            搜索
          </button>
        </div>
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
              onClick={() => {
                setKind(f.value);
                if (query) doSearch(query, f.value);
              }}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>

      <div className="search-results">
        {searched && results.length === 0 && (
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
