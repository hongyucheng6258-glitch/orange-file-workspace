/**
 * 重复文件检测页 — 显示按哈希分组的重复文件
 */

import { useEffect, useState, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import {
  Copy,
  Loader2,
  Hash,
  ChevronDown,
  ChevronRight,
  HardDrive,
  Trash2,
  Check,
} from "lucide-react";
import { formatSize } from "../../../lib/tauri";
import {
  findDuplicates,
  getHashStats,
  trashResources,
} from "../api/batchOpsApi";
import type { DuplicateGroup } from "../types/batchOps";
import { PathIconThumb } from "../../../components/FileIconThumb";

export function DuplicatesPage() {
  const navigate = useNavigate();
  const [groups, setGroups] = useState<DuplicateGroup[]>([]);
  const [stats, setStats] = useState<[number, number]>([0, 0]);
  const [loading, setLoading] = useState(true);
  const [deleting, setDeleting] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      // 先检测（内部会补齐哈希），再刷新统计，保证数字准确
      const dups = await findDuplicates();
      const st = await getHashStats();
      setGroups(dups);
      setStats(st);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // 计算所有需要删除的副本 ID（每组保留第一个，其余删除）
  const allDuplicateIds = groups.flatMap((g) =>
    g.entries.slice(1).map((e) => e.resource_id),
  );

  const toggleGroup = (hash: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(hash)) next.delete(hash);
      else next.add(hash);
      return next;
    });
  };

  const handleDelete = async (ids: string[], keepCount: number) => {
    if (ids.length === 0) return;
    const ok = window.confirm(
      `确定要删除这 ${ids.length} 个重复副本（移入回收站）吗？将保留 ${keepCount} 个文件。`,
    );
    if (!ok) return;
    setDeleting(true);
    setError(null);
    try {
      await trashResources(ids);
      await loadData();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setDeleting(false);
    }
  };

  const totalWasted = groups.reduce(
    (sum, g) => sum + g.size_bytes * (g.entries.length - 1),
    0,
  );

  return (
    <div className="duplicates-page">
      <div className="duplicates-head">
        <div className="duplicates-title-row">
          <Hash size={20} />
          <h2>重复文件检测</h2>
        </div>
        <div className="duplicates-stats">
          <span className="dup-stat">
            <HardDrive size={13} />
            已哈希: {stats[0]} / {stats[1]}
          </span>
          <span className="dup-stat">
            <Copy size={13} />
            重复组: {groups.length}
          </span>
          <span className="dup-stat">
            可释放: {formatSize(totalWasted)}
          </span>
          <button
            className="dup-clean-all"
            disabled={allDuplicateIds.length === 0 || deleting || loading}
            onClick={() => handleDelete(allDuplicateIds, groups.length)}
          >
            <Trash2 size={14} />
            {deleting ? "删除中…" : `清理全部重复 (${allDuplicateIds.length})`}
          </button>
        </div>
      </div>

      {loading && (
        <div className="duplicates-loading">
          <Loader2 size={20} className="spin" />
          <span>正在扫描重复文件…</span>
        </div>
      )}

      {error && <div className="ssd-error">{error}</div>}

      {!loading && groups.length === 0 && (
        <div className="empty-state">
          <Hash size={32} />
          <p>未发现重复文件</p>
          <p className="dup-hint">
            已扫描 {stats[0]} 个文件哈希，共 {stats[1]} 个文件。
            <br />
            检测时已自动补齐缺失的哈希，如果仍有文件未计入，可能是该文件已不在原位置。
          </p>
        </div>
      )}

      <div className="duplicates-list">
        {groups.map((group) => {
          const isExpanded = expanded.has(group.content_hash);
          const deleteIds = group.entries
            .slice(1)
            .map((e) => e.resource_id);
          return (
            <div key={group.content_hash} className="dup-group">
              <div className="dup-group-head">
                <span
                  className="dup-group-toggle"
                  onClick={() => toggleGroup(group.content_hash)}
                >
                  {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                </span>
                <span
                  className="dup-group-main"
                  onClick={() => toggleGroup(group.content_hash)}
                >
                  <span className="dup-group-size">
                    {formatSize(group.size_bytes)}
                  </span>
                  <span className="dup-group-count">
                    {group.entries.length} 个副本
                  </span>
                  <span className="dup-group-hash" title={group.content_hash}>
                    {group.content_hash.slice(0, 12)}…
                  </span>
                </span>
                {deleteIds.length > 0 && (
                  <button
                    className="dup-group-del"
                    disabled={deleting}
                    onClick={() => handleDelete(deleteIds, 1)}
                  >
                    <Trash2 size={13} />
                    删除其余 ({deleteIds.length})
                  </button>
                )}
              </div>
              {isExpanded && (
                <div className="dup-entries">
                  {group.entries.map((entry, idx) => (
                    <div
                      key={entry.resource_id}
                      className={`dup-entry ${idx === 0 ? "is-keep" : ""}`}
                      onDoubleClick={() =>
                        navigate(
                          `/files?path=${encodeURIComponent(entry.path)}`,
                        )
                      }
                      title={entry.path}
                    >
                      {idx === 0 && (
                        <span className="dup-keep-badge">
                          <Check size={12} /> 保留
                        </span>
                      )}
                      <PathIconThumb
                        path={entry.path}
                        size={20}
                        fallback={<span className="dup-entry-fallback">📄</span>}
                      />
                      <div className="dup-entry-body">
                        <div className="dup-entry-name">{entry.name}</div>
                        <div className="dup-entry-path">{entry.path}</div>
                      </div>
                      <span className="dup-entry-size">
                        {formatSize(entry.size_bytes)}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
