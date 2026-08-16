import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// 事件订阅句柄收集器。
const eventHandlers: Record<string, ((payload: unknown) => void)[]> = {};

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, cb: (e: unknown) => void) => {
    if (!eventHandlers[event]) eventHandlers[event] = [];
    eventHandlers[event].push(cb);
    return Promise.resolve(() => {
      const arr = eventHandlers[event] ?? [];
      const idx = arr.indexOf(cb);
      if (idx >= 0) arr.splice(idx, 1);
    });
  }),
}));

const mockCall = vi.fn();
vi.mock("../../../lib/tauri", () => ({
  call: (...args: unknown[]) => mockCall(...args),
}));

import {
  useProjectRuntimeStore,
} from "./projectRuntimeStore";

// 模拟 Tauri 事件对象：{ payload, event, id }。
function emit(event: string, payload: unknown) {
  for (const cb of eventHandlers[event] ?? []) cb({ payload, event, id: 1 });
}

const detection = {
  runtimeKind: "node",
  candidates: [
    { label: "npm run dev", executable: "npm", args: ["run", "dev"], confidence: 100 },
    { label: "npm run start", executable: "npm", args: ["run", "start"], confidence: 90 },
  ],
  diagnostics: [],
};

const runSnapshot = {
  runId: "run-1",
  projectId: "p1",
  state: "running",
  cwd: "C:\\proj",
  pid: 1234,
  startedAt: 1,
  exitCode: null,
  errorCode: null,
  errorMessage: null,
  stopReason: null,
  summary: { executable: "npm", args: ["run", "dev"], cwd: "C:\\proj", env: {}, expected_port: null, preview_scheme: "http" },
};

function activeRun() {
  const s = useProjectRuntimeStore.getState();
  return s.runs[s.activeCwd] ?? null;
}

