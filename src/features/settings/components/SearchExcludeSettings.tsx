import { useEffect, useState } from "react";
import { FolderSearch, Plus, X } from "lucide-react";
import { call } from "../../../lib/tauri";
import { SaveBadge, SettingRow } from "./SettingRow";
import type { SaveState } from "../stores/settingsStore";

interface SearchSettings {
  excluded_dirs: string[];
}

const DESC = "排除的目录不会出现在全局搜索结果中，更改后下次扫描生效（无需手动重建）";
const EMPTY_HINT = "未设置排除目录，使用默认排除（回收站、系统卷信息、临时目录）";

export function SearchExcludeSettings() {
  const [dirs, setDirs] = useState<string[]>([]);
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<SaveState>({
    saving: false,
    error: null,
    okAt: null,
  });

  // 初始加载：失败静默，按空列表处理（显示默认排除提示）
  useEffect(() => {
    let disposed = false;
    call<SearchSettings>("get_search_settings", {})
      .then((s) => {
        if (!disposed) setDirs(s.excluded_dirs ?? []);
      })
      .catch(() => {
        if (!disposed) setDirs([]);
      });
    return () => {
      disposed = true;
    };
  }, []);

  const persist = async (next: string[]) => {
    setSaveState({ saving: true, error: null, okAt: null });
    setError(null);
    try {
      const s = await call<SearchSettings>("update_search_settings", {
        settings: { excluded_dirs: next },
      });
      setDirs(s.excluded_dirs ?? next);
      setSaveState({ saving: false, error: null, okAt: Date.now() });
    } catch (e) {
      setSaveState({ saving: false, error: (e as Error).message, okAt: null });
    }
  };

  const addDir = () => {
    const trimmed = input.trim();
    if (!trimmed || saveState.saving) return;
    // Windows 路径不区分大小写，去重时忽略大小写
    if (dirs.some((d) => d.toLowerCase() === trimmed.toLowerCase())) {
      setError("该目录已在排除列表中");
      return;
    }
    persist([...dirs, trimmed]);
    setInput("");
  };

  const removeDir = (index: number) => {
    if (saveState.saving) return;
    persist(dirs.filter((_, i) => i !== index));
  };

  return (
    <div className="settings-section">
      <h3>
        <FolderSearch size={14} /> 搜索排除目录
      </h3>
      <SaveBadge state={saveState} />

      <SettingRow label="排除目录" desc={DESC}>
        <div className="exclude-chips">
          {dirs.map((d, i) => (
            <span key={`${d}#${i}`} className="exclude-chip" title={d}>
              <span className="exclude-chip-path">{d}</span>
              <button
                className="exclude-chip-remove"
                title="移除排除目录"
                disabled={saveState.saving}
                onClick={() => removeDir(i)}
              >
                <X size={11} />
              </button>
            </span>
          ))}
        </div>
      </SettingRow>

      {dirs.length === 0 && <p className="settings-note">{EMPTY_HINT}</p>}

      <SettingRow label="添加排除目录" desc="输入要排除的目录绝对路径">
        <div className="ignore-add">
          <input
            className="input exclude-add-input"
            placeholder="例如 D:\Downloads"
            value={input}
            onChange={(e) => {
              setInput(e.target.value);
              if (error) setError(null);
            }}
            onKeyDown={(e) => e.key === "Enter" && addDir()}
          />
          <button
            className="btn btn-primary"
            onClick={addDir}
            disabled={saveState.saving || !input.trim()}
          >
            <Plus size={14} /> 添加
          </button>
        </div>
      </SettingRow>

      {error && (
        <div className="settings-error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}
