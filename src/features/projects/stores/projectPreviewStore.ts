import { create } from "zustand";
import { openUrl } from "@tauri-apps/plugin-opener";
import { PreviewTarget, openProjectPreview } from "../lib/projectPreview";
import { useProjectRuntimeStore } from "./projectRuntimeStore";

interface ProjectPreviewState {
  target: PreviewTarget | null;
  checking: boolean;
  error: string | null;

  /** 解析并校验预览目标；归属确认时自动打开浏览器。 */
  openPreview: () => Promise<void>;
  /** 在浏览器中打开当前预览目标（归属未确认时由用户手动触发）。 */
  openInBrowser: () => Promise<void>;
  reset: () => void;
}

export const useProjectPreviewStore = create<ProjectPreviewState>((set, get) => ({
  target: null,
  checking: false,
  error: null,

  openPreview: async () => {
    const run = useProjectRuntimeStore.getState().run;
    if (!run || run.state !== "running") {
      set({ error: "项目未在运行，无法打开预览" });
      return;
    }
    set({ checking: true, error: null });
    try {
      const target = await openProjectPreview(run.runId);
      set({ target, checking: false });
      if (target.ownership === "confirmed") {
        // 归属已确认：直接打开系统浏览器。
        try {
          await openUrl(target.url);
        } catch {
          // 打开失败仅提示，不阻断预览目标展示。
          set({ error: "已在浏览器外展示预览地址，可手动打开" });
        }
      }
    } catch (e) {
      set({ checking: false, error: (e as Error).message });
    }
  },

  openInBrowser: async () => {
    const target = get().target;
    if (!target) return;
    set({ error: null });
    try {
      await openUrl(target.url);
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  reset: () => set({ target: null, checking: false, error: null }),
}));
