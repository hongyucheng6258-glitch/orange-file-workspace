import { useEffect, useState } from "react";
import { useSettingsStore } from "../stores/settingsStore";
import { SettingsTabs, type SettingsTab } from "../components/SettingsTabs";
import { GeneralSettings } from "../components/GeneralSettings";
import { AppearanceSettings } from "../components/AppearanceSettings";
import { TerminalSettings } from "../components/TerminalSettings";
import { StorageSettings } from "../components/StorageSettings";
import { BackupSettings } from "../components/BackupSettings";
import { IgnoreRulesSettings } from "../components/IgnoreRulesSettings";
import { AboutSettings } from "../components/AboutSettings";

export function SettingsPage() {
  const [tab, setTab] = useState<SettingsTab>("general");
  const { load, loading, saveState } = useSettingsStore();

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="settings-page">
      <div className="settings-head">
        <h2>设置</h2>
        {saveState.error && (
          <div className="settings-error" role="alert">
            {saveState.error}
          </div>
        )}
      </div>
      <SettingsTabs active={tab} onChange={setTab} />
      {loading ? (
        <div className="empty-state">加载设置中…</div>
      ) : (
        <div className="settings-body">
          {tab === "general" && <GeneralSettings />}
          {tab === "appearance" && <AppearanceSettings />}
          {tab === "terminal" && <TerminalSettings />}
          {tab === "storage" && <StorageSettings />}
          {tab === "ignore" && <IgnoreRulesSettings />}
          {tab === "backup" && <BackupSettings />}
          {tab === "about" && <AboutSettings />}
        </div>
      )}
    </div>
  );
}
