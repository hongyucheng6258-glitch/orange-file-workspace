import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockCall = vi.fn();
vi.mock("../../../lib/tauri", () => ({
  call: (...args: unknown[]) => mockCall(...args),
}));

const mockOpenUrl = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => mockOpenUrl(...args),
}));

import {
  PreviewTarget,
} from "../lib/projectPreview";
import {
  useProjectPreviewStore,
} from "./projectPreviewStore";
import { useProjectRuntimeStore } from "./projectRuntimeStore";

const target: PreviewTarget = {
  runId: "run-1",
  projectId: "p1",
  url: "http://127.0.0.1:3000",
  scheme: "http",
  host: "127.0.0.1",
  port: 3000,
  path: "",
  source: "config",
  ownership: "confirmed",
};

const runningSnapshot = {
  runId: "run-1",
  projectId: "p1",
  state: "running" as const,
  cwd: "C:\\proj",
  pid: 1234,
  startedAt: 1,
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

beforeEach(() => {
  mockCall.mockReset();
  mockOpenUrl.mockReset();
});

afterEach(() => {
  useProjectPreviewStore.getState().reset();
  useProjectRuntimeStore.setState({ runs: {}, runIdToCwd: {}, activeCwd: "" });
});

/** 把运行快照挂到 "" cwd 下（等效于旧版 setState({ run })）。 */
function setActiveRun(snap: unknown) {
  useProjectRuntimeStore.setState({
    activeCwd: "",
    runs: { "": snap as never },
    runIdToCwd: { [(snap as { runId: string }).runId]: "" },
  });
}

describe("projectPreviewStore", () => {
  it("openPreview opens browser when ownership confirmed", async () => {
    mockCall.mockResolvedValueOnce(target);
    mockOpenUrl.mockResolvedValueOnce(undefined);
    setActiveRun(runningSnapshot);

    await useProjectPreviewStore.getState().openPreview();

    expect(mockCall).toHaveBeenCalledWith("open_project_preview", { runId: "run-1" });
    expect(mockOpenUrl).toHaveBeenCalledWith("http://127.0.0.1:3000");
    expect(useProjectPreviewStore.getState().target?.ownership).toBe("confirmed");
    expect(useProjectPreviewStore.getState().error).toBeNull();
  });

  it("openPreview does not auto-open when ownership unconfirmed", async () => {
    const unconfirmed = { ...target, ownership: "unconfirmed" as const };
    mockCall.mockResolvedValueOnce(unconfirmed);

    setActiveRun(runningSnapshot);
    await useProjectPreviewStore.getState().openPreview();

    expect(mockOpenUrl).not.toHaveBeenCalled();
    expect(useProjectPreviewStore.getState().target?.ownership).toBe("unconfirmed");
  });

  it("openPreview surfaces preview_unavailable error", async () => {
    mockCall.mockRejectedValueOnce(new Error("端口 3000 当前未监听"));
    setActiveRun(runningSnapshot);

    await useProjectPreviewStore.getState().openPreview();

    expect(useProjectPreviewStore.getState().error).toContain("未监听");
    expect(mockOpenUrl).not.toHaveBeenCalled();
  });

  it("openPreview rejects when project not running", async () => {
    setActiveRun({ ...runningSnapshot, state: "exited" });

    await useProjectPreviewStore.getState().openPreview();

    expect(mockCall).not.toHaveBeenCalled();
    expect(useProjectPreviewStore.getState().error).toContain("未在运行");
  });

  it("openInBrowser opens current target manually", async () => {
    useProjectPreviewStore.setState({ target: { ...target, ownership: "unconfirmed" } });
    mockOpenUrl.mockResolvedValueOnce(undefined);

    await useProjectPreviewStore.getState().openInBrowser();

    expect(mockOpenUrl).toHaveBeenCalledWith("http://127.0.0.1:3000");
  });

  it("reset clears target and error", () => {
    useProjectPreviewStore.setState({ target, error: "x" });
    useProjectPreviewStore.getState().reset();
    expect(useProjectPreviewStore.getState().target).toBeNull();
    expect(useProjectPreviewStore.getState().error).toBeNull();
  });
});
