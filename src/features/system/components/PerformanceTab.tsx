import { useEffect, useRef, useState } from "react";
import { Activity, ArrowDown, ArrowUp, Cpu, MemoryStick, Network } from "lucide-react";
import type { SystemSnapshot } from "../../../lib/types";
import { call } from "../../../lib/tauri";
import { formatRate, formatSize, PercentBar, SvgSpark } from "../components/shared";

const MAX_POINTS = 60;

export function PerformanceTab() {
  const [snap, setSnap] = useState<SystemSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [cpuHistory, setCpuHistory] = useState<number[]>([]);
  const [memHistory, setMemHistory] = useState<number[]>([]);
  const [netDownHistory, setNetDownHistory] = useState<number[]>([]);
  const [netUpHistory, setNetUpHistory] = useState<number[]>([]);
  const historyRef = useRef({ cpu: [] as number[], mem: [] as number[], down: [] as number[], up: [] as number[] });

  useEffect(() => {
    let stopped = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const s = await call<SystemSnapshot>("get_system_snapshot", {});
        if (stopped) return;
        setSnap(s);
        const h = historyRef.current;
        h.cpu.push(s.cpu_usage);
        h.mem.push(s.mem_percent);
        const down = s.networks.reduce((acc, n) => acc + n.down_rate, 0);
        const up = s.networks.reduce((acc, n) => acc + n.up_rate, 0);
        h.down.push(down);
        h.up.push(up);
        if (h.cpu.length > MAX_POINTS) h.cpu.shift();
        if (h.mem.length > MAX_POINTS) h.mem.shift();
        if (h.down.length > MAX_POINTS) h.down.shift();
        if (h.up.length > MAX_POINTS) h.up.shift();
        setCpuHistory([...h.cpu]);
        setMemHistory([...h.mem]);
        setNetDownHistory([...h.down]);
        setNetUpHistory([...h.up]);
        setError(null);
      } catch (e) {
        if (!stopped) setError((e as Error).message);
      }
    };

    tick();
    timer = window.setInterval(tick, 1000);
    return () => {
      stopped = true;
      if (timer) window.clearInterval(timer);
    };
  }, []);

  const memTotal = snap?.mem_total ?? 0;
  const memUsed = snap?.mem_used ?? 0;

  return (
    <div className="system-tab">
      {error && <div className="system-error">{error}</div>}

      <div className="perf-grid">
        <section className="system-card">
          <h4>
            <Cpu size={14} /> CPU 使用率
          </h4>
          <div className="perf-big">{snap ? `${snap.cpu_usage.toFixed(1)}%` : "-"}</div>
          <PercentBar value={snap?.cpu_usage ?? 0} danger />
          <div className="perf-note">线程数：{snap?.cpu_per_core.length ?? 0}</div>
          <SvgSpark values={cpuHistory} width={260} height={56} />
        </section>

        <section className="system-card">
          <h4>
            <MemoryStick size={14} /> 内存使用
          </h4>
          <div className="perf-big">{snap ? `${snap.mem_percent.toFixed(1)}%` : "-"}</div>
          <PercentBar value={snap?.mem_percent ?? 0} danger />
          <div className="perf-note">
            {memTotal ? `${formatSize(memUsed)} / ${formatSize(memTotal)}` : "-"}
          </div>
          <SvgSpark values={memHistory} width={260} height={56} />
        </section>
      </div>

      <section className="system-card">
        <h4>
          <Network size={14} /> 网络速率
        </h4>
        <div className="net-total">
          <span className="net-chip down">
            <ArrowDown size={13} /> 下载 {formatRate(netDownHistory[netDownHistory.length - 1] ?? 0)}
          </span>
          <span className="net-chip up">
            <ArrowUp size={13} /> 上传 {formatRate(netUpHistory[netUpHistory.length - 1] ?? 0)}
          </span>
        </div>
        <div className="net-sparks">
          <div>
            <SvgSpark values={netDownHistory} width={260} height={48} color="var(--primary)" />
            <span className="perf-note">下载趋势</span>
          </div>
          <div>
            <SvgSpark values={netUpHistory} width={260} height={48} color="var(--success)" />
            <span className="perf-note">上传趋势</span>
          </div>
        </div>
        {snap && snap.networks.length > 0 && (
          <table className="net-table">
            <thead>
              <tr>
                <th>适配器</th>
                <th>下载</th>
                <th>上传</th>
                <th>累计接收</th>
                <th>累计发送</th>
              </tr>
            </thead>
            <tbody>
              {snap.networks.map((n) => (
                <tr key={n.name}>
                  <td className="mono">{n.name}</td>
                  <td>{formatRate(n.down_rate)}</td>
                  <td>{formatRate(n.up_rate)}</td>
                  <td>{formatSize(n.total_down)}</td>
                  <td>{formatSize(n.total_up)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="system-card">
        <h4>
          <Activity size={14} /> 每核心使用率
        </h4>
        {snap ? (
          <div className="core-grid">
            {snap.cpu_per_core.map((v, i) => (
              <div className="core-item" key={i}>
                <span className="core-label">核心 {i + 1}</span>
                <PercentBar value={v} />
                <span className="core-value">{v.toFixed(0)}%</span>
              </div>
            ))}
          </div>
        ) : (
          <div className="perf-note">等待采样…</div>
        )}
      </section>
    </div>
  );
}
