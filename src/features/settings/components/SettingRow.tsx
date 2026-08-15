import { CheckCircle2, AlertCircle, Loader2 } from "lucide-react";
import type { SaveState } from "../stores/settingsStore";

export function SaveBadge({ state }: { state: SaveState }) {
  if (state.saving) {
    return (
      <span className="save-badge saving">
        <Loader2 size={12} className="spin" /> 保存中
      </span>
    );
  }
  if (state.error) {
    return (
      <span className="save-badge error">
        <AlertCircle size={12} /> {state.error}
      </span>
    );
  }
  if (state.okAt) {
    return (
      <span className="save-badge ok">
        <CheckCircle2 size={12} /> 已保存
      </span>
    );
  }
  return null;
}

export function SettingRow({
  label,
  desc,
  children,
}: {
  label: string;
  desc?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="setting-row">
      <div className="setting-row-label">
        <span className="setting-row-name">{label}</span>
        {desc && <span className="setting-row-desc">{desc}</span>}
      </div>
      <div className="setting-row-control">{children}</div>
    </div>
  );
}
