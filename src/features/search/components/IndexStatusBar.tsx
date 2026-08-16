import { useCallback, useEffect, useState } from "react";
import { Database, Pause, Play, RotateCcw, Loader2 } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { call, formatTime } from "../../../lib/tauri";

// 与后端 events.rs EVENT_GLOBAL_SEARCH_INDEX_PROGRESS 常量一致（app.emit 用字符串）
const EVENT_INDEX_PROGRESS = "global-search://index-progress";

interface VolumeStatus {
  volume_id: string;
  root_path: string;
  status: string;
  indexed_count: number;
  skipped_count: number;
  last_error: string | null;
  completed_at: number | null;
}

interface IndexStatus {
  volumes: VolumeStatus[];
  paused: boolean;
  fts_enabled: boolean;
}

const STATUS_LABEL: Record<string, string> = {
  pending: "等待扫描",
  scanning: "扫描中",
  paused: "已暂停",
  completed: "已完成",
  error: "出错",
  offline: "离线",
};

export function IndexStatusBar() {
  const [status, setStatus] = useState<IndexStatus | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await call<IndexStatus>("get_search_index_status"));
    } catch {
      /* 索引表未就绪时静默 */
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    refresh();
    // 事件驱动刷新：索引进度事件到达时同步状态（比轮询更实时）
    listen(EVENT_INDEX_PROGRESS, () => {
      if (!disposed) refresh();
    }).then((u) => {
      if (disposed) u();
      else unlisten = u;
    });
    // 30s 兜底轮询：事件丢失时保证状态最终一致
    const timer = window.setInterval(refresh, 30000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
      if (unlisten) unlisten();
    };
  }, [refresh]);

  const run = async (cmd: string) => {
    setLoading(true);
    try {
      await call<void>(cmd);
      await refresh();
    } catch (e) {
      // 操作失败静默（状态栏辅助功能，不打断页面）
      console.error(`index control failed: ${cmd}`, e);
    } finally {
      setLoading(false);
    }
  };

  if (!status || status.volumes.length === 0) return null;

  const active = status.volumes.filter((v) => v.status === "scanning").length;
  const total = status.volumes.reduce((s, v) => s + v.indexed_count, 0);

  return (
    <div className="index-status-bar">
      <Database size={13} className="index-status-icon" />
      <div className="index-status-info">
        <span className="index-status-title">索引状态</span>
        <span className="index-status-detail">
          {status.paused
            ? "已暂停"
            : active > 0
              ? `正在扫描 ${active} 个磁盘`
              : `全部完成（${total.toLocaleString()} 项）`}
          {!status.fts_enabled && " · 名称匹配模式"}
        </span>
      </div>
      <div className="index-status-actions">
        {status.paused ? (
          <button className="btn btn-ghost btn-sm" onClick={() => run("resume_search_index")} disabled={loading}>
            <Play size={12} /> 继续
          </button>
        ) : (
          <button className="btn btn-ghost btn-sm" onClick={() => run("pause_search_index")} disabled={loading}>
            <Pause size={12} /> 暂停
          </button>
        )}
        <button className="btn btn-ghost btn-sm" onClick={() => run("rebuild_search_index")} disabled={loading}>
          {loading ? <Loader2 size={12} className="spin" /> : <RotateCcw size={12} />} 重建
        </button>
      </div>
      <div className="index-status-volumes">
        {status.volumes.map((v) => (
          <span
            key={v.volume_id}
            className="index-volume-chip"
            title={v.status === "error" && v.last_error ? v.last_error : undefined}
          >
            {v.root_path} · {STATUS_LABEL[v.status] ?? v.status} · {v.indexed_count.toLocaleString()}
            {v.completed_at ? ` · ${formatTime(v.completed_at)} 更新` : ""}
            {v.status === "error" && v.last_error ? ` · ${v.last_error}` : ""}
          </span>
        ))}
      </div>
    </div>
  );
}
