import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { useSettingsStore, type IgnoreRule } from "../stores/settingsStore";
import { SaveBadge, SettingRow } from "./SettingRow";
import { SearchExcludeSettings } from "./SearchExcludeSettings";

const BUILTIN_RULES = [
  "node_modules",
  ".git",
  "target",
  "dist",
  ".cache",
  "__pycache__",
  ".venv",
  "venv",
  ".idea",
  ".vscode",
  ".next",
  "build",
];

export function IgnoreRulesSettings() {
  const { settings, saveState, update } = useSettingsStore();
  const [pattern, setPattern] = useState("");
  const [kind, setKind] = useState<"name" | "path">("name");

  if (!settings) return null;
  const rules = settings.ignore.custom_rules;

  const saveRules = (next: IgnoreRule[]) => {
    update("ignore.custom_rules", next);
  };

  const addRule = () => {
    const trimmed = pattern.trim();
    if (!trimmed) return;
    saveRules([...rules, { kind, pattern: trimmed, enabled: true }]);
    setPattern("");
  };

  const removeRule = (index: number) => {
    saveRules(rules.filter((_, i) => i !== index));
  };

  const toggleRule = (index: number) => {
    saveRules(rules.map((r, i) => (i === index ? { ...r, enabled: !r.enabled } : r)));
  };

  return (
    <>
      <div className="settings-section">
        <h3>忽略规则</h3>
        <SaveBadge state={saveState} />

        <SettingRow label="内置规则" desc="导入时始终跳过的常见构建与依赖目录">
          <div className="ignore-chips">
            {BUILTIN_RULES.map((r) => (
              <span key={r} className="ignore-chip">
                {r}
              </span>
            ))}
          </div>
        </SettingRow>

        <SettingRow label="自定义规则" desc="name 匹配文件名/目录名，path 匹配路径片段">
          <div className="ignore-add">
            <select
              className="input"
              value={kind}
              onChange={(e) => setKind(e.target.value as "name" | "path")}
            >
              <option value="name">名称</option>
              <option value="path">路径</option>
            </select>
            <input
              className="input"
              placeholder="例如 *.tmp 或 node_modules"
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addRule()}
            />
            <button className="btn btn-primary" onClick={addRule} disabled={!pattern.trim()}>
              <Plus size={14} /> 添加
            </button>
          </div>
        </SettingRow>

        {rules.length > 0 && (
          <div className="ignore-list">
            {rules.map((r, i) => (
              <div key={i} className="ignore-item">
                <label className="ignore-check">
                  <input type="checkbox" checked={r.enabled} onChange={() => toggleRule(i)} />
                  <span className={`ignore-kind ${r.kind}`}>
                    {r.kind === "name" ? "名称" : "路径"}
                  </span>
                  <span className="ignore-pattern">{r.pattern}</span>
                </label>
                <button className="icon-btn" title="删除规则" onClick={() => removeRule(i)}>
                  <Trash2 size={14} />
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="settings-actions">
          <button
            className="btn"
            onClick={() => useSettingsStore.getState().resetCategory("ignore")}
          >
            清空自定义规则
          </button>
        </div>
      </div>

      <SearchExcludeSettings />
    </>
  );
}
