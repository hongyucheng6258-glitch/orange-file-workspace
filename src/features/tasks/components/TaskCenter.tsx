import { useEffect, useState } from "react";
import { X, CheckCircle2, AlertCircle, Loader2, Ban, Archive, Database } from "lucide-react";
import { useTaskStore, useActiveTasks } from "../stores/taskStore";
import { call, formatTime } from "../../../lib/tauri";

interface BackupRecord {
  id: string;
  backup_type: string;
  source: string;
  resource_count: number | null;
  status: string;
  created_at: number;
}

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

function statusIcon(status: string) {
  switch (status) {
    case "completed":
      return <CheckCircle2 size={13} color="var(--success)" />;
    case "failed":
      return <AlertCircle size={13} color="var(--danger)" />;
    case "cancelled":
      return <Ban size={13} color="var(--text-tertiary)" />;
    default:
      return <Loader2 size={13} className="spin" color="var(--primary)" />;
  }
}

/** 任务中心页面。 */
export function TaskCenterPage() {
  const { tasks, refresh, cancel } = useTaskStore();
  const [backups, setBackups] = useState<BackupRecord[]>([]);
  const [indexStatus, setIndexStatus] = useState<IndexStatus | null>(null);

  const loadBackups = async () => {
    try {
      const list = await call<BackupRecord[]>("list_backups", {});
      setBackups(list.slice(0, 5));
    } catch {
      /* 静默 */
    }
  };

  const loadIndex = async () => {
    try {
      setIndexStatus(await call<IndexStatus>("get_search_index_status"));
    } catch {
      /* 静默 */
    }
  };

  useEffect(() => {
    refresh();
    loadBackups();
    loadIndex();
  }, []);

  return (
    <div className="task-center">
      <h2>任务中心</h2>

      {tasks.length === 0 && backups.length === 0 && (!indexStatus || indexStatus.volumes.length === 0) ? (
        <div className="empty-state">
          <span>暂无任务</span>
        </div>
      ) : (
        <div className="task-list">
          {tasks.map((t) => (
            <div key={t.id} className="task-card">
              <div className="task-card-head">
                <span className="task-title">
                  {statusIcon(t.status)}
                  {t.title}
                </span>
                <span className={`tag task-status-${t.status}`}>{t.status}</span>
              </div>
              <div className="task-progress">
                <div
                  className="task-progress-bar"
                  style={{
                    width: `${
                      t.total_count
                        ? Math.min(100, ((t.completed_count + t.failed_count) / t.total_count) * 100)
                        : 0
                    }%`,
                  }}
                />
              </div>
              <div className="task-card-meta">
                <span>
                  {t.completed_count} 完成 · {t.failed_count} 失败
                  {t.total_count != null ? ` · 共 ${t.total_count}` : ""}
                </span>
                {(t.status === "queued" || t.status === "running" || t.status === "paused") && (
                  <button className="btn btn-ghost" onClick={() => cancel(t.id)}>
                    <X size={13} /> 取消
                  </button>
                )}
              </div>
            </div>
          ))}

          {backups.map((b) => (
            <div key={b.id} className="task-card">
              <div className="task-card-head">
                <span className="task-title">
                  <Archive size={13} color="var(--primary)" />
                  备份（{b.backup_type === "full" ? "完整" : "元数据"}
                  {b.source === "auto" ? " · 自动" : " · 手动"}）
                </span>
                <span className="tag task-status-completed">
                  {b.status === "completed" ? "已完成" : b.status}
                </span>
              </div>
              <div className="task-card-meta">
                <span>
                  {b.resource_count ?? 0} 资源 · {formatTime(b.created_at)}
                </span>
              </div>
            </div>
          ))}

          {indexStatus && indexStatus.volumes.length > 0 && (
            <div className="task-card">
              <div className="task-card-head">
                <span className="task-title">
                  <Database size={13} color="var(--primary)" />
                  索引扫描{indexStatus.paused ? "（已暂停）" : ""}
                  {!indexStatus.fts_enabled ? "（名称匹配模式）" : ""}
                </span>
              </div>
              <div className="task-card-meta index-volumes">
                {indexStatus.volumes.map((v) => (
                  <span key={v.volume_id} className="index-volume-chip">
                    {v.root_path} · {STATUS_LABEL[v.status] ?? v.status} · {v.indexed_count.toLocaleString()}
                    {v.status === "error" && v.last_error ? ` · ${v.last_error}` : ""}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** 底部活跃任务条，显示进行中的任务。 */
export function TaskBar() {
  const active = useActiveTasks();
  const { cancel } = useTaskStore();

  if (active.length === 0) return null;

  return (
    <div className="task-bar">
      {active.map((t) => (
        <div key={t.id} className="task-bar-item">
          <Loader2 size={13} className="spin" color="var(--primary)" />
          <span className="task-bar-title">{t.title}</span>
          <span className="task-bar-count">
            {t.total_count
              ? `${t.completed_count + t.failed_count}/${t.total_count}`
              : `${t.completed_count + t.failed_count}`}
          </span>
          <button className="icon-btn" onClick={() => cancel(t.id)} title="取消任务">
            <X size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
