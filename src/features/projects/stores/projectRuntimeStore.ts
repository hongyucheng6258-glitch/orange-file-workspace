import { create } from "zustand";
import {
  ConfirmationPreview,
  DetectionResult,
  LogEntry,
  RunConfig,
  RunSnapshot,
  detectProjectRuntime,
  getProcessLogs,
  listProjectRunsByProject,
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
  /** 各候选 cwd 的操作繁忙状态（互不影响并行实例）。 */
  busyByCwd: Record<string, boolean>;
  error: string | null;
  /** 最近一次成功启动的 runId（UI 据此触发自动预览）。 */
  lastStartedRunId: string | null;

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

/** 全局事件订阅句柄；load 重新进入时先解绑旧订阅。 */
let unlistenRef: (() => void) | null = null;
/** 项目加载代际：递增后旧请求的异步结果不再写入状态，防止快速切换串线。 */
let loadGeneration = 0;

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
  busyByCwd: {},
  error: null,
  lastStartedRunId: null,

  load: async (projectId) => {
    const generation = ++loadGeneration;
    get().reset();
    set({ projectId, detecting: true, error: null });
    // 检测运行候选（可能较慢；结果写入前校验代际）。
    try {
      const detection = await detectProjectRuntime(projectId);
      if (generation !== loadGeneration) return;
      set({ detection, config: defaultConfig(projectId, detection), detecting: false });
    } catch (e) {
      if (generation !== loadGeneration) return;
      set({ detecting: false, error: (e as Error).message });
    }
    // 恢复该项目的全部运行实例（活动 + 各 cwd 最近终态）。
    try {
      const entries = await listProjectRunsByProject(projectId);
      if (generation !== loadGeneration) return;
      const runs: Record<string, RunSnapshot> = {};
      const runIdToCwd: Record<string, string> = {};
      for (const entry of entries) {
        const cwd = entry.cwdRel ?? "";
        runs[cwd] = entry.snapshot;
        runIdToCwd[entry.runId] = cwd;
      }
      if (generation !== loadGeneration) return;
      const activeCwd = entries[0]?.cwdRel ?? get().activeCwd;
      set({ runs, runIdToCwd, activeCwd });
    } catch {
      // 运行尚未创建过，忽略。
    }
    // 订阅事件（仅保留当前代际的事件）。先订阅再查日志，
    // 避免查询与订阅之间的日志落入盲区。
    if (unlistenRef) {
      unlistenRef();
      unlistenRef = null;
    }
    unlistenRef = await subscribeProjectProcess({
      onStatus: (p) => {
        if (generation !== loadGeneration) return;
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
        if (generation !== loadGeneration) return;
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
        if (generation !== loadGeneration) return;
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
        if (generation !== loadGeneration) return;
        const { projectId: current } = get();
        if (p.projectId !== current) return;
        set({ error: `${p.errorCode}: ${p.errorMessage}` });
      },
    });
    // 订阅建立后补查各实例日志；期间到达的 output 事件已通过合并去重。
    if (generation !== loadGeneration) return;
    const logsByCwd: Record<string, LogEntry[]> = {};
    for (const [runId, cwd] of Object.entries(get().runIdToCwd)) {
      try {
        const page = await getProcessLogs(runId, 0);
        if (generation !== loadGeneration) return;
        const merged = mergeLogs(get().logsByCwd[cwd] ?? [], page.entries);
        logsByCwd[cwd] = merged;
      } catch {
        logsByCwd[cwd] = get().logsByCwd[cwd] ?? [];
      }
    }
    if (generation !== loadGeneration) return;
    set({
      logsByCwd,
      logs: logsByCwd[get().activeCwd] ?? [],
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
      busyByCwd: {},
      error: null,
      lastStartedRunId: null,
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
    const cwd = config.cwd;
    set({ busyByCwd: { ...get().busyByCwd, [cwd]: true }, error: null });
    try {
      const preview = await prepareRunConfirmation(projectId, config);
      set({ confirmation: preview, busyByCwd: { ...get().busyByCwd, [cwd]: false } });
      return preview;
    } catch (e) {
      set({ busyByCwd: { ...get().busyByCwd, [cwd]: false }, error: (e as Error).message });
      return null;
    }
  },

  startWithConfirmation: async (confirmationId) => {
    const { projectId, config } = get();
    if (!projectId || !config) return;
    const cwd = config.cwd;
    set({ busyByCwd: { ...get().busyByCwd, [cwd]: true }, error: null });
    try {
      const grant = await confirmRunConfig(confirmationId);
      const snap = await startProjectProcess(projectId, config, grant.confirmationHash);
      const runIdToCwd = { ...get().runIdToCwd, [snap.runId]: cwd };
      set({
        runs: { ...get().runs, [cwd]: snap },
        runIdToCwd,
        activeCwd: cwd,
        busyByCwd: { ...get().busyByCwd, [cwd]: false },
        error: null,
        lastStartedRunId: snap.runId,
        // 授权已消费：清除确认票据，下次启动必须重新确认。
        confirmation: null,
      });
    } catch (e) {
      set({ busyByCwd: { ...get().busyByCwd, [cwd]: false }, error: (e as Error).message });
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
      set({ busyByCwd: { ...get().busyByCwd, [cwd]: true }, error: null });
      try {
        const grant = await confirmRunConfig(confirmation.confirmationId);
        const snap = await startProjectProcess(projectId, config, grant.confirmationHash);
        const runIdToCwd = { ...get().runIdToCwd, [snap.runId]: cwd };
        set({
          runs: { ...get().runs, [cwd]: snap },
          runIdToCwd,
          activeCwd: cwd,
          busyByCwd: { ...get().busyByCwd, [cwd]: false },
          error: null,
          lastStartedRunId: snap.runId,
          confirmation: null,
        });
        return null;
      } catch (e) {
        set({
          busyByCwd: { ...get().busyByCwd, [cwd]: false },
          error: (e as Error).message,
          confirmation: null,
        });
        return null;
      }
    }
    return get().prepare();
  },

  stop: async () => {
    const { activeCwd, runs } = get();
    const active = runs[activeCwd];
    if (!active || active.state !== "running") return;
    set({ busyByCwd: { ...get().busyByCwd, [activeCwd]: true }, error: null });
    try {
      const snap = await stopProjectProcess(active.runId);
      // 基于最新状态合并，避免覆盖等待期间到达的事件。
      set({
        runs: { ...get().runs, [activeCwd]: snap },
        busyByCwd: { ...get().busyByCwd, [activeCwd]: false },
      });
    } catch (e) {
      set({ busyByCwd: { ...get().busyByCwd, [activeCwd]: false }, error: (e as Error).message });
    }
  },

  restart: async () => {
    const { activeCwd, runs, runIdToCwd } = get();
    const active = runs[activeCwd];
    if (!active) return;
    set({ busyByCwd: { ...get().busyByCwd, [activeCwd]: true }, error: null });
    try {
      const snap = await restartProjectProcess(active.runId);
      // 重启生成新 runId：必须同步更新反查表，否则新进程事件会被丢弃。
      const nextRunIdToCwd: Record<string, string> = {};
      for (const [rid, cwd] of Object.entries(runIdToCwd)) {
        if (cwd !== activeCwd) nextRunIdToCwd[rid] = cwd;
      }
      nextRunIdToCwd[snap.runId] = activeCwd;
      set({
        runs: { ...get().runs, [activeCwd]: snap },
        runIdToCwd: nextRunIdToCwd,
        logsByCwd: { ...get().logsByCwd, [activeCwd]: [] },
        logs: [],
        busyByCwd: { ...get().busyByCwd, [activeCwd]: false },
        lastStartedRunId: snap.runId,
      });
    } catch (e) {
      set({ busyByCwd: { ...get().busyByCwd, [activeCwd]: false }, error: (e as Error).message });
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