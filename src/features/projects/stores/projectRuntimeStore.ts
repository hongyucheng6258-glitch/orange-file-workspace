import { create } from "zustand";
import {
  ConfirmationPreview,
  DetectionResult,
  LogEntry,
  RunConfig,
  RunSnapshot,
  deriveRunActions,
  detectProjectRuntime,
  getProcessLogs,
  getProjectRun,
  mergeLogs,
  prepareRunConfirmation,
  confirmRunConfig,
  restartProjectProcess,
  startProjectProcess,
  stopProjectProcess,
  subscribeProjectProcess,
} from "../lib/projectRuntime";

function defaultConfig(projectId: string, detection: DetectionResult | null): RunConfig {
  const first = detection?.candidates[0];
  return {
    projectId,
    executable: first?.executable ?? "",
    args: first?.args ?? [],
    cwd: "",
    envOverrides: {},
    expectedPort: null,
    previewScheme: "http",
  };
}

interface ProjectRuntimeState {
  projectId: string | null;
  detecting: boolean;
  detection: DetectionResult | null;
  config: RunConfig | null;
  /** 已确认的预览（配置变化后失效）。 */
  confirmation: ConfirmationPreview | null;
  run: RunSnapshot | null;
  logs: LogEntry[];
  busy: boolean;
  error: string | null;

  load: (projectId: string) => Promise<void>;
  reset: () => void;
  pickCandidate: (index: number) => void;
  setConfig: (patch: Partial<RunConfig>) => void;
  prepare: () => Promise<ConfirmationPreview | null>;
  startWithConfirmation: (confirmationId: string) => Promise<void>;
  start: () => Promise<ConfirmationPreview | null>;
  stop: () => Promise<void>;
  restart: () => Promise<void>;
  clearVisibleLogs: () => void;
}

let unlistenRef: (() => void) | null = null;

