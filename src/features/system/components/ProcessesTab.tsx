import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCw, XCircle } from "lucide-react";
import type { ProcessInfo } from "../../../lib/types";
import { call } from "../../../lib/tauri";
import { formatSize } from "../components/shared";

type SortKey = "cpu" | "memory";

export function ProcessesTab() {
  const [list, setList] = useState<ProcessInfo[]>([]);
  const [sortBy, setSortBy] = useState<SortKey>("cpu");
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busyPid, setBusyPid] = useState<number | null>(null);
  const sortRef = useRef<SortKey>("cpu");

  const load = useCallback(async () => {
    try {
      setList(await call<ProcessInfo[]>("get_processes", { sortBy: sortRef.current, limit: 200 }));
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);

  useEffect(() => {
    load();
    const timer = window.setInterval(load, 3000);
    return () => window.clearInterval(timer);
  }, [load]);

  const changeSort = (key: SortKey) => {
    sortRef.current = key;
    setSortBy(key);
    load();
  };

  const doKill = async (p: ProcessInfo) => {
    const name = p.name || `PID ${p.pid}`;
    if (!window.confirm(`确定要结束进程“${name}”（PID ${p.pid}）吗？\n未保存的数据可能会丢失。`)) {
      return;
    }
    setBusyPid(p.pid);
    setMessage(null);
    try {
      await call("kill_process", { pid: p.pid });
      setMessage(`已结束进程 ${name}`);
      await load();
    } catch (e) {
      setMessage(`结束失败：${(e as Error).message}`);
    } finally {
      setBusyPid(null);
    }
  };

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>进程列表（每 3 秒刷新）</h3>
        <div className="system-tab-actions">
          <div className="seg">
            <button className={sortBy === "cpu" ? "active" : ""} onClick={() => changeSort("cpu")}>
              按 CPU
            </button>
            <button className={sortBy === "memory" ? "active" : ""} onClick={() => changeSort("memory")}>
              按内存
            </button>
          </div>
          <button className="btn btn-ghost" onClick={load}>
            <RefreshCw size={13} /> 刷新
          </button>
        </div>
      </div>
      {error && <div className="system-error">{error}</div>}
      {message && <div className="system-message">{message}</div>}

      <div className="table-scroll">
        <table className="system-table">
          <thead>
            <tr>
              <th>PID</th>
              <th>名称</th>
              <th>CPU</th>
              <th>内存</th>
              <th>状态</th>
              <th>用户</th>
              <th>路径</th>
              <th className="th-action">操作</th>
            </tr>
          </thead>
          <tbody>
            {list.map((p) => (
              <tr key={p.pid}>
                <td className="mono">{p.pid}</td>
                <td className="proc-name" title={p.path ?? p.name}>
                  {p.name}
                </td>
                <td className="mono">{p.cpu_usage.toFixed(1)}%</td>
                <td className="mono">{formatSize(p.memory)}</td>
                <td>{p.status}</td>
                <td>{p.user ?? "-"}</td>
                <td className="proc-path mono" title={p.path ?? ""}>
                  {p.path ?? "-"}
                </td>
                <td>
                  <button
                    className="btn btn-ghost btn-danger-text"
                    disabled={busyPid === p.pid}
                    onClick={() => doKill(p)}
                  >
                    <XCircle size={13} /> 结束
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
