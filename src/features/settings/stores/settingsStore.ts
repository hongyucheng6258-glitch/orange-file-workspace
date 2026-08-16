import { create } from "zustand";
import { call } from "../../../lib/tauri";

export interface IgnoreRule {
  kind: "name" | "path";
  pattern: string;
  enabled: boolean;
}

export interface AppSettings {
  general: {
    launch_behavior: "home" | "files";
    default_import_mode: "managed" | "external";
    duplicate_policy: "skip" | "keep_both";
    preview_size_limit_mb: number;
  };
  appearance: {
    theme_mode: "system" | "light" | "dark";
    density: "comfortable" | "compact";
  };
  terminal: {
    font_size: number;
    cursor_style: "block" | "bar" | "underline";
    theme:
      | "campbell"
      | "vs_dark"
      | "one_dark"
      | "dracula"
      | "solarized_dark";
  };
  ignore: { custom_rules: IgnoreRule[] };
  backup: {
    enabled: boolean;
    frequency: "daily" | "weekly";
    run_time: string;
    retention_count: number;
    backup_type: "metadata" | "full";
  };
  storage: { data_dir: string; managed_dir: string };
}

export interface SaveState {
  saving: boolean;
  error: string | null;
  okAt: number | null;
}

interface SettingsState {
  settings: AppSettings | null;
  loading: boolean;
  saveState: SaveState;
  load: () => Promise<void>;
  update: (key: string, value: unknown) => Promise<void>;
  resetCategory: (category: string) => Promise<void>;
  clearError: () => void;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: null,
  loading: false,
  saveState: { saving: false, error: null, okAt: null },

  load: async () => {
    set({ loading: true });
    try {
      const settings = await call<AppSettings>("get_settings", {});
      set({ settings, loading: false });
    } catch (e) {
      set({
        loading: false,
        saveState: { saving: false, error: (e as Error).message, okAt: null },
      });
    }
  },

  update: async (key, value) => {
    const prev = get().settings;
    if (!prev) return;
    // 乐观更新
    const next = applyPatch(prev, key, value);
    set({ settings: next, saveState: { saving: true, error: null, okAt: null } });
    try {
      const settings = await call<AppSettings>("update_setting", { key, value });
      set({ settings, saveState: { saving: false, error: null, okAt: Date.now() } });
    } catch (e) {
      // 失败回滚
      set({
        settings: prev,
        saveState: { saving: false, error: (e as Error).message, okAt: null },
      });
    }
  },

  resetCategory: async (category) => {
    const prev = get().settings;
    set({ saveState: { saving: true, error: null, okAt: null } });
    try {
      const settings = await call<AppSettings>("reset_settings_category", { category });
      set({ settings, saveState: { saving: false, error: null, okAt: Date.now() } });
    } catch (e) {
      set({
        settings: prev,
        saveState: { saving: false, error: (e as Error).message, okAt: null },
      });
    }
  },

  clearError: () => set((s) => ({ saveState: { ...s.saveState, error: null } })),
}));

/** 按 "分类.字段" 键路径打补丁，返回新对象（不修改原对象）。 */
export function applyPatch(prev: AppSettings, key: string, value: unknown): AppSettings {
  const [section, field] = key.split(".") as [keyof AppSettings, string];
  const target = prev[section] as Record<string, unknown>;
  return { ...prev, [section]: { ...target, [field]: value } };
}
