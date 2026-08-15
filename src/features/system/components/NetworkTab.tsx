import { useCallback, useEffect, useState } from "react";
import { Globe, RefreshCw } from "lucide-react";
import type { NetworkAdapterInfo } from "../../../lib/types";
import { call } from "../../../lib/tauri";

function formatSpeed(bps: number): string {
  if (!bps) return "-";
  const mbps = bps / 1_000_000;
  if (mbps >= 1000) return `${(mbps / 1000).toFixed(2)} Gbps`;
  return `${Math.round(mbps)} Mbps`;
}

export function NetworkTab() {
  const [list, setList] = useState<NetworkAdapterInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setList(await call<NetworkAdapterInfo[]>("get_network_adapters", {}));
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
        <h3>网络适配器</h3>
        <button className="btn btn-ghost" onClick={load} disabled={loading}>
          <RefreshCw size={13} className={loading ? "spin" : ""} /> 刷新
        </button>
      </div>
      {error && <div className="system-error">{error}</div>}

      <div className="storage-list">
        {list.map((a, i) => (
          <section className="system-card" key={`${a.name}-${i}`}>
            <h4>
              <Globe size={14} /> {a.friendly_name || a.name || "适配器"}
              {a.status === "已连接" && <span className="tag tag-ok">{a.status}</span>}
              {a.status === "未连接" && <span className="tag">{a.status}</span>}
            </h4>
            <div className="storage-meta">
              <span>{a.status}</span>
              <span>{formatSpeed(a.speed_bps)}</span>
              <span className="mono">{a.mac ?? "无 MAC"}</span>
            </div>
            <div className="net-ip">
              {a.ipv4.length > 0 && (
                <div className="net-ip-row">
                  <span className="info-label">IPv4</span>
                  <span className="mono">{a.ipv4.join(", ")}</span>
                </div>
              )}
              {a.ipv6.length > 0 && (
                <div className="net-ip-row">
                  <span className="info-label">IPv6</span>
                  <span className="mono ip6">{a.ipv6.join(", ")}</span>
                </div>
              )}
              {a.ipv4.length === 0 && a.ipv6.length === 0 && (
                <div className="perf-note">无已配置地址</div>
              )}
              {a.name && a.friendly_name !== a.name && (
                <div className="net-ip-row">
                  <span className="info-label">描述</span>
                  <span>{a.name}</span>
                </div>
              )}
            </div>
          </section>
        ))}
        {list.length === 0 && !error && <div className="perf-note">未检测到网络适配器</div>}
      </div>
    </div>
  );
}