export const useProjectRuntimeStore = create<ProjectRuntimeState>((set, get) => ({
  projectId: null,
  detecting: false,
  detection: null,
  config: null,
  confirmation: null,
  run: null,
  logs: [],
  busy: false,
  error: null,

  load: async (projectId) => {
    get().reset();
    set({ projectId, detecting: true, error: null });
    try {
      const detection = await detectProjectRuntime(projectId);
      set({ detection, config: defaultConfig(projectId, detection), detecting: false });
    } catch (e) {
      set({ detecting: false, error: (e as Error).message });
    }
    // 读取既有运行状态与日志。
    try {
      const run = await getProjectRun(projectId);
      set({ run });
      if (run) {
        const page = await getProcessLogs(run.runId, 0);
        set({ logs: page.entries });
      }
    } catch {
      // 运行尚未创建过，忽略。
    }
    // 订阅事件（仅保留当前 runId 的事件）。
    if (unlistenRef) {
      unlistenRef();
      unlistenRef = null;
    }
    unlistenRef = await subscribeProjectProcess({
      onStatus: (p) => {
        const { projectId: current } = get();
        if (p.projectId !== current) return;
        const run = get().run;
        set({
          run: {
            runId: p.runId,
            projectId: p.projectId,
            state: p.state,
            cwd: run?.cwd ?? "",
            pid: p.pid,
            startedAt: run?.startedAt ?? null,
            exitCode: p.exitCode,
            errorCode: p.errorCode,
            errorMessage: p.errorMessage,
            stopReason: run?.stopReason ?? null,
            summary: run?.summary ?? {
              executable: "",
              args: [],
              cwd: "",
              env: {},
              expected_port: null,
              preview_scheme: "http",
            },
          },
        });
      },
      onOutput: (p) => {
        const { projectId: current, run } = get();
        if (p.projectId !== current) return;
        if (run && p.runId !== run.runId) return;
        set((state) => ({
          logs: mergeLogs(state.logs, [
            { seq: p.seq, stream: p.stream, text: p.text, truncated: p.truncated },
          ]),
        }));
      },
      onExited: (p) => {
        const { projectId: current, run } = get();
        if (p.projectId !== current) return;
        if (run && p.runId !== run.runId) return;
        set((state) => ({
          run: state.run
            ? {
                ...state.run,
                state: "exited",
                exitCode: p.exitCode,
                stopReason: p.stopReason,
              }
            : state.run,
        }));
      },
      onError: (p) => {
        const { projectId: current } = get();
        if (p.projectId !== current) return;
        set({ error: `${p.errorCode}: ${p.errorMessage}` });
      },
    });
  },

  reset: () => {
    if (unlistenRef) {
      unlistenRef();
      unlistenRef = null;
    }
    set({
      projectId: null,
      detection: null,
      config: null,
      confirmation: null,
      run: null,
      logs: [],
      error: null,
      busy: false,
    });
  },

  pickCandidate: (index) => {
    const { detection, projectId } = get();
    const cand = detection?.candidates[index];
    if (!cand || !projectId) return;
    set({
      config: {
        ...(get().config ?? defaultConfig(projectId, detection)),
        executable: cand.executable,
        args: cand.args,
      },
      confirmation: null,
    });
  },

  setConfig: (patch) => {
    const { config, projectId } = get();
    if (!config || !projectId) return;
    set({ config: { ...config, ...patch }, confirmation: null });
  },

  prepare: async () => {
    const { projectId, config } = get();
    if (!projectId || !config) return null;
    set({ busy: true, error: null });
    try {
      const preview = await prepareRunConfirmation(projectId, config);
      set({ confirmation: preview, busy: false });
      return preview;
    } catch (e) {
      set({ busy: false, error: (e as Error).message });
      return null;
    }
  },

  startWithConfirmation: async (confirmationId) => {
    const { projectId, config, confirmation } = get();
    if (!projectId || !config || !confirmation) return;
    set({ busy: true, error: null });
    try {
      const grant = await confirmRunConfig(confirmationId);
      const snap = await startProjectProcess(projectId, config, grant.confirmationHash);
      set({ run: snap, busy: false, error: null });
    } catch (e) {
      set({ busy: false, error: (e as Error).message });
    }
  },

  start: async () => {
    const { run, config, confirmation } = get();
    if (!config) return null;
    if (run && (run.state === "starting" || run.state === "running" || run.state === "stopping")) {
      set({ error: "项目已有运行实例，请先停止" });
      return null;
    }
    // 已确认且配置未变化 → 直接启动；否则弹出确认对话框。
    if (confirmation) {
      const { projectId } = get();
      if (!projectId) return null;
      set({ busy: true, error: null });
      try {
        const grant = await confirmRunConfig(confirmation.confirmationId);
        const snap = await startProjectProcess(projectId, config, grant.confirmationHash);
        set({ run: snap, busy: false, error: null });
        return null;
      } catch (e) {
        set({ busy: false, error: (e as Error).message, confirmation: null });
        return null;
      }
    }
    return get().prepare();
  },

  stop: async () => {
    const { run } = get();
    if (!run || run.state !== "running") return;
    set({ busy: true, error: null });
    try {
      const snap = await stopProjectProcess(run.runId);
      set({ run: snap, busy: false });
    } catch (e) {
      set({ busy: false, error: (e as Error).message });
    }
  },

  restart: async () => {
    const { run } = get();
    if (!run) return;
    set({ busy: true, error: null });
    try {
      const snap = await restartProjectProcess(run.runId);
      set({ run: snap, busy: false, logs: [] });
    } catch (e) {
      set({ busy: false, error: (e as Error).message });
    }
  },

  clearVisibleLogs: () => set({ logs: [] }),
}));

/** 供组件使用的派生状态。 */
export function useRunActionFlags() {
  const run = useProjectRuntimeStore((s) => s.run);
  return deriveRunActions(run);
}
