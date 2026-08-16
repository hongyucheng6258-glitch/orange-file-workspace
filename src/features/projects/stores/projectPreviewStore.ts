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
  /** 启动后自动等待端口就绪并打开页面；失败时静默重试至超时。 */
  autoOpenPreview: (runId: string, timeoutMs?: number) => void;
  /** 在浏览器中打开当前预览目标（归属未确认时由用户手动触发）。 */
  openInBrowser: () => Promise<void>;
  reset: () => void;
}

/** 自动预览轮询句柄。 */
let autoTimer: ReturnType<typeof setTimeout> | null = null;
/** 当前正在轮询的 runId（避免重复轮询）。 */
let autoRunId: string | null = null;

export const useProjectPreviewStore = create<ProjectPreviewState>((set, get) => ({
  target: null,
  checking: false,
  error: null,

  openPreview: async () => {
    const s = useProjectRuntimeStore.getState();
    const run = s.runs[s.activeCwd];
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

  autoOpenPreview: (runId, timeoutMs = 30000) => {
    // 已在轮询同一运行，避免重复。
    if (autoRunId === runId) return;
    if (autoTimer) {
      clearTimeout(autoTimer);
      autoTimer = null;
    }
    autoRunId = runId;
    const intervalMs = 2000;
    const maxAttempts = Math.max(1, Math.floor(timeoutMs / intervalMs));
    let attempts = 0;

    const attempt = async () => {
      if (autoRunId !== runId) return; // 已被 reset 或切换到新运行。
      attempts += 1;
      try {
        const target = await openProjectPreview(runId);
        autoRunId = null;
        set({ target, checking: false, error: null });
        if (target.ownership === "confirmed") {
          try {
            await openUrl(target.url);
          } catch {
            set({ error: "已在浏览器外展示预览地址，可手动打开" });
          }
        }
      } catch {
        if (attempts >= maxAttempts) {
          // 超时静默放弃，不打扰用户；面板上仍可手动点预览。
          autoRunId = null;
          return;
        }
        autoTimer = setTimeout(attempt, intervalMs);
      }
    };
    // 进程启动到端口监听通常需要数秒，先等待再开始探测。
    autoTimer = setTimeout(attempt, 1500);
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

  reset: () => {
    if (autoTimer) {
      clearTimeout(autoTimer);
      autoTimer = null;
    }
    autoRunId = null;
    set({ target: null, checking: false, error: null });
  },
}));
