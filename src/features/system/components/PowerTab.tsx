import { useCallback, useEffect, useState } from "react";
import { BatteryCharging, BatteryLow, Plug, RefreshCw, Timer } from "lucide-react";
import type { BatteryInfo } from "../../../lib/types";
import { call } from "../../../lib/tauri";
import { InfoRow, PercentBar } from "../components/shared";

export function PowerTab() {
  const [data, setData] = useState<BatteryInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setData(await call<BatteryInfo>("get_battery_info", {}));
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

  const minutesLeft = data?.life_time_secs
    ? Math.round(data.life_time_secs / 60)
    : null;

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>电源与电池</h3>
        <button className="btn btn-ghost" onClick={load} disabled={loading}>
          <RefreshCw size={13} className={loading ? "spin" : ""} /> 刷新
        </button>
      </div>
      {error && <div className="system-error">{error}</div>}

      <div className="system-cards">
        <section className="system-card">
          <h4>
            {data?.charging ? <BatteryCharging size={14} /> : <BatteryLow size={14} />}{" "}
            电池状态
          </h4>
          {data?.percent != null ? (
            <>
              <div className="perf-big">{data.percent}%</div>
              <PercentBar value={data.percent} danger={data.percent <= 20} />
            </>
          ) : (
            <div className="perf-note">当前设备未检测到电池（可能为台式机）</div>
          )}
          <InfoRow label="电源状态">{data?.ac_status ?? "-"}</InfoRow>
          <InfoRow label="充电中">{data?.charging ? "是" : "否"}</InfoRow>
          <InfoRow label="剩余续航">
            {minutesLeft != null ? `${minutesLeft} 分钟` : "-"}
          </InfoRow>
        </section>

        <section className="system-card">
          <h4>
            <Plug size={14} /> 说明
          </h4>
          <p className="system-hint">
            电池信息来自 Windows 电源状态接口，仅反映当前瞬间状态。
            {data?.charging
              ? " 当前正在充电，剩余续航估算会随充电状态变化。"
              : ""}
          </p>
          <div className="perf-note">
            <Timer size={12} /> 若设备始终显示"未检测到电池"，请确认电源管理中启用了电池驱动。
          </div>
        </section>
      </div>
    </div>
  );
}
