import { useCallback, useEffect, useMemo, useState } from "react";
import { Search } from "lucide-react";
import type { DriverInfo } from "../../../lib/types";
import { call } from "../../../lib/tauri";

export function DriversTab() {
  const [list, setList] = useState<DriverInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const load = useCallback(async () => {
    try {
      setList(await call<DriverInfo[]>("get_drivers", {}));
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
      (d) =>
        d.name.toLowerCase().includes(q) || d.display_name.toLowerCase().includes(q),
    );
  }, [list, query]);

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>内核与文件系统驱动（{list.length} 项）</h3>
        <div className="system-tab-actions">
          <div className="filter-box">
            <Search size={13} />
            <input
              placeholder="筛选驱动名称…"
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

      <div className="table-scroll">
        <table className="system-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>显示名称</th>
              <th>状态</th>
              <th>启动类型</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((d) => (
              <tr key={d.name}>
                <td className="mono">{d.name}</td>
                <td className="proc-name" title={d.display_name}>
                  {d.display_name || "-"}
                </td>
                <td>
                  <span className={`state-badge ${d.state === "运行中" ? "ok" : ""}`}>
                    {d.state}
                  </span>
                </td>
                <td>{d.start_type}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
