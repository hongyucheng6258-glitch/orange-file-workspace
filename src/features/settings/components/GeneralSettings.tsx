import { useSettingsStore } from "../stores/settingsStore";
import { SaveBadge, SettingRow } from "./SettingRow";

export function GeneralSettings() {
  const { settings, saveState, update } = useSettingsStore();
  if (!settings) return null;
  const g = settings.general;

  return (
    <div className="settings-section">
      <h3>通用</h3>
      <SaveBadge state={saveState} />

      <SettingRow label="启动后展示" desc="应用启动后默认进入的页面">
        <select
          className="input"
          value={g.launch_behavior}
          onChange={(e) => update("general.launch_behavior", e.target.value)}
        >
          <option value="home">首页</option>
          <option value="files">文件</option>
        </select>
      </SettingRow>

      <SettingRow label="默认导入方式" desc="导入文件时默认选择的模式">
        <select
          className="input"
          value={g.default_import_mode}
          onChange={(e) => update("general.default_import_mode", e.target.value)}
        >
          <option value="managed">复制到仓库</option>
          <option value="external">保留原位置</option>
        </select>
      </SettingRow>

      <SettingRow label="重复文件策略" desc="导入路径与已有文件相同时的处理方式">
        <select
          className="input"
          value={g.duplicate_policy}
          onChange={(e) => update("general.duplicate_policy", e.target.value)}
        >
          <option value="skip">跳过重复文件</option>
          <option value="keep_both">保留两者</option>
        </select>
      </SettingRow>

      <SettingRow label="文本预览上限" desc="文本类文件预览的最大体积（MB）">
        <input
          className="input"
          type="number"
          min={1}
          max={1024}
          value={g.preview_size_limit_mb}
          onChange={(e) => {
            const n = Number(e.target.value);
            if (Number.isFinite(n) && n >= 1 && n <= 1024) {
              update("general.preview_size_limit_mb", n);
            }
          }}
        />
      </SettingRow>

      <div className="settings-actions">
        <button
          className="btn"
          onClick={() => useSettingsStore.getState().resetCategory("general")}
        >
          恢复默认
        </button>
      </div>
    </div>
  );
}
