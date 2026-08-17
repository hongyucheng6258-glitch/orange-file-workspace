/**
 * 批量重命名对话框 — 预览 + 确认执行
 *
 * 支持模式：
 *   - 查找替换：old → new
 *   - 添加前缀/后缀
 *   - 序号重命名：base_001, base_002, ...
 */

import { useState, useEffect, useCallback } from "react";
import { X, Loader2, Check, AlertTriangle, RotateCcw } from "lucide-react";
import type { RenameItem } from "../types/batchOps";
import type { Resource } from "../../../lib/tauri";
import { previewBatchRename, executeBatchRename } from "../api/batchOpsApi";

type RenameMode = "replace" | "prefix" | "suffix" | "number";

interface BatchRenameDialogProps {
  selected: Resource[];
  onClose: () => void;
  onDone: () => void;
}

export function BatchRenameDialog({
  selected,
  onClose,
  onDone,
}: BatchRenameDialogProps) {
  const [mode, setMode] = useState<RenameMode>("replace");
  const [findStr, setFindStr] = useState("");
  const [replaceStr, setReplaceStr] = useState("");
  const [prefix, setPrefix] = useState("");
  const [suffix, setSuffix] = useState("");
  const [baseName, setBaseName] = useState("file");
  const [startIndex, setStartIndex] = useState(1);
  const [preview, setPreview] = useState<RenameItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 计算新名称
  const computeNewName = useCallback(
    (oldName: string, index: number): string => {
      const dotIdx = oldName.lastIndexOf(".");
      const stem = dotIdx > 0 ? oldName.slice(0, dotIdx) : oldName;
      const ext = dotIdx > 0 ? oldName.slice(dotIdx) : "";

      switch (mode) {
        case "replace":
          if (!findStr) return oldName;
          return stem.split(findStr).join(replaceStr) + ext;
        case "prefix":
          return prefix + oldName;
        case "suffix":
          return stem + suffix + ext;
        case "number": {
          const num = startIndex + index;
          const padded = String(num).padStart(3, "0");
          return `${baseName}_${padded}${ext}`;
        }
      }
    },
    [mode, findStr, replaceStr, prefix, suffix, baseName, startIndex],
  );

  // 生成预览
  const generatePreview = useCallback(async () => {
    if (selected.length === 0) return;
    setLoading(true);
    setError(null);
    try {
      const items: [string, string][] = selected.map((r, i) => [
        r.id,
        computeNewName(r.name, i),
      ]);
      const result = await previewBatchRename(items);
      setPreview(result);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, [selected, computeNewName]);

  // 自动生成预览
  useEffect(() => {
    generatePreview();
  }, [generatePreview]);

  const handleExecute = async () => {
    const okItems = preview.filter((p) => p.status === "ok" && p.old_name !== p.new_name);
    if (okItems.length === 0) return;
    setExecuting(true);
    setError(null);
    try {
      await executeBatchRename(okItems, `批量重命名 ${okItems.length} 个文件`);
      onDone();
      onClose();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setExecuting(false);
    }
  };

  const okCount = preview.filter((p) => p.status === "ok" && p.old_name !== p.new_name).length;
  const conflictCount = preview.filter((p) => p.status === "conflict").length;
  const invalidCount = preview.filter((p) => p.status === "invalid" || p.status === "missing").length;

  return (
    <div className="ssd-overlay" onClick={onClose}>
      <div className="brd-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="ssd-head">
          <span>批量重命名 ({selected.length} 个文件)</span>
          <button className="btn btn-ghost btn-sm" onClick={onClose}>
            <X size={14} />
          </button>
        </div>

        <div className="brd-body">
          {/* 模式选择 */}
          <div className="brd-modes">
            {([
              ["replace", "查找替换"],
              ["prefix", "加前缀"],
              ["suffix", "加后缀"],
              ["number", "序号"],
            ] as [RenameMode, string][]).map(([m, label]) => (
              <button
                key={m}
                className={`brd-mode-btn ${mode === m ? "active" : ""}`}
                onClick={() => setMode(m)}
              >
                {label}
              </button>
            ))}
          </div>

          {/* 参数输入 */}
          <div className="brd-params">
            {mode === "replace" && (
              <>
                <input
                  className="brd-input"
                  placeholder="查找…"
                  value={findStr}
                  onChange={(e) => setFindStr(e.target.value)}
                />
                <span className="brd-arrow">→</span>
                <input
                  className="brd-input"
                  placeholder="替换为…"
                  value={replaceStr}
                  onChange={(e) => setReplaceStr(e.target.value)}
                />
              </>
            )}
            {mode === "prefix" && (
              <input
                className="brd-input"
                placeholder="前缀…"
                value={prefix}
                onChange={(e) => setPrefix(e.target.value)}
              />
            )}
            {mode === "suffix" && (
              <input
                className="brd-input"
                placeholder="后缀（插入到扩展名前）…"
                value={suffix}
                onChange={(e) => setSuffix(e.target.value)}
              />
            )}
            {mode === "number" && (
              <>
                <input
                  className="brd-input"
                  placeholder="基础名…"
                  value={baseName}
                  onChange={(e) => setBaseName(e.target.value)}
                />
                <input
                  className="brd-input brd-input-narrow"
                  type="number"
                  placeholder="起始"
                  value={startIndex}
                  onChange={(e) => setStartIndex(parseInt(e.target.value) || 1)}
                />
              </>
            )}
            <button
              className="btn btn-ghost btn-sm"
              onClick={generatePreview}
              disabled={loading}
              title="刷新预览"
            >
              <RotateCcw size={13} />
            </button>
          </div>

          {/* 统计 */}
          <div className="brd-stats">
            <span className="brd-stat ok">
              <Check size={12} /> {okCount} 可执行
            </span>
            {conflictCount > 0 && (
              <span className="brd-stat warn">
                <AlertTriangle size={12} /> {conflictCount} 冲突
              </span>
            )}
            {invalidCount > 0 && (
              <span className="brd-stat err">
                <AlertTriangle size={12} /> {invalidCount} 无效
              </span>
            )}
          </div>

          {/* 预览列表 */}
          <div className="brd-preview">
            {loading && (
              <div className="brd-loading">
                <Loader2 size={16} className="spin" /> 生成预览…
              </div>
            )}
            {!loading && preview.length === 0 && (
              <div className="brd-empty">暂无预览</div>
            )}
            {!loading &&
              preview.map((item) => (
                <div key={item.resource_id} className={`brd-row brd-${item.status}`}>
                  <div className="brd-old">{item.old_name}</div>
                  <span className="brd-arrow">→</span>
                  <div className="brd-new">{item.new_name}</div>
                  {item.status !== "ok" && (
                    <span className="brd-status-tag" title={item.error ?? ""}>
                      {item.status}
                    </span>
                  )}
                </div>
              ))}
          </div>

          {error && <div className="ssd-error">{error}</div>}
        </div>

        <div className="ssd-foot">
          <button className="btn btn-ghost" onClick={onClose} disabled={executing}>
            取消
          </button>
          <button
            className="btn btn-primary"
            onClick={handleExecute}
            disabled={executing || okCount === 0}
          >
            {executing ? <Loader2 size={14} className="spin" /> : <Check size={14} />}
            执行重命名 ({okCount})
          </button>
        </div>
      </div>
    </div>
  );
}
