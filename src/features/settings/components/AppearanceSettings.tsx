import { useSettingsStore } from "../stores/settingsStore";
import { applyTheme } from "../../../lib/theme";
import { SaveBadge, SettingRow } from "./SettingRow";

export function AppearanceSettings() {
  const { settings, saveState, update } = useSettingsStore();
  if (!settings) return null;
  const a = settings.appearance;

  const setTheme = (mode: "system" | "light" | "dark") => {
    applyTheme(mode); // 立即生效
    update("appearance.theme_mode", mode);
  };

  return (
    <div className="settings-section">
      <h3>外观</h3>
      <SaveBadge state={saveState} />

      <SettingRow label="主题模式" desc="选择界面配色">
        <div className="seg">
          <button
            className={a.theme_mode === "system" ? "active" : ""}
            onClick={() => setTheme("system")}
          >
            跟随系统
          </button>
          <button
            className={a.theme_mode === "light" ? "active" : ""}
            onClick={() => setTheme("light")}
          >
            浅色
          </button>
          <button
            className={a.theme_mode === "dark" ? "active" : ""}
            onClick={() => setTheme("dark")}
          >
            深色
          </button>
        </div>
      </SettingRow>

      <SettingRow label="界面紧凑度" desc="列表与工具栏的间距密度">
        <select
          className="input"
          value={a.density}
          onChange={(e) => update("appearance.density", e.target.value)}
        >
          <option value="comfortable">舒适</option>
          <option value="compact">紧凑</option>
        </select>
      </SettingRow>

      <div className="settings-actions">
        <button
          className="btn"
          onClick={() => {
            applyTheme("system");
            useSettingsStore.getState().resetCategory("appearance");
          }}
        >
          恢复默认
        </button>
      </div>
    </div>
  );
}
