import { useSettingsStore } from "../stores/settingsStore";
import { SaveBadge, SettingRow } from "./SettingRow";
import {
  TERMINAL_THEME_LABELS,
  TERMINAL_THEMES,
  type TerminalThemeId,
} from "../../terminal/lib/terminalThemes";

export function TerminalSettings() {
  const { settings, saveState, update } = useSettingsStore();
  if (!settings) return null;
  const t = settings.terminal;

  const applyTerminal = (key: string, value: unknown) => {
    update(`terminal.${key}`, value);
  };

  return (
    <div className="settings-section">
      <h3>终端</h3>
      <SaveBadge state={saveState} />

      <SettingRow label="字号" desc="终端文字大小（10–24 px）">
        <select
          className="input"
          value={t.font_size}
          onChange={(e) => applyTerminal("font_size", Number(e.target.value))}
        >
          {Array.from({ length: 15 }, (_, i) => i + 10).map((n) => (
            <option key={n} value={n}>
              {n} px
            </option>
          ))}
        </select>
      </SettingRow>

      <SettingRow label="光标样式" desc="光标显示形状，改动立即生效">
        <div className="seg">
          <button
            className={t.cursor_style === "block" ? "active" : ""}
            onClick={() => applyTerminal("cursor_style", "block")}
          >
            方块
          </button>
          <button
            className={t.cursor_style === "bar" ? "active" : ""}
            onClick={() => applyTerminal("cursor_style", "bar")}
          >
            竖线
          </button>
          <button
            className={t.cursor_style === "underline" ? "active" : ""}
            onClick={() => applyTerminal("cursor_style", "underline")}
          >
            下划线
          </button>
        </div>
      </SettingRow>

      <SettingRow label="配色方案" desc="终端背景与前景配色，改动立即生效">
        <select
          className="input"
          value={t.theme}
          onChange={(e) => applyTerminal("theme", e.target.value)}
        >
          {(Object.keys(TERMINAL_THEME_LABELS) as TerminalThemeId[]).map((id) => (
            <option key={id} value={id}>
              {TERMINAL_THEME_LABELS[id]}
            </option>
          ))}
        </select>
      </SettingRow>

      {/* 配色预览 */}
      <div className="terminal-theme-preview">
        <div
          className="terminal-theme-preview-bg"
          style={{ background: TERMINAL_THEMES[t.theme].background }}
        >
          <span style={{ color: TERMINAL_THEMES[t.theme].red }}>PS</span>
          <span style={{ color: TERMINAL_THEMES[t.theme].blue }}>
            {" "}
            C:\Users\asus&gt;{" "}
          </span>
          <span style={{ color: TERMINAL_THEMES[t.theme].foreground }}>
            Get-ChildItem
          </span>
          <span
            className="terminal-theme-preview-cursor"
            style={{
              background: TERMINAL_THEMES[t.theme].cursor,
              width: t.cursor_style === "block" ? "9px" : "2px",
              height: t.cursor_style === "underline" ? "2px" : undefined,
              transform:
                t.cursor_style === "underline" ? "translateY(4px)" : undefined,
            }}
          />
        </div>
      </div>

      <div className="settings-actions">
        <button
          className="btn"
          onClick={() => useSettingsStore.getState().resetCategory("terminal")}
        >
          恢复默认
        </button>
      </div>
    </div>
  );
}
