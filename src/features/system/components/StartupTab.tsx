import { useCallback, useEffect, useMemo, useState } from "react";
import { Rocket, Search } from "lucide-react";
import type { StartupItemInfo } from "../../../lib/types";
import { call } from "../../../lib/tauri";

export function StartupTab() {
  const [list, setList] = useState<StartupItemInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const load = useCallback(async () => {
    try {
      setList(await call<StartupItemInfo[]>("get_startup_items", {}));
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
        s.name.toLowerCase().includes(q) || s.command.toLowerCase().includes(q),
    );
  }, [list, query]);

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>启动项（{list.length} 项）</h3>
        <div className="system-tab-actions">
          <div className="filter-box">
            <Search size={13} />
            <input
              placeholder="筛选启动项…"
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
              <th>来源</th>
              <th>命令 / 位置</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((s, i) => (
              <tr key={`${s.name}-${i}`}>
                <td className="proc-name" title={s.name}>
                  <span className="startup-name">
                    <Rocket size={13} /> {s.name}
                  </span>
                </td>
                <td>
                  <span className="tag">{s.source}</span>
                </td>
                <td className="proc-path mono" title={s.command}>
                  {s.command}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
