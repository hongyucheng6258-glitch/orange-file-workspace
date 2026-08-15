import { useCallback, useEffect, useState } from "react";
import { RefreshCw, ShieldCheck, ShieldAlert, ShieldX } from "lucide-react";
import type { AlertRules, HealthItem } from "../../../lib/types";
import { call } from "../../../lib/tauri";
import { loadAlertRules } from "../lib/alerts";
import { AlertRulesPanel } from "./AlertRulesPanel";

const LEVEL_META: Record<HealthItem["level"], { label: string; icon: typeof ShieldCheck }> = {
  ok: { label: "正常", icon: ShieldCheck },
  warning: { label: "注意", icon: ShieldAlert },
  danger: { label: "风险", icon: ShieldX },
};

export function HealthTab() {
  const [list, setList] = useState<HealthItem[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [rules, setRules] = useState<AlertRules>(loadAlertRules);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setList(await call<HealthItem[]>("get_system_health", {}));
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

  const danger = list.filter((i) => i.level === "danger").length;
  const warning = list.filter((i) => i.level === "warning").length;

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>异常检测与系统健康</h3>
        <button className="btn btn-ghost" onClick={load} disabled={loading}>
          <RefreshCw size={13} className={loading ? "spin" : ""} /> 重新检测
        </button>
      </div>
      {error && <div className="system-error">{error}</div>}

      <AlertRulesPanel rules={rules} onChange={setRules} />

      {list.length > 0 && (
        <div className="health-summary">
          <span className="health-chip ok">正常 {list.length - danger - warning}</span>
          <span className={`health-chip warn${warning > 0 ? "" : " muted"}`}>
            注意 {warning}
          </span>
          <span className={`health-chip danger${danger > 0 ? "" : " muted"}`}>
            风险 {danger}
          </span>
        </div>
      )}

      <div className="health-list">
        {list.map((item, i) => {
          const meta = LEVEL_META[item.level];
          const Icon = meta.icon;
          return (
            <section className={`health-item ${item.level}`} key={i}>
              <span className="health-icon">
                <Icon size={15} />
              </span>
              <div className="health-body">
                <div className="health-title-row">
                  <span className="health-title">{item.title}</span>
                  <span className={`state-badge ${item.level}`}>{meta.label}</span>
                </div>
                <div className="health-detail">{item.detail}</div>
              </div>
            </section>
          );
        })}
        {list.length === 0 && !error && <div className="perf-note">正在检测…</div>}
      </div>
    </div>
  );
}
