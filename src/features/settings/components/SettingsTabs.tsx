import { Settings, Palette, HardDrive, Filter, Archive, Info } from "lucide-react";

export type SettingsTab =
  | "general"
  | "appearance"
  | "storage"
  | "ignore"
  | "backup"
  | "about";

const TABS: { id: SettingsTab; label: string; icon: React.ReactNode }[] = [
  { id: "general", label: "通用", icon: <Settings size={14} /> },
  { id: "appearance", label: "外观", icon: <Palette size={14} /> },
  { id: "storage", label: "存储", icon: <HardDrive size={14} /> },
  { id: "ignore", label: "忽略规则", icon: <Filter size={14} /> },
  { id: "backup", label: "备份与恢复", icon: <Archive size={14} /> },
  { id: "about", label: "关于", icon: <Info size={14} /> },
];

export function SettingsTabs({
  active,
  onChange,
}: {
  active: SettingsTab;
  onChange: (tab: SettingsTab) => void;
}) {
  return (
    <div className="settings-tabs scrollable">
      {TABS.map((t) => (
        <button
          key={t.id}
          className={`settings-tab-btn${active === t.id ? " active" : ""}`}
          onClick={() => onChange(t.id)}
        >
          {t.icon}
          {t.label}
        </button>
      ))}
    </div>
  );
}
