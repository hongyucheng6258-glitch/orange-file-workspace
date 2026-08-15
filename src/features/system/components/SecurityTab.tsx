import { useCallback, useEffect, useState } from "react";
import { RefreshCw, Shield, ShieldAlert, ShieldCheck } from "lucide-react";
import type { SecurityStatus } from "../../../lib/types";
import { call } from "../../../lib/tauri";

interface SecItem {
  label: string;
  ok: boolean;
  desc: string;
}

export function SecurityTab() {
  const [data, setData] = useState<SecurityStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setData(await call<SecurityStatus>("get_security_status", {}));
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

  const items: SecItem[] = data
    ? [
        {
          label: "防火墙（标准配置）",
          ok: data.firewall_standard,
          desc: data.firewall_standard ? "已启用" : "已关闭",
        },
        {
          label: "防火墙（域配置）",
          ok: data.firewall_domain,
          desc: data.firewall_domain ? "已启用" : "已关闭",
        },
        {
          label: "防火墙（公用配置）",
          ok: data.firewall_public,
          desc: data.firewall_public ? "已启用" : "已关闭",
        },
        {
          label: "Windows Defender",
          ok: data.defender_running,
          desc: data.defender_running ? "运行中" : "未运行",
        },
        {
          label: "Windows 更新服务",
          ok: data.windows_update_running,
          desc: data.windows_update_running ? "运行中" : "未运行",
        },
        {
          label: "Secure Boot",
          ok: data.secure_boot,
          desc: data.secure_boot ? "已启用" : "未启用",
        },
        {
          label: "UAC 用户账户控制",
          ok: data.uac_enabled,
          desc: data.uac_enabled ? "已启用" : "已禁用",
        },
        {
          label: "当前运行身份",
          ok: data.running_as_admin,
          desc: data.running_as_admin ? "管理员" : "普通用户",
        },
      ]
    : [];

  const okCount = items.filter((i) => i.ok).length;

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>安全状态</h3>
        <button className="btn btn-ghost" onClick={load} disabled={loading}>
          <RefreshCw size={13} className={loading ? "spin" : ""} /> 刷新
        </button>
      </div>
      {error && <div className="system-error">{error}</div>}

      {data && (
        <div className="sec-summary">
          <span className={`sec-count ${okCount === items.length ? "ok" : ""}`}>
            {okCount === items.length ? <ShieldCheck size={16} /> : <ShieldAlert size={16} />}
            {okCount} / {items.length} 项安全设置正常
          </span>
        </div>
      )}

      <div className="system-cards">
        {items.map((item) => (
          <section className="system-card" key={item.label}>
            <h4>
              {item.ok ? <ShieldCheck size={14} /> : <ShieldAlert size={14} />} {item.label}
            </h4>
            <div className="sec-status-row">
              <span className={`state-badge ${item.ok ? "ok" : "warn"}`}>{item.desc}</span>
            </div>
          </section>
        ))}
      </div>

      {data && (
        <p className="system-hint">
          <Shield size={13} /> 安全状态仅反映本机当前配置，不会上传任何数据。当前以
          {data.running_as_admin ? "管理员" : "普通用户"}身份运行。
        </p>
      )}
    </div>
  );
}
