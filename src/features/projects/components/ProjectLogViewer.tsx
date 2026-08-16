import { Copy, Eraser } from "lucide-react";
import { LogEntry } from "../lib/projectRuntime";

/** 运行日志查看器：区分 stdout/stderr，支持清空视图与复制。 */
export function ProjectLogViewer({
  logs,
  onClear,
  onCopy,
}: {
  logs: LogEntry[];
  onClear: () => void;
  onCopy: () => void;
}) {
  return (
    <div className="run-log-viewer">
      <div className="run-log-head">
        <span>运行输出</span>
        <div className="run-log-actions">
          <button className="icon-btn" title="清空视图" onClick={onClear} disabled={logs.length === 0}>
            <Eraser size={13} />
          </button>
          <button className="icon-btn" title="复制日志" onClick={onCopy} disabled={logs.length === 0}>
            <Copy size={13} />
          </button>
        </div>
      </div>
      <div className="run-log-body">
        {logs.length === 0 && <div className="run-log-empty">暂无输出，启动项目后实时显示</div>}
        {logs.map((entry) => (
          <div key={entry.seq} className={`run-log-line ${entry.stream}`}>
            {entry.truncated && <span className="run-log-trunc">[截断] </span>}
            {entry.text}
          </div>
        ))}
      </div>
    </div>
  );
}
