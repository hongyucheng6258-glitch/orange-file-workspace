import { useCallback, useEffect, useState } from "react";
import { Activity, Gauge, HardDrive, RefreshCw, Thermometer } from "lucide-react";
import type { DiskHealthInfo, GpuMetric, TemperatureInfo } from "../../../lib/types";
import { call, formatSize } from "../../../lib/tauri";
import { PercentBar } from "../components/shared";

const HEALTH_LEVEL: Record<string, string> = {
  健康: "ok",
  警告: "warn",
  不健康: "danger",
};

function tempClass(c: number): string {
  if (c >= 85) return "danger";
  if (c >= 70) return "warn";
  return "ok";
}

export function SensorsTab() {
  const [temps, setTemps] = useState<TemperatureInfo[]>([]);
  const [disks, setDisks] = useState<DiskHealthInfo[]>([]);
  const [gpus, setGpus] = useState<GpuMetric[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [t, d, g] = await Promise.all([
        call<TemperatureInfo[]>("get_temperature_info", {}),
        call<DiskHealthInfo[]>("get_disk_health", {}),
        call<GpuMetric[]>("get_gpu_metrics", {}),
      ]);
      setTemps(t);
      setDisks(d);
      setGpus(g);
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
        <h3>传感器与磁盘健康</h3>
        <button className="btn btn-ghost" onClick={load} disabled={loading}>
          <RefreshCw size={13} className={loading ? "spin" : ""} /> 刷新
        </button>
      </div>
      {error && <div className="system-error">{error}</div>}

      <section className="system-card">
        <h4>
          <Thermometer size={14} /> 温度传感器
        </h4>
        {temps.length > 0 ? (
          <div className="temp-grid">
            {temps.map((t, i) => (
              <div className="temp-item" key={`${t.label}-${i}`}>
                <span className="temp-label">{t.label}</span>
                <span className={`temp-value ${t.temperature_c != null ? tempClass(t.temperature_c) : ""}`}>
                  {t.temperature_c != null ? `${t.temperature_c.toFixed(1)}°C` : "未知"}
                </span>
                {t.max_c != null && <span className="temp-max">上限 {t.max_c.toFixed(0)}°C</span>}
              </div>
            ))}
          </div>
        ) : (
          <div className="perf-note">
            未检测到温度传感器。Windows 温度信息通常依赖 ACPI，台式机与部分硬件可能不提供。
          </div>
        )}
      </section>

      <section className="system-card">
        <h4>
          <Gauge size={14} /> GPU 显存使用
        </h4>
        <div className="gpu-grid">
          {gpus.map((g, i) => (
            <div className="gpu-item" key={`${g.name}-${i}`}>
              <div className="gpu-name" title={g.name}>
                {g.name}
              </div>
              {g.vram_total > 0 ? (
                <>
                  <PercentBar value={g.vram_percent} danger />
                  <div className="gpu-values">
                    <span className="value-strong">{g.vram_percent.toFixed(1)}%</span>
                    <span>
                      {formatSize(g.vram_used)} / {formatSize(g.vram_total)}
                    </span>
                  </div>
                </>
              ) : (
                <div className="perf-note">显存使用量不可用</div>
              )}
            </div>
          ))}
          {gpus.length === 0 && !error && <div className="perf-note">未检测到显卡</div>}
        </div>
      </section>

      <section className="system-card">
        <h4>
          <HardDrive size={14} /> 磁盘健康（SMART）
        </h4>
        <div className="disk-health-grid">
          {disks.map((d, i) => (
            <div className="disk-health-item" key={`${d.mount_point}-${i}`}>
              <div className="disk-health-head">
                <span className="mono">{d.mount_point}</span>
                <span className={`state-badge ${HEALTH_LEVEL[d.health_status] ?? ""}`}>
                  {d.health_status}
                </span>
              </div>
              <div className="disk-health-name" title={d.name}>
                {d.name || "-"}
              </div>
            </div>
          ))}
          {disks.length === 0 && !error && <div className="perf-note">未检测到磁盘</div>}
        </div>
        <p className="system-hint">
          <Activity size={12} /> 健康状态来自存储设备管理接口；部分硬盘或驱动不支持时显示“不支持或无法读取”。
        </p>
      </section>
    </div>
  );
}
