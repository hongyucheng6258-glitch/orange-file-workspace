import { Copy, Eraser } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { LogEntry } from "../lib/projectRuntime";

type StreamFilter = "all" | "stdout" | "stderr";

/** 运行日志查看器：区分 stdout/stderr，支持流过滤、自动跟随、清空视图与复制。 */
export function ProjectLogViewer({
  logs,
  onClear,
  onCopy,
}: {
  logs: LogEntry[];
  onClear: () => void;
  onCopy: () => void;
}) {
  const [filter, setFilter] = useState<StreamFilter>("all");
  const [follow, setFollow] = useState(true);
  const bodyRef = useRef<HTMLDivElement>(null);

  const visible = useMemo(
    () => (filter === "all" ? logs : logs.filter((l) => l.stream === filter)),
    [logs, filter],
  );

  // 自动跟随尾部：日志增长且开关打开时滚动到底部。
  useEffect(() => {
    const el = bodyRef.current;
    if (el && follow) el.scrollTop = el.scrollHeight;
  }, [visible.length, follow]);

  return (
    <div className="run-log-viewer">
      <div className="run-log-head">
        <span>运行输出</span>
        <div className="run-log-actions">
          <select
            className="run-log-filter"
            value={filter}
            onChange={(e) => setFilter(e.target.value as StreamFilter)}
            title="按输出流过滤"
          >
            <option value="all">全部</option>
            <option value="stdout">stdout</option>
            <option value="stderr">stderr</option>
          </select>
          <label className="run-log-follow" title="新日志到达时自动滚动到底部">
            <input
              type="checkbox"
              checked={follow}
              onChange={(e) => setFollow(e.target.checked)}
            />
            跟随
          </label>
          <button className="icon-btn" title="清空视图" onClick={onClear} disabled={logs.length === 0}>
            <Eraser size={13} />
          </button>
          <button className="icon-btn" title="复制日志" onClick={onCopy} disabled={logs.length === 0}>
            <Copy size={13} />
          </button>
        </div>
      </div>
      <div className="run-log-body" ref={bodyRef}>
        {logs.length === 0 && <div className="run-log-empty">暂无输出，启动项目后实时显示</div>}
        {visible.map((entry) => (
          <div key={entry.seq} className={`run-log-line ${entry.stream}`}>
            {entry.truncated && <span className="run-log-trunc">[截断] </span>}
            {entry.text}
          </div>
        ))}
      </div>
    </div>
  );
}
