import { useCallback, useEffect, useState } from "react";
import { HardDrive, RefreshCw } from "lucide-react";
import type { StorageInfo } from "../../../lib/types";
import { call } from "../../../lib/tauri";
import { formatSize, PercentBar } from "../components/shared";

export function StorageTab() {
  const [list, setList] = useState<StorageInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setList(await call<StorageInfo[]>("get_storage_info", {}));
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>存储与磁盘</h3>
        <button className="btn btn-ghost" onClick={load} disabled={loading}>
          <RefreshCw size={13} className={loading ? "spin" : ""} /> 刷新
        </button>
      </div>
      {error && <div className="system-error">{error}</div>}

      <div className="storage-list">
        {list.map((d, i) => {
          const used = d.total_space - d.available_space;
          const percent = d.total_space > 0 ? (used / d.total_space) * 100 : 0;
          return (
            <section className="system-card" key={`${d.name}-${i}`}>
              <h4>
                <HardDrive size={14} /> {d.name || d.mount_point}
                {d.is_removable && <span className="tag tag-warn">可移动</span>}
              </h4>
              <div className="storage-meta">
                <span className="mono">{d.mount_point}</span>
                <span>{d.file_system || "-"}</span>
                <span>{d.kind || "-"}</span>
              </div>
              <PercentBar value={percent} danger />
              <div className="storage-values">
                <span className="value-strong">{percent.toFixed(1)}%</span>
                <span>
                  {formatSize(used)} / {formatSize(d.total_space)}
                </span>
                <span>可用 {formatSize(d.available_space)}</span>
              </div>
            </section>
          );
        })}
        {list.length === 0 && !error && <div className="perf-note">暂无磁盘信息</div>}
      </div>
    </div>
  );
}
