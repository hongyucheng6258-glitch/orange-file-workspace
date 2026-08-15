import { useCallback, useEffect, useState } from "react";
import { Cpu, MemoryStick, HardDrive, Monitor, RefreshCw } from "lucide-react";
import type { SystemOverview } from "../../../lib/types";
import { call } from "../../../lib/tauri";
import { formatUptime, formatSize, InfoRow, PercentBar } from "../components/shared";

export function OverviewTab() {
  const [data, setData] = useState<SystemOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const d = await call<SystemOverview>("get_system_overview", {});
      setData(d);
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
        <h3>设备概览</h3>
        <button className="btn btn-ghost" onClick={load} disabled={loading}>
          <RefreshCw size={13} className={loading ? "spin" : ""} /> 刷新
        </button>
      </div>
      {error && <div className="system-error">{error}</div>}

      <div className="system-cards">
        <section className="system-card">
          <h4>
            <Monitor size={14} /> 设备
          </h4>
          <InfoRow label="设备名称">{data?.device_name ?? "-"}</InfoRow>
          <InfoRow label="制造商">{data?.manufacturer ?? "-"}</InfoRow>
          <InfoRow label="产品型号">{data?.product_name ?? "-"}</InfoRow>
          <InfoRow label="操作系统">{data?.os_name ?? "-"}</InfoRow>
          <InfoRow label="系统版本">{data?.os_version ?? "-"}</InfoRow>
          <InfoRow label="内核版本">{data?.kernel_version ?? "-"}</InfoRow>
          <InfoRow label="BIOS 版本">{data?.bios_version ?? "-"}</InfoRow>
          <InfoRow label="运行时间">{formatUptime(data?.uptime ?? 0)}</InfoRow>
        </section>

        <section className="system-card">
          <h4>
            <Cpu size={14} /> 处理器
          </h4>
          <InfoRow label="型号">{data?.cpu_brand ?? "-"}</InfoRow>
          <InfoRow label="厂商">{data?.cpu_vendor ?? "-"}</InfoRow>
          <InfoRow label="物理核心">{data?.cpu_cores ?? 0}</InfoRow>
          <InfoRow label="逻辑线程">{data?.cpu_threads ?? 0}</InfoRow>
          <InfoRow label="当前频率">{data?.cpu_frequency ? `${data.cpu_frequency} MHz` : "-"}</InfoRow>
          <InfoRow label="使用率">
            <span className="value-strong">{data ? `${data.cpu_usage.toFixed(1)}%` : "-"}</span>
          </InfoRow>
          <PercentBar value={data?.cpu_usage ?? 0} />
        </section>

        <section className="system-card">
          <h4>
            <MemoryStick size={14} /> 内存
          </h4>
          <InfoRow label="内存总量">{data ? formatSize(data.mem_total) : "-"}</InfoRow>
          <InfoRow label="已使用">{data ? formatSize(data.mem_used) : "-"}</InfoRow>
          <InfoRow label="使用率">
            <span className="value-strong">{data ? `${data.mem_percent.toFixed(1)}%` : "-"}</span>
          </InfoRow>
          <PercentBar value={data?.mem_percent ?? 0} danger />
          <InfoRow label="交换空间">{data ? `${formatSize(data.swap_used)} / ${formatSize(data.swap_total)}` : "-"}</InfoRow>
        </section>

        <section className="system-card">
          <h4>
            <HardDrive size={14} /> 其他
          </h4>
          <InfoRow label="磁盘分区">{data?.disk_count ?? 0}</InfoRow>
          <InfoRow label="进程数">{data?.process_count ?? 0}</InfoRow>
          <InfoRow label="开机时间">
            {data?.boot_time ? new Date(data.boot_time * 1000).toLocaleString("zh-CN") : "-"}
          </InfoRow>
        </section>
      </div>
    </div>
  );
}
