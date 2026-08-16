import { create } from "zustand";
import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { LogEntry, RunSnapshot } from "../../projects/lib/projectRuntime";
import {
  getProcessLogs,
  restartProjectProcess,
  stopProjectProcess,
} from "../../projects/lib/projectRuntime";
import { openProjectPreview } from "../../projects/lib/projectPreview";
import { listProjectRuns, loadProjectNames } from "../lib/runCenter";

interface RunCenterState {
  runs: RunSnapshot[];
  projectNames: Record<string, string>;
  loading: boolean;
  error: string | null;
  /** 展开日志的 run_id。 */
  expandedRunId: string | null;
  logs: LogEntry[];

  load: () => Promise<void>;
  toggleLogs: (runId: string) => Promise<void>;
  stop: (runId: string) => Promise<void>;
  restart: (runId: string) => Promise<void>;
  openPreview: (runId: string) => Promise<void>;
  reset: () => void;
}

let unlistenRef: (() => void) | null = null;

/** 订阅项目进程事件，变化时刷新列表（运行中心与项目页共用后端事件，保持状态一致）。 */
export async function startRunCenterEvents(store: typeof useRunCenterStore) {
  const offs = await Promise.all([
    listen("project-process://status", () => void store.getState().load()),
    listen("project-process://exited", () => void store.getState().load()),
    listen("project-process://error", () => void store.getState().load()),
  ]);
  return () => offs.forEach((off) => off());
}

export const useRunCenterStore = create<RunCenterState>((set, get) => ({
  runs: [],
  projectNames: {},
  loading: false,
  error: null,
  expandedRunId: null,
  logs: [],

  load: async () => {
    set({ loading: true, error: null });
    try {
      const [runs, projectNames] = await Promise.all([
        listProjectRuns(true),
        loadProjectNames(),
      ]);
      set({ runs, projectNames, loading: false });
    } catch (e) {
      set({ loading: false, error: (e as Error).message });
    }
  },

  toggleLogs: async (runId) => {
    if (get().expandedRunId === runId) {
      set({ expandedRunId: null, logs: [] });
      return;
    }
    set({ expandedRunId: runId, logs: [], error: null });
    try {
      const page = await getProcessLogs(runId, 0);
      // 响应写入前校验当前展开项，避免慢请求覆盖新展开的日志。
      if (get().expandedRunId !== runId) return;
      set({ logs: page.entries });
    } catch (e) {
      if (get().expandedRunId !== runId) return;
      // 历史运行日志不跨会话持久化。
      set({ logs: [], error: (e as Error).message });
    }
  },

  stop: async (runId) => {
    set({ error: null });
    try {
      await stopProjectProcess(runId);
      await get().load();
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  restart: async (runId) => {
    set({ error: null });
    try {
      await restartProjectProcess(runId);
      await get().load();
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  openPreview: async (runId) => {
    set({ error: null });
    try {
      const target = await openProjectPreview(runId);
      if (target.ownership === "confirmed") {
        await openUrl(target.url);
      } else {
        set({
          error: `端口归属未确认（${target.url}）：该端口可能被其他进程占用，请确认后手动打开。`,
        });
      }
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  reset: () => {
    if (unlistenRef) {
      unlistenRef();
      unlistenRef = null;
    }
    set({ runs: [], projectNames: {}, error: null, expandedRunId: null, logs: [] });
  },
}));

/** 页面挂载时启动事件订阅；严格模式下重复挂载也能正确清理，不泄漏。 */
export function useRunCenterEvents(): void {
  useEffect(() => {
    let disposed = false;
    let off: (() => void) | null = null;
    void startRunCenterEvents(useRunCenterStore).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        off = unlisten;
      }
    });
    return () => {
      disposed = true;
      off?.();
    };
  }, []);
}
