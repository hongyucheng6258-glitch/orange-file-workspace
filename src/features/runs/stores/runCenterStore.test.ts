import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockCall = vi.fn();
vi.mock("../../../lib/tauri", () => ({
  call: (...args: unknown[]) => mockCall(...args),
}));

const mockOpenUrl = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => mockOpenUrl(...args),
}));

import { useRunCenterStore } from "./runCenterStore";

const running = {
  runId: "r1",
  projectId: "p1",
  state: "running",
  cwd: "C:\\proj",
  pid: 123,
  startedAt: 1700000000,
  exitCode: null,
  errorCode: null,
  errorMessage: null,
  stopReason: null,
  summary: {
    executable: "npm",
    args: ["run", "dev"],
    cwd: "C:\\proj",
    env: {},
    expected_port: 3000,
    preview_scheme: "http",
  },
};

const exited = {
  ...running,
  runId: "r2",
  state: "exited",
  pid: null,
  exitCode: 0,
  startedAt: 1699999000,
};

beforeEach(() => {
  mockCall.mockReset();
  mockOpenUrl.mockReset();
  mockCall.mockImplementation((cmd: string) => {
    if (cmd === "list_project_runs") return Promise.resolve([running, exited]);
    if (cmd === "list_projects") return Promise.resolve([{ id: "p1", name: "我的项目" }]);
    if (cmd === "get_process_logs") {
      return Promise.resolve({ entries: [{ seq: 1, stream: "stdout", text: "hi", truncated: false }], nextSeq: 2 });
    }
    if (cmd === "stop_project_process" || cmd === "restart_project_process") {
      return Promise.resolve({ ...running, state: "stopping" });
    }
    if (cmd === "open_project_preview") {
      return Promise.resolve({
        runId: "r1",
        projectId: "p1",
        url: "http://127.0.0.1:3000",
        scheme: "http",
        host: "127.0.0.1",
        port: 3000,
        path: "",
        source: "config",
        ownership: "confirmed",
      });
    }
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  useRunCenterStore.getState().reset();
});

describe("runCenterStore", () => {
  it("load populates runs and project names", async () => {
    await useRunCenterStore.getState().load();
    const s = useRunCenterStore.getState();
    expect(s.runs).toHaveLength(2);
    expect(s.projectNames.p1).toBe("我的项目");
    expect(mockCall).toHaveBeenCalledWith("list_project_runs", { includeExited: true });
  });

  it("load surfaces error", async () => {
    mockCall.mockRejectedValueOnce(new Error("db locked"));
    await useRunCenterStore.getState().load();
    expect(useRunCenterStore.getState().error).toContain("db locked");
  });

  it("toggleLogs loads entries and collapses on second call", async () => {
    await useRunCenterStore.getState().load();
    await useRunCenterStore.getState().toggleLogs("r1");
    expect(useRunCenterStore.getState().logs[0].text).toBe("hi");
    expect(useRunCenterStore.getState().expandedRunId).toBe("r1");
    await useRunCenterStore.getState().toggleLogs("r1");
    expect(useRunCenterStore.getState().expandedRunId).toBeNull();
  });

  it("stop calls stop_project_process and reloads", async () => {
    await useRunCenterStore.getState().load();
    await useRunCenterStore.getState().stop("r1");
    expect(mockCall).toHaveBeenCalledWith("stop_project_process", { runId: "r1" });
    // 重新加载列表。
    expect(mockCall).toHaveBeenCalledWith("list_project_runs", { includeExited: true });
  });

  it("openPreview opens browser when confirmed", async () => {
    await useRunCenterStore.getState().load();
    await useRunCenterStore.getState().openPreview("r1");
    expect(mockOpenUrl).toHaveBeenCalledWith("http://127.0.0.1:3000");
    expect(useRunCenterStore.getState().error).toBeNull();
  });

  it("openPreview warns when ownership unconfirmed", async () => {
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "open_project_preview") {
        return Promise.resolve({
          runId: "r1",
          projectId: "p1",
          url: "http://127.0.0.1:3000",
          scheme: "http",
          host: "127.0.0.1",
          port: 3000,
          path: "",
          source: "config",
          ownership: "unconfirmed",
        });
      }
      return Promise.resolve(undefined);
    });
    await useRunCenterStore.getState().load();
    await useRunCenterStore.getState().openPreview("r1");
    expect(mockOpenUrl).not.toHaveBeenCalled();
    expect(useRunCenterStore.getState().error).toContain("归属未确认");
  });
});
