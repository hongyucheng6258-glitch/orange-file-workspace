import { BellRing } from "lucide-react";
import type { AlertRule, AlertRules } from "../../../lib/types";
import { saveAlertRules } from "../lib/alerts";

const FIELDS: { key: keyof AlertRules; label: string; unit: string; hint: string }[] = [
  { key: "cpu", label: "CPU 使用率", unit: "%", hint: "超过阈值时提醒" },
  { key: "mem", label: "内存使用率", unit: "%", hint: "超过阈值时提醒" },
  { key: "disk", label: "磁盘使用率", unit: "%", hint: "任一分区超过阈值时提醒" },
  { key: "temp", label: "设备温度", unit: "°C", hint: "超过阈值时提醒（需传感器支持）" },
];

export function AlertRulesPanel({
  rules,
  onChange,
}: {
  rules: AlertRules;
  onChange: (r: AlertRules) => void;
}) {
  const update = (key: keyof AlertRules, patch: Partial<AlertRule>) => {
    const next = { ...rules, [key]: { ...rules[key], ...patch } };
    saveAlertRules(next);
    onChange(next);
  };

  return (
    <section className="system-card alert-panel">
      <h4>
        <BellRing size={14} /> 告警规则
      </h4>
      <p className="system-hint">
        每 5 秒自动检测一次，超出阈值时在当前页面顶部提示（同一规则 5 分钟内只提醒一次）。
      </p>
      <div className="alert-rule-list">
        {FIELDS.map(({ key, label, unit, hint }) => (
          <div className="alert-rule-row" key={key}>
            <label className="alert-check">
              <input
                type="checkbox"
                checked={rules[key].enabled}
                onChange={(e) => update(key, { enabled: e.target.checked })}
              />
              <span>{label}</span>
            </label>
            <div className="alert-input">
              <input
                type="number"
                min={1}
                max={key === "temp" ? 200 : 100}
                value={rules[key].threshold}
                disabled={!rules[key].enabled}
                onChange={(e) =>
                  update(key, { threshold: Number(e.target.value) || 0 })
                }
              />
              <span>{unit}</span>
            </div>
            <span className="alert-hint">{hint}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