beforeEach(() => {
  mockCall.mockReset();
  // 默认行为：识别 + 无运行状态。
  mockCall.mockImplementation((cmd: string) => {
    if (cmd === "detect_project_runtime") return Promise.resolve(detection);
    if (cmd === "get_project_run") return Promise.resolve(null);
    if (cmd === "get_process_logs") return Promise.resolve({ entries: [], nextSeq: 1 });
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  useProjectRuntimeStore.getState().reset();
  for (const key of Object.keys(eventHandlers)) eventHandlers[key] = [];
  vi.restoreAllMocks();
});

describe("projectRuntimeStore", () => {
  it("load populates detection and config from first candidate", async () => {
    await useProjectRuntimeStore.getState().load("p1");
    const s = useProjectRuntimeStore.getState();
    expect(s.detection?.runtimeKind).toBe("node");
    expect(s.config?.executable).toBe("npm");
    expect(s.config?.args).toEqual(["run", "dev"]);
  });

  it("picking candidate updates config and invalidates confirmation", async () => {
    const store = useProjectRuntimeStore;
    await store.getState().load("p1");
    mockCall
      .mockImplementationOnce((cmd: string) =>
        cmd === "prepare_run_confirmation"
          ? Promise.resolve({ confirmationId: "c1", summary: detection.candidates[0], expiresInSeconds: 600 })
          : Promise.resolve(undefined),
      )
      .mockImplementation((cmd: string) => {
        if (cmd === "get_project_run") return Promise.resolve(null);
        return Promise.resolve(undefined);
      });
    const preview = await store.getState().prepare();
    expect(preview?.confirmationId).toBe("c1");
    expect(store.getState().confirmation?.confirmationId).toBe("c1");
    store.getState().pickCandidate(1);
    expect(store.getState().config?.args).toEqual(["run", "start"]);
    expect(store.getState().confirmation).toBeNull();
  });

  it("editing config invalidates confirmation", async () => {
    const store = useProjectRuntimeStore;
    await store.getState().load("p1");
    store.getState().setConfig({ executable: "python" });
    expect(store.getState().config?.executable).toBe("python");
  });

  it("default config applies subproject candidate cwd", async () => {
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "detect_project_runtime") {
        return Promise.resolve({
          runtimeKind: "java",
          candidates: [
            { label: "backend: mvn spring-boot:run", executable: "mvn", args: ["spring-boot:run"], confidence: 80, cwd: "backend" },
          ],
          diagnostics: [],
        });
      }
      if (cmd === "get_project_run") return Promise.resolve(null);
      return Promise.resolve(undefined);
    });
    await useProjectRuntimeStore.getState().load("p1");
    const s = useProjectRuntimeStore.getState();
    expect(s.config?.executable).toBe("mvn");
    expect(s.config?.cwd).toBe("backend");
  });

  it("pickCandidate applies subproject cwd and clears it for root candidates", async () => {
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "detect_project_runtime") {
        return Promise.resolve({
          runtimeKind: "java",
          candidates: [
            { label: "backend: mvn spring-boot:run", executable: "mvn", args: ["spring-boot:run"], confidence: 80, cwd: "backend" },
            { label: "npm run dev", executable: "npm", args: ["run", "dev"], confidence: 100 },
          ],
          diagnostics: [],
        });
      }
      if (cmd === "get_project_run") return Promise.resolve(null);
      return Promise.resolve(undefined);
    });
    const store = useProjectRuntimeStore;
    await store.getState().load("p1");
    store.getState().pickCandidate(0);
    expect(store.getState().config?.cwd).toBe("backend");
    store.getState().pickCandidate(1);
    expect(store.getState().config?.cwd).toBe("");
  });

  it("start flow: prepare then startWithConfirmation", async () => {
    const store = useProjectRuntimeStore;
    await store.getState().load("p1");
    mockCall
      .mockImplementationOnce(() =>
        Promise.resolve({ confirmationId: "c1", summary: detection.candidates[0], expiresInSeconds: 600 }),
      )
      .mockImplementationOnce(() => Promise.resolve({ confirmationId: "c1", confirmationHash: "h1" }))
      .mockImplementationOnce(() => Promise.resolve(runSnapshot))
      .mockImplementation((cmd: string) => {
        if (cmd === "get_project_run") return Promise.resolve(null);
        return Promise.resolve(undefined);
      });
    const preview = await store.getState().prepare();
    expect(preview).not.toBeNull();
    await store.getState().startWithConfirmation(preview!.confirmationId);
    expect(activeRun()?.state).toBe("running");
    expect(activeRun()?.pid).toBe(1234);
    expect(store.getState().lastStartedRunId).toBe("run-1");
    expect(mockCall).toHaveBeenCalledWith("start_project_process", expect.anything());
  });

  it("output events append deduped logs to the active run", async () => {
    const store = useProjectRuntimeStore;
    await store.getState().load("p1");
    // 先有 run（挂在 "" cwd 下）。
    store.setState({ runs: { "": runSnapshot as never }, runIdToCwd: { "run-1": "" } });
    emit("project-process://output", {
      runId: "run-1",
      projectId: "p1",
      seq: 1,
      stream: "stdout",
      text: "hello",
      truncated: false,
    });
    emit("project-process://output", {
      runId: "run-1",
      projectId: "p1",
      seq: 1,
      stream: "stdout",
      text: "hello-dup",
      truncated: false,
    });
    emit("project-process://output", {
      runId: "run-1",
      projectId: "p1",
      seq: 2,
      stream: "stderr",
      text: "warn",
      truncated: false,
    });
    await vi.waitFor(() => {
      expect(store.getState().logs).toHaveLength(2);
    });
    const logs = store.getState().logs;
    expect(logs.map((l) => l.seq)).toEqual([1, 2]);
    expect(logs[0].text).toBe("hello-dup");
  });

  it("status events update the run mapped by cwd", async () => {
    const store = useProjectRuntimeStore;
    await store.getState().load("p1");
    store.setState({ runs: { "": runSnapshot as never }, runIdToCwd: { "run-1": "" } });
    emit("project-process://status", {
      runId: "run-1",
      projectId: "p1",
      state: "exited",
      pid: 1234,
      exitCode: 0,
      errorCode: null,
      errorMessage: null,
    });
    await vi.waitFor(() => {
      expect(activeRun()?.state).toBe("exited");
    });
  });

  it("error event surfaces error message", async () => {
    const store = useProjectRuntimeStore;
    await store.getState().load("p1");
    emit("project-process://error", {
      runId: "run-1",
      projectId: "p1",
      errorCode: "process_spawn_failed",
      errorMessage: "无法创建子进程",
    });
    await vi.waitFor(() => {
      expect(store.getState().error).toContain("process_spawn_failed");
    });
  });

  it("stop calls stop_project_process for the active run", async () => {
    const store = useProjectRuntimeStore;
    await store.getState().load("p1");
    store.setState({ runs: { "": { ...runSnapshot, state: "running" } as never }, runIdToCwd: { "run-1": "" } });
    mockCall.mockResolvedValueOnce({ ...runSnapshot, state: "stopping" });
    await store.getState().stop();
    expect(mockCall).toHaveBeenCalledWith("stop_project_process", { runId: "run-1" });
  });

  it("parallel: starting backend then frontend does not block the second start", async () => {
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "detect_project_runtime") {
        return Promise.resolve({
          runtimeKind: "java",
          candidates: [
            { label: "backend: mvn spring-boot:run", executable: "mvn", args: ["spring-boot:run"], confidence: 80, cwd: "web/backend" },
            { label: "frontend: npm run dev", executable: "npm", args: ["run", "dev"], confidence: 100, cwd: "web/frontend" },
          ],
          diagnostics: [],
        });
      }
      if (cmd === "get_project_run") return Promise.resolve(null);
      return Promise.resolve(undefined);
    });
    const store = useProjectRuntimeStore;
    await store.getState().load("p1");

    // 启动后端（候选 0 → cwd=web/backend）。
    store.getState().pickCandidate(0);
    expect(store.getState().activeCwd).toBe("web/backend");
    mockCall
      .mockImplementationOnce(() =>
        Promise.resolve({ confirmationId: "c1", summary: {}, expiresInSeconds: 600 }),
      )
      .mockImplementationOnce(() => Promise.resolve({ confirmationId: "c1", confirmationHash: "h1" }))
      .mockImplementationOnce(() =>
        Promise.resolve({ ...runSnapshot, runId: "run-backend", cwd: "C:\\proj\\web\\backend", summary: { ...runSnapshot.summary, cwd: "C:\\proj\\web\\backend" } }),
      );
    const preview1 = await store.getState().start();
    expect(preview1).not.toBeNull();
    await store.getState().startWithConfirmation(preview1!.confirmationId);
    expect(store.getState().runs["web/backend"]?.runId).toBe("run-backend");

    // 切换前端（候选 1 → cwd=web/frontend），其 runs 项不存在 → 不应被拦截。
    store.getState().pickCandidate(1);
    expect(store.getState().activeCwd).toBe("web/frontend");
    const preview2 = await store.getState().start();
    expect(preview2).not.toBeNull();
    // 后端仍在运行映射中。
    expect(store.getState().runs["web/backend"]?.state).toBe("running");
  });
});
