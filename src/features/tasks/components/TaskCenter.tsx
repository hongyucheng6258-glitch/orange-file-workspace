import { useEffect } from "react";
import { X, CheckCircle2, AlertCircle, Loader2, Ban } from "lucide-react";
import { useTaskStore, useActiveTasks } from "../stores/taskStore";

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

  useEffect(() => {
    refresh();
  }, []);

  return (
    <div className="task-center">
      <h2>任务中心</h2>
      {tasks.length === 0 ? (
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
