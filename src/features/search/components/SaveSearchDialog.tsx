/**
 * 保存搜索对话框 — 从当前搜索条件创建智能集合
 */

import { useState } from "react";
import { Star, X, Loader2 } from "lucide-react";
import { useSavedSearchStore } from "../stores/savedSearchStore";
import { serializeFilters } from "../api/savedSearchApi";
import type { SearchFilters } from "../types/savedSearch";

interface SaveSearchDialogProps {
  /** 当前搜索关键词 */
  query: string;
  /** 当前类型筛选 */
  kind: string;
  /** 关闭回调 */
  onClose: () => void;
  /** 保存成功后回调 */
  onSaved?: () => void;
}

export function SaveSearchDialog({
  query,
  kind,
  onClose,
  onSaved,
}: SaveSearchDialogProps) {
  const addSearch = useSavedSearchStore((s) => s.addSearch);
  const [name, setName] = useState(query || "未命名搜索");
  const [color, setColor] = useState<string>("");
  const [pinned, setPinned] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const filters: SearchFilters = {};
  if (kind) {
    // kind 可能是 "page"（含 project）或具体类型
    if (kind === "page") {
      filters.kinds = ["page", "project"];
    } else {
      filters.kinds = [kind];
    }
  }

  const handleSave = async () => {
    if (!name.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const search = await addSearch({
        name: name.trim(),
        query: query.trim() || null,
        filters_json: serializeFilters(filters),
        color: color || null,
      });
      if (pinned) {
        await useSavedSearchStore.getState().togglePin(search.id, true);
      }
      onSaved?.();
      onClose();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const colors = ["", "#f59e0b", "#3b82f6", "#10b981", "#8b5cf6", "#ef4444"];

  return (
    <div className="ssd-overlay" onClick={onClose}>
      <div className="ssd-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="ssd-head">
          <Star size={16} />
          <span>保存搜索为智能集合</span>
          <button className="btn btn-ghost btn-sm" onClick={onClose}>
            <X size={14} />
          </button>
        </div>
        <div className="ssd-body">
          <label className="ssd-label">名称</label>
          <input
            className="ssd-input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
            placeholder="给这个搜索起个名字…"
          />

          <label className="ssd-label">搜索条件预览</label>
          <div className="ssd-preview">
            {query && <span className="ssd-chip">关键词: {query}</span>}
            {kind && <span className="ssd-chip">类型: {kind}</span>}
            {!query && !kind && <span className="ssd-chip">无筛选条件</span>}
          </div>

          <label className="ssd-label">颜色标签（可选）</label>
          <div className="ssd-colors">
            {colors.map((c) => (
              <button
                key={c || "none"}
                className={`ssd-color-btn ${color === c ? "active" : ""}`}
                style={c ? { backgroundColor: c } : undefined}
                onClick={() => setColor(c)}
                title={c || "无颜色"}
              >
                {!c && "无"}
              </button>
            ))}
          </div>

          <label className="ssd-check">
            <input
              type="checkbox"
              checked={pinned}
              onChange={(e) => setPinned(e.target.checked)}
            />
            <span>固定到侧栏（智能集合）</span>
          </label>

          {error && <div className="ssd-error">{error}</div>}
        </div>
        <div className="ssd-foot">
          <button className="btn btn-ghost" onClick={onClose} disabled={saving}>
            取消
          </button>
          <button
            className="btn btn-primary"
            onClick={handleSave}
            disabled={saving || !name.trim()}
          >
            {saving ? <Loader2 size={14} className="spin" /> : <Star size={14} />}
            保存
          </button>
        </div>
      </div>
    </div>
  );
}
