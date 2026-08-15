import { useCallback, useEffect, useMemo, useState } from "react";
import { Play, Search, Square } from "lucide-react";
import type { ServiceInfo } from "../../../lib/types";
import { call } from "../../../lib/tauri";

export function ServicesTab() {
  const [list, setList] = useState<ServiceInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [busyName, setBusyName] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setList(await call<ServiceInfo[]>("get_services", {}));
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return list;
    return list.filter(
      (s) =>
        s.name.toLowerCase().includes(q) || s.display_name.toLowerCase().includes(q),
    );
  }, [list, query]);

  const doAction = async (s: ServiceInfo, action: "start" | "stop") => {
    const label = action === "start" ? "启动" : "停止";
    if (
      !window.confirm(
        `确定要${label}服务“${s.display_name || s.name}”（${s.name}）吗？\n部分系统服务不可随意启停。`,
      )
    ) {
      return;
    }
    setBusyName(s.name);
    setMessage(null);
    try {
      await call(action === "start" ? "start_service" : "stop_service", { name: s.name });
      setMessage(`已发送${label}请求：${s.display_name || s.name}`);
      await load();
    } catch (e) {
      setMessage(`${label}失败：${(e as Error).message}`);
    } finally {
      setBusyName(null);
    }
  };

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>Windows 服务（{list.length} 项）</h3>
        <div className="system-tab-actions">
          <div className="filter-box">
            <Search size={13} />
            <input
              placeholder="筛选服务名称…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <button className="btn btn-ghost" onClick={load}>
            刷新
          </button>
        </div>
      </div>
      {error && <div className="system-error">{error}</div>}
      {message && <div className="system-message">{message}</div>}

      <div className="table-scroll">
        <table className="system-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>显示名称</th>
              <th>状态</th>
              <th>启动类型</th>
              <th className="th-action">操作</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((s) => (
              <tr key={s.name}>
                <td className="mono">{s.name}</td>
                <td className="proc-name" title={s.display_name}>
                  {s.display_name || "-"}
                </td>
                <td>
                  <span className={`state-badge ${s.state === "运行中" ? "ok" : ""}`}>
                    {s.state}
                  </span>
                </td>
                <td>{s.start_type}</td>
                <td>
                  <div className="row-actions">
                    {s.state !== "运行中" && (
                      <button
                        className="btn btn-ghost btn-ok-text"
                        disabled={busyName === s.name}
                        onClick={() => doAction(s, "start")}
                      >
                        <Play size={13} /> 启动
                      </button>
                    )}
                    {s.state === "运行中" && (
                      <button
                        className="btn btn-ghost btn-danger-text"
                        disabled={busyName === s.name}
                        onClick={() => doAction(s, "stop")}
                      >
                        <Square size={13} /> 停止
                      </button>
                    )}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
