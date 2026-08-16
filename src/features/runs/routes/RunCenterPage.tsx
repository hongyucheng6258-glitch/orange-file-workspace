import { useEffect } from "react";
import {
  Activity,
  Eye,
  Loader2,
  RefreshCw,
  RotateCw,
  Square,
  TerminalSquare,
} from "lucide-react";
import { useRunCenterStore, useRunCenterEvents } from "../stores/runCenterStore";
import { ProjectLogViewer } from "../../projects/components/ProjectLogViewer";
import {
  commandLineOf,
  expectedPortOf,
  formatRunTime,
  runDurationSeconds,
  runStateLabel,
} from "../lib/runCenter";
import type { RunSnapshot } from "../../projects/lib/projectRuntime";

function isActive(snap: RunSnapshot): boolean {
  return snap.state === "starting" || snap.state === "running" || snap.state === "stopping";
}

/** 运行中心：活动实例 + 已退出记录 + 日志查看。 */
export function RunCenterPage() {
  const store = useRunCenterStore();

  useEffect(() => {
    void store.load();
    const off = useRunCenterEvents();
    return () => off();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const active = store.runs.filter(isActive);
  const exited = store.runs.filter((r) => !isActive(r));

  return (
    <div className="run-center">
      <div className="run-center-head">
        <Activity size={16} color="var(--primary)" />
        <span>运行中心</span>
        <button
          className="icon-btn"
          title="刷新"
          onClick={() => void store.load()}
          disabled={store.loading}
        >
          <RefreshCw size={13} className={store.loading ? "spin" : ""} />
        </button>
      </div>

      {store.error && (
        <div className="run-error-banner">
          <span>{store.error}</span>
          <button className="icon-btn" onClick={() => useRunCenterStore.setState({ error: null })}>
            ×
          </button>
        </div>
      )}

      <section className="run-center-section">
        <h3>活动实例（{active.length}）</h3>
        {active.length === 0 && <div className="run-center-empty">当前没有活动实例</div>}
        {active.map((snap) => (
          <div key={snap.runId} className="run-center-card">
            <div className="run-center-card-head">
              <span className={`run-state-badge ${snap.state}`}>{runStateLabel(snap.state)}</span>
              <span className="run-center-name">
                {store.projectNames[snap.projectId] ?? snap.projectId}
              </span>
              {snap.pid != null && <span className="run-center-meta">PID {snap.pid}</span>}
              {runDurationSeconds(snap.startedAt) != null && (
                <span className="run-center-meta">
                  {runDurationSeconds(snap.startedAt)}s
                </span>
              )}
            </div>
            <div className="run-center-code">
              <code>{commandLineOf(snap)}</code>
            </div>
            <div className="run-center-meta-line">
              <span>启动 {formatRunTime(snap.startedAt)}</span>
              {expectedPortOf(snap) != null && <span>端口 {expectedPortOf(snap)}</span>}
            </div>
            <div className="run-center-actions">
              <button
                className="btn-secondary"
                disabled={snap.state !== "running"}
                onClick={() => void store.stop(snap.runId)}
              >
                <Square size={12} /> 停止
              </button>
              <button
                className="btn-secondary"
                disabled={snap.state !== "running"}
                onClick={() => void store.restart(snap.runId)}
              >
                <RotateCw size={12} /> 重启
              </button>
              <button
                className="btn-secondary"
                disabled={snap.state !== "running"}
                onClick={() => void store.openPreview(snap.runId)}
              >
                <Eye size={12} /> 预览
              </button>
              <button
                className="btn-secondary"
                onClick={() => void store.toggleLogs(snap.runId)}
              >
                <TerminalSquare size={12} /> 日志
              </button>
            </div>
          </div>
        ))}
      </section>

      <section className="run-center-section">
        <h3>已退出记录（{exited.length}）</h3>
        {exited.length === 0 && (
          <div className="run-center-empty">
            暂无已退出记录。每个项目保留最近 20 条、最多 7 天，日志仅保留在当前会话。
          </div>
        )}
        <div className="run-center-table">
          {exited.map((snap) => (
            <div key={snap.runId} className="run-center-row">
              <span className={`run-state-badge ${snap.state}`}>{runStateLabel(snap.state)}</span>
              <span className="run-center-name">
                {store.projectNames[snap.projectId] ?? snap.projectId}
              </span>
              <span className="run-center-code-short">
                <code>{commandLineOf(snap)}</code>
              </span>
              <span className="run-center-meta">
                {snap.exitCode != null ? `退出码 ${snap.exitCode}` : "—"}
              </span>
              <span className="run-center-meta">
                {snap.errorMessage ? `错误：${snap.errorMessage}` : ""}
              </span>
              <span className="run-center-meta">启动 {formatRunTime(snap.startedAt)}</span>
              <button
                className="btn-secondary"
                onClick={() => void store.toggleLogs(snap.runId)}
              >
                <TerminalSquare size={12} /> 日志
              </button>
            </div>
          ))}
        </div>
      </section>

      {store.expandedRunId && (
        <section className="run-center-section">
          <h3>日志（{store.expandedRunId.slice(0, 8)}…）</h3>
          <div className="run-center-log-note">日志仅保留在当前应用会话内。</div>
          <ProjectLogViewer
            logs={store.logs}
            onClear={() => useRunCenterStore.setState({ logs: [] })}
            onCopy={() => {
              void navigator.clipboard.writeText(store.logs.map((l) => l.text).join(""));
            }}
          />
        </section>
      )}

      {store.loading && (
        <div className="run-center-loading">
          <Loader2 size={14} className="spin" /> 加载中…
        </div>
      )}
    </div>
  );
}
