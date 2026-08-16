import { create } from "zustand";
import {
  ConfirmationPreview,
  DetectionResult,
  LogEntry,
  RunConfig,
  RunSnapshot,
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
    cwd: first?.cwd ?? "",
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
  /** 当前选中候选的工作目录（相对项目根；空串 = 根）。 */
  activeCwd: string;
  /** 已确认的预览（配置变化后失效）。 */
  confirmation: ConfirmationPreview | null;
  /** 各子项目运行实例：key = 候选 cwd。 */
  runs: Record<string, RunSnapshot>;
  /** runId → 候选 cwd 反查表，用于把事件归属到对应运行。 */
  runIdToCwd: Record<string, string>;
  /** 当前选中运行的日志。 */
  logs: LogEntry[];
  /** 各候选 cwd 的日志缓冲。 */
  logsByCwd: Record<string, LogEntry[]>;
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
  activeCwd: "",
  confirmation: null,
  runs: {},
  runIdToCwd: {},
  logs: [],
  logsByCwd: {},
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
    // 读取既有运行状态与日志（最近一条，用于初始化视图）。
    try {
      const run = await getProjectRun(projectId);
      const activeCwd = get().activeCwd;
      if (run) {
        const runs = { ...get().runs, [activeCwd]: run };
        const runIdToCwd = { ...get().runIdToCwd, [run.runId]: activeCwd };
        set({ runs, runIdToCwd });
        const page = await getProcessLogs(run.runId, 0);
        set({
          logs: page.entries,
          logsByCwd: { ...get().logsByCwd, [activeCwd]: page.entries },
        });
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
        const key = get().runIdToCwd[p.runId];
        if (key == null) return;
        // 将状态合并进对应运行实例。
        const prev = get().runs[key];
        const next: RunSnapshot = {
          runId: p.runId,
          projectId: p.projectId,
          state: p.state,
          cwd: prev?.cwd ?? "",
          pid: p.pid,
          startedAt: prev?.startedAt ?? null,
          exitCode: p.exitCode,
          errorCode: p.errorCode,
          errorMessage: p.errorMessage,
          stopReason: prev?.stopReason ?? null,
          summary: prev?.summary ?? {
            executable: "",
            args: [],
            cwd: "",
            env: {},
            expected_port: null,
            preview_scheme: "http",
          },
        };
        set({ runs: { ...get().runs, [key]: next } });
      },
      onOutput: (p) => {
        const { projectId: current, runIdToCwd } = get();
        if (p.projectId !== current) return;
        const key = runIdToCwd[p.runId];
        if (key == null) return;
        const merged = mergeLogs(get().logsByCwd[key] ?? [], [
          { seq: p.seq, stream: p.stream, text: p.text, truncated: p.truncated },
        ]);
        set({
          logsByCwd: { ...get().logsByCwd, [key]: merged },
          ...(get().activeCwd === key ? { logs: merged } : {}),
        });
      },
      onExited: (p) => {
        const { projectId: current, runIdToCwd } = get();
        if (p.projectId !== current) return;
        const key = runIdToCwd[p.runId];
        if (key == null) return;
        const prev = get().runs[key];
        if (!prev) return;
        const next = { ...prev, state: "exited" as const, exitCode: p.exitCode, stopReason: p.stopReason };
        set({ runs: { ...get().runs, [key]: next } });
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
      activeCwd: "",
      confirmation: null,
      runs: {},
      runIdToCwd: {},
      logs: [],
      logsByCwd: {},
      error: null,
      busy: false,
    });
  },

  pickCandidate: (index) => {
    const { detection, projectId } = get();
    const cand = detection?.candidates[index];
    if (!cand || !projectId) return;
    const cwd = cand.cwd ?? "";
    set({
      config: {
        ...(get().config ?? defaultConfig(projectId, detection)),
        executable: cand.executable,
        args: cand.args,
        cwd,
      },
      activeCwd: cwd,
      confirmation: null,
      logs: get().logsByCwd[cwd] ?? [],
    });
  },

  setConfig: (patch) => {
    const { config, projectId } = get();
    if (!config || !projectId) return;
    // cwd 变化时同步切换活动运行与日志视图。
    const nextConfig = { ...config, ...patch };
    const nextCwd = nextConfig.cwd;
    set({
      config: nextConfig,
      activeCwd: nextCwd,
      confirmation: null,
      logs: nextCwd !== config.cwd ? get().logsByCwd[nextCwd] ?? [] : get().logs,
    });
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
    const { projectId, config } = get();
    if (!projectId || !config) return;
    set({ busy: true, error: null });
    try {
      const grant = await confirmRunConfig(confirmationId);
      const snap = await startProjectProcess(projectId, config, grant.confirmationHash);
      const cwd = config.cwd;
      set({
        runs: { ...get().runs, [cwd]: snap },
        runIdToCwd: { ...get().runIdToCwd, [snap.runId]: cwd },
        activeCwd: cwd,
        busy: false,
        error: null,
      });
    } catch (e) {
      set({ busy: false, error: (e as Error).message });
    }
  },

  start: async () => {
    const { config, confirmation } = get();
    if (!config) return null;
    const cwd = config.cwd;
    const active = get().runs[cwd];
    if (active && (active.state === "starting" || active.state === "running" || active.state === "stopping")) {
      set({ error: "该运行已有实例，请先停止" });
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
        set({
          runs: { ...get().runs, [cwd]: snap },
          runIdToCwd: { ...get().runIdToCwd, [snap.runId]: cwd },
          activeCwd: cwd,
          busy: false,
          error: null,
        });
        return null;
      } catch (e) {
        set({ busy: false, error: (e as Error).message, confirmation: null });
        return null;
      }
    }
    return get().prepare();
  },

  stop: async () => {
    const { activeCwd, runs } = get();
    const active = runs[activeCwd];
    if (!active || active.state !== "running") return;
    set({ busy: true, error: null });
    try {
      const snap = await stopProjectProcess(active.runId);
      set({
        runs: { ...runs, [activeCwd]: snap },
        busy: false,
      });
    } catch (e) {
      set({ busy: false, error: (e as Error).message });
    }
  },

  restart: async () => {
    const { activeCwd, runs } = get();
    const active = runs[activeCwd];
    if (!active) return;
    set({ busy: true, error: null });
    try {
      const snap = await restartProjectProcess(active.runId);
      set({
        runs: { ...runs, [activeCwd]: snap },
        logsByCwd: { ...get().logsByCwd, [activeCwd]: [] },
        logs: [],
        busy: false,
      });
    } catch (e) {
      set({ busy: false, error: (e as Error).message });
    }
  },

  clearVisibleLogs: () => {
    const { activeCwd } = get();
    set({
      logs: [],
      logsByCwd: { ...get().logsByCwd, [activeCwd]: [] },
    });
  },
}));