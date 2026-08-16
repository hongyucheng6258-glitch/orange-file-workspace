import { useEffect, useMemo, useState } from "react";
import { Eye, Loader2, Play, RotateCw, Square, Trash2, Plus, TerminalSquare } from "lucide-react";
import { useProjectRuntimeStore } from "../stores/projectRuntimeStore";
import { useProjectPreviewStore } from "../stores/projectPreviewStore";
import { ownershipLabel, previewSourceLabel, subscribePreviewReady } from "../lib/projectPreview";
import { ProjectLogViewer } from "./ProjectLogViewer";
import { RunConfirmationDialog } from "./RunConfirmationDialog";

/** 项目页运行面板：识别、配置、确认、启动/停止/重启、Web 预览、日志。 */
export function ProjectRuntimePanel() {
  const store = useProjectRuntimeStore();
  const preview = useProjectPreviewStore();
  const [showConfirm, setShowConfirm] = useState(false);

  const projectId = store.projectId;
  const detection = store.detection;
  const config = store.config;
  const run = store.run;
  const actions = useMemo(() => {
    if (!run) return { canStart: true, canStop: false, canRestart: false };
    if (run.state === "starting" || run.state === "running") {
      return { canStart: false, canStop: true, canRestart: true };
    }
    if (run.state === "stopping") {
      return { canStart: false, canStop: false, canRestart: false };
    }
    return { canStart: true, canStop: false, canRestart: true };
  }, [run]);

  const canPreview = run?.state === "running";

  // 项目切换时清空预览状态并订阅就绪事件。
  useEffect(() => {
    preview.reset();
    let disposed = false;
    let off: (() => void) | null = null;
    void subscribePreviewReady((target) => {
      if (disposed) return;
      const current = useProjectRuntimeStore.getState().run;
      if (current && target.runId === current.runId) {
        useProjectPreviewStore.setState({ target });
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else off = unlisten;
    });
    return () => {
      disposed = true;
      off?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  const stateLabel: Record<string, string> = {
    starting: "启动中",
    running: "运行中",
    stopping: "停止中",
    exited: run?.exitCode === 0 ? "已退出" : "已失败",
    failed: "失败",
  };

  const [envRows, setEnvRows] = useState<{ key: string; value: string; remove: boolean }[]>([]);

  // 环境变量编辑行与配置同步。
  useEffect(() => {
    if (!config) return;
    setEnvRows(
      Object.entries(config.envOverrides ?? {}).map(([key, value]) => ({
        key,
        value: value ?? "",
        remove: value === null,
      })),
    );
  }, [config?.envOverrides]);

  const handleStart = async () => {
    const preview = await store.start();
    if (preview) setShowConfirm(true);
  };

  const applyEnvRows = (rows: { key: string; value: string; remove: boolean }[]) => {
    setEnvRows(rows);
    const overrides: Record<string, string | null> = {};
    for (const row of rows) {
      if (!row.key.trim()) continue;
      overrides[row.key] = row.remove || row.value === "" ? null : row.value;
    }
    store.setConfig({ envOverrides: overrides });
  };

  const copyLogs = async () => {
    const text = store.logs.map((l) => l.text).join("");
    await navigator.clipboard.writeText(text).catch(() => {});
  };

  return (
    <div className="run-panel">
      <div className="run-panel-head">
        <TerminalSquare size={14} color="var(--primary)" />
        <span>运行</span>
        {run && (
          <span className={`run-state-badge ${run.state}`}>
            {stateLabel[run.state] ?? run.state}
          </span>
        )}
        {run?.pid != null && <span className="run-pid">PID {run.pid}</span>}
        {run?.exitCode != null && (
          <span className="run-exit-code">退出码 {run.exitCode}</span>
        )}
      </div>

      {store.error && (
        <div className="run-error-banner">
          <span>{store.error}</span>
          <button
            className="icon-btn"
            onClick={() => useProjectRuntimeStore.setState({ error: null })}
          >
            ×
          </button>
        </div>
      )}

      {store.detecting ? (
        <div className="run-empty">
          <Loader2 size={14} className="spin" /> 正在识别项目…
        </div>
      ) : detection && detection.candidates.length === 0 ? (
        <div className="run-empty">
          未识别到可运行命令。
          {detection.diagnostics.map((d, i) => (
            <div key={i} className="run-diagnostic">
              {d}
            </div>
          ))}
        </div>
      ) : (
        config && (
          <div className="run-config">
            {detection && detection.candidates.length > 0 && (
              <div className="run-candidates">
                {detection.candidates.map((c, i) => (
                  <button
                    key={c.label}
                    className={`run-candidate ${config.executable === c.executable && config.args.join(" ") === c.args.join(" ") ? "active" : ""}`}
                    onClick={() => store.pickCandidate(i)}
                  >
                    {c.label}
                  </button>
                ))}
              </div>
            )}

            <div className="run-field">
              <label>程序</label>
              <input
                value={config.executable}
                onChange={(e) => store.setConfig({ executable: e.target.value })}
                placeholder="例如 node、python、cargo"
              />
            </div>
            <div className="run-field">
              <label>参数</label>
              <input
                value={config.args.join(" ")}
                onChange={(e) =>
                  store.setConfig({ args: e.target.value.split(/\s+/).filter(Boolean) })
                }
                placeholder="空格分隔，例如 run dev"
              />
            </div>
            <div className="run-field">
              <label>工作目录</label>
              <input
                value={config.cwd}
                onChange={(e) => store.setConfig({ cwd: e.target.value })}
                placeholder="留空表示项目根目录"
              />
            </div>

            <div className="run-field-row">
              <div className="run-field">
                <label>端口</label>
                <input
                  type="number"
                  min={1}
                  max={65535}
                  value={config.expectedPort ?? ""}
                  onChange={(e) =>
                    store.setConfig({
                      expectedPort: e.target.value ? Number(e.target.value) : null,
                    })
                  }
                  placeholder="可选"
                />
              </div>
              <div className="run-field">
                <label>协议</label>
                <select
                  value={config.previewScheme}
                  onChange={(e) => store.setConfig({ previewScheme: e.target.value })}
                >
                  <option value="http">http</option>
                  <option value="https">https</option>
                </select>
              </div>
            </div>

            <div className="run-field">
              <label>环境变量</label>
              <div className="run-env-list">
                {envRows.map((row, i) => (
                  <div key={i} className="run-env-row">
                    <input
                      value={row.key}
                      placeholder="变量名"
                      onChange={(e) => {
                        const rows = envRows.map((r, j) => (j === i ? { ...r, key: e.target.value } : r));
                        applyEnvRows(rows);
                      }}
                    />
                    <input
                      value={row.value}
                      placeholder="值（留空表示删除该变量）"
                      onChange={(e) => {
                        const rows = envRows.map((r, j) => (j === i ? { ...r, value: e.target.value } : r));
                        applyEnvRows(rows);
                      }}
                    />
                    <button
                      className="icon-btn"
                      title="删除变量"
                      onClick={() => applyEnvRows(envRows.filter((_, j) => j !== i))}
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                ))}
                <button
                  className="run-env-add"
                  onClick={() => applyEnvRows([...envRows, { key: "", value: "", remove: false }])}
                >
                  <Plus size={12} /> 添加变量
                </button>
              </div>
            </div>

            <div className="run-actions">
              <button
                className="btn-primary run-btn-start"
                disabled={!actions.canStart || store.busy || !config.executable.trim()}
                onClick={() => void handleStart()}
              >
                <Play size={13} /> 启动
              </button>
              <button
                className="btn-secondary"
                disabled={!actions.canStop || store.busy}
                onClick={() => void store.stop()}
              >
                <Square size={13} /> 停止
              </button>
              <button
                className="btn-secondary"
                disabled={!actions.canRestart || store.busy}
                onClick={() => void store.restart()}
              >
                <RotateCw size={13} /> 重启
              </button>
              <button
                className="btn-secondary run-btn-preview"
                disabled={!canPreview || preview.checking}
                onClick={() => void preview.openPreview()}
              >
                <Eye size={13} /> {preview.checking ? "检查中…" : "预览"}
              </button>
            </div>

            {preview.target && (
              <div className="run-preview-target">
                <div className="run-preview-url">
                  <code>{preview.target.url}</code>
                  <span className={`run-ownership-badge ${preview.target.ownership}`}>
                    {ownershipLabel(preview.target.ownership)}
                  </span>
                </div>
                <div className="run-preview-meta">
                  来源：{previewSourceLabel(preview.target.source)} · 端口 {preview.target.port}
                </div>
                {preview.target.ownership === "unconfirmed" && (
                  <div className="run-preview-warn">
                    端口归属未确认：该端口可能被其他进程占用。请确认后手动打开。
                    <button
                      className="run-preview-manual"
                      onClick={() => void preview.openInBrowser()}
                    >
                      手动打开
                    </button>
                  </div>
                )}
              </div>
            )}
            {preview.error && (
              <div className="run-error-line">{preview.error}</div>
            )}

            {run?.errorMessage && (
              <div className="run-error-line">
                {run.errorCode}: {run.errorMessage}
              </div>
            )}
          </div>
        )
      )}

      <ProjectLogViewer logs={store.logs} onClear={store.clearVisibleLogs} onCopy={() => void copyLogs()} />

      {showConfirm && store.confirmation && (
        <RunConfirmationDialog
          preview={store.confirmation}
          busy={store.busy}
          onCancel={() => setShowConfirm(false)}
          onConfirm={() => {
            void store.startWithConfirmation(store.confirmation!.confirmationId);
            setShowConfirm(false);
          }}
        />
      )}
    </div>
  );
}
