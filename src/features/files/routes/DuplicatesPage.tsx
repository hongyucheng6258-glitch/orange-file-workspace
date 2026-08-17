/**
 * 重复文件检测页 — 显示按哈希分组的重复文件
 */

import { useEffect, useState, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import {
  Copy,
  Loader2,
  Hash,
  Trash2,
  ChevronDown,
  ChevronRight,
  HardDrive,
} from "lucide-react";
import { formatSize } from "../../../lib/tauri";
import {
  findDuplicates,
  getHashStats,
  hashResources,
} from "../api/batchOpsApi";
import type { DuplicateGroup } from "../types/batchOps";
import { PathIconThumb } from "../../../components/FileIconThumb";

export function DuplicatesPage() {
  const navigate = useNavigate();
  const [groups, setGroups] = useState<DuplicateGroup[]>([]);
  const [stats, setStats] = useState<[number, number]>([0, 0]);
  const [loading, setLoading] = useState(true);
  const [hashing, setHashing] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [dups, st] = await Promise.all([findDuplicates(), getHashStats()]);
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

  // 计算全部需要哈希的资源 ID（仅当哈希率低时才需要）
  // 这里仅展示统计，实际哈希操作在 FilePage 进行

  const toggleGroup = (hash: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(hash)) next.delete(hash);
      else next.add(hash);
      return next;
    });
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
          {stats[0] < stats[1] && (
            <p className="dup-hint">
              已计算 {stats[0]} 个文件哈希，共 {stats[1]} 个文件。
              <br />
              前往文件页面，选中文件后点击「计算哈希」以检测更多重复。
            </p>
          )}
        </div>
      )}

      <div className="duplicates-list">
        {groups.map((group) => {
          const isExpanded = expanded.has(group.content_hash);
          return (
            <div key={group.content_hash} className="dup-group">
              <div
                className="dup-group-head"
                onClick={() => toggleGroup(group.content_hash)}
              >
                {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                <span className="dup-group-size">
                  {formatSize(group.size_bytes)}
                </span>
                <span className="dup-group-count">
                  {group.entries.length} 个副本
                </span>
                <span className="dup-group-hash" title={group.content_hash}>
                  {group.content_hash.slice(0, 12)}…
                </span>
              </div>
              {isExpanded && (
                <div className="dup-entries">
                  {group.entries.map((entry) => (
                    <div
                      key={entry.resource_id}
                      className="dup-entry"
                      onDoubleClick={() =>
                        navigate(
                          `/files?path=${encodeURIComponent(entry.path)}`,
                        )
                      }
                      title={entry.path}
                    >
                      <PathIconThumb
                        path={entry.path}
                        size={20}
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
