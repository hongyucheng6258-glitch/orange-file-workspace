/**
 * 智能集合结果页 — 执行保存的搜索并展示结果
 */

import { useEffect, useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import {
  Search,
  FileText,
  Folder,
  Star,
  Trash2,
  Loader2,
  ArrowLeft,
  Pin,
  PinOff,
  FileCode2,
} from "lucide-react";
import { useSavedSearchStore } from "../stores/savedSearchStore";
import { deserializeFilters } from "../api/savedSearchApi";
import type { SearchHit } from "../types/savedSearch";

const KIND_ICON: Record<string, typeof FileText> = {
  file: FileText,
  folder: Folder,
  page: FileCode2,
  project: FileCode2,
};

export function CollectionPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const {
    searches,
    executing,
    runSearch,
    togglePin,
    removeSearch,
    selectSearch,
    loadAll,
  } = useSavedSearchStore();
  const [showResults, setShowResults] = useState<SearchHit[]>([]);

  const search = searches.find((s) => s.id === id);

  useEffect(() => {
    if (!id) return;
    selectSearch(id);
    loadAll().then(async () => {
      try {
        const hits = await runSearch(id);
        setShowResults(hits);
      } catch {
        setShowResults([]);
      }
    });
  }, [id]);

  if (!search) {
    return (
      <div className="collection-page">
        <div className="empty-state">
          <span>未找到此智能集合</span>
          <button className="btn btn-ghost" onClick={() => navigate("/")}>
            返回首页
          </button>
        </div>
      </div>
    );
  }

  const filters = deserializeFilters(search.filters_json);

  return (
    <div className="collection-page">
      <div className="collection-head">
        <button
          className="btn btn-ghost btn-sm"
          onClick={() => navigate("/")}
        >
          <ArrowLeft size={14} />
        </button>
        <span
          className="collection-color-dot"
          style={search.color ? { backgroundColor: search.color } : undefined}
        />
        <h2 className="collection-title">{search.name}</h2>
        <div className="collection-actions">
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => togglePin(search.id, !search.is_pinned)}
            title={search.is_pinned ? "取消固定" : "固定到侧栏"}
          >
            {search.is_pinned ? <PinOff size={14} /> : <Pin size={14} />}
          </button>
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => {
              if (confirm(`删除智能集合「${search.name}」？`)) {
                removeSearch(search.id);
                navigate("/");
              }
            }}
            title="删除"
          >
            <Trash2 size={14} />
          </button>
        </div>
      </div>

      <div className="collection-filters">
        {search.query && (
          <span className="ssd-chip">
            <Search size={11} />
            {search.query}
          </span>
        )}
        {filters.kinds?.map((k) => (
          <span key={k} className="ssd-chip">
            类型: {k}
          </span>
        ))}
        {filters.favorite_only && (
          <span className="ssd-chip">
            <Star size={11} />
            仅收藏
          </span>
        )}
        {filters.extensions?.map((e) => (
          <span key={e} className="ssd-chip">
            .{e}
          </span>
        ))}
        {!search.query &&
          !filters.kinds?.length &&
          !filters.favorite_only &&
          !filters.extensions?.length && (
            <span className="collection-no-filter">无筛选条件</span>
          )}
      </div>

      <div className="collection-results">
        {executing && (
          <div className="collection-loading">
            <Loader2 size={20} className="spin" />
            <span>正在搜索…</span>
          </div>
        )}
        {!executing && showResults.length === 0 && (
          <div className="empty-state">
            <span>没有匹配的资源</span>
          </div>
        )}
        {showResults.map((hit) => {
          const Icon = KIND_ICON[hit.kind] ?? FileText;
          return (
            <div
              key={hit.id}
              className="collection-result"
              onDoubleClick={() => {
                if (hit.path) {
                  navigate(`/files?path=${encodeURIComponent(hit.path)}`);
                }
              }}
              title={hit.path ?? hit.name}
            >
              <span className="collection-result-icon">
                <Icon size={15} />
              </span>
              <div className="collection-result-body">
                <div className="collection-result-name">
                  {hit.name}
                  {hit.is_favorite && <Star size={11} className="fav-star" />}
                </div>
                <div className="collection-result-meta">
                  {hit.path ?? hit.kind}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
