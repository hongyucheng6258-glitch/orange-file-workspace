import { useCallback, useEffect, useState } from "react";
import { MonitorCog, RefreshCw } from "lucide-react";
import type { GpuInfo } from "../../../lib/types";
import { call, formatSize } from "../../../lib/tauri";
import { InfoRow } from "../components/shared";

export function HardwareTab() {
  const [list, setList] = useState<GpuInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setList(await call<GpuInfo[]>("get_gpu_info", {}));
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
        <h3>显卡与显示适配器</h3>
        <button className="btn btn-ghost" onClick={load} disabled={loading}>
          <RefreshCw size={13} className={loading ? "spin" : ""} /> 刷新
        </button>
      </div>
      {error && <div className="system-error">{error}</div>}

      <div className="system-cards">
        {list.map((g, i) => (
          <section className="system-card" key={`${g.name}-${i}`}>
            <h4>
              <MonitorCog size={14} /> 显卡 {i + 1}
            </h4>
            <InfoRow label="名称">{g.name}</InfoRow>
            <InfoRow label="厂商">{g.vendor ?? "-"}</InfoRow>
            <InfoRow label="显存">{g.vram_bytes > 0 ? formatSize(g.vram_bytes) : "-"}</InfoRow>
            <InfoRow label="驱动版本">{g.driver_version ?? "-"}</InfoRow>
            <InfoRow label="驱动日期">{g.driver_date ?? "-"}</InfoRow>
          </section>
        ))}
        {list.length === 0 && !error && <div className="perf-note">未检测到显卡信息</div>}
      </div>
    </div>
  );
}
