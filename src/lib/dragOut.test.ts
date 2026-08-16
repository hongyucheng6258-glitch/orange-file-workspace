import { afterEach, describe, expect, it, vi } from "vitest";
import { startDragOut } from "./dragOut";
import { call } from "./tauri";

vi.mock("./tauri", () => ({
  call: vi.fn(),
}));

const mockedCall = vi.mocked(call);

afterEach(() => {
  vi.clearAllMocks();
  delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe("startDragOut", () => {
  it("immediately announces the drag request before native drag completes", () => {
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    mockedCall.mockReturnValue(new Promise<number>(() => {}));
    const listener = vi.fn();
    window.addEventListener("nexus:drag-status", listener);

    startDragOut(["resource-1"]);

    expect(listener).toHaveBeenCalledOnce();
    expect((listener.mock.calls[0][0] as CustomEvent).detail).toEqual({
      phase: "starting",
      message: "正在启动系统拖拽…",
    });
    window.removeEventListener("nexus:drag-status", listener);
  });

  it("announces native drag failures", async () => {
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    mockedCall.mockRejectedValue(new Error("OLE failed"));
    const listener = vi.fn();
    window.addEventListener("nexus:drag-status", listener);

    startDragOut(["resource-1"]);
    await Promise.resolve();
    await Promise.resolve();

    const lastCall = listener.mock.calls[listener.mock.calls.length - 1];
    const detail = (lastCall[0] as CustomEvent).detail;
    expect(detail).toEqual({ phase: "error", message: "OLE failed" });
    window.removeEventListener("nexus:drag-status", listener);
  });
});
