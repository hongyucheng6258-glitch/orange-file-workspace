import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockCall = vi.fn();
vi.mock("../../../lib/tauri", () => ({
  call: (...args: unknown[]) => mockCall(...args),
}));

import { useEditorStore } from "./editorStore";

const fileData = {
  resource: { id: "f1", kind: "file", name: "a.txt", created_at: 1, updated_at: 1 },
  content: "v1",
  session: {},
  path: "C:\\a.txt",
};

beforeEach(() => {
  mockCall.mockReset();
  mockCall.mockImplementation((cmd: string) => {
    if (cmd === "open_file") return Promise.resolve(fileData);
    if (cmd === "save_file") return Promise.resolve({ status: "saved" });
    if (cmd === "save_file_force") return Promise.resolve({ status: "saved" });
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  useEditorStore.setState({
    openFile: null,
    content: "",
    dirty: false,
    saving: false,
    conflict: null,
    openError: null,
    pendingOpenId: null,
    pendingClose: false,
  });
});

describe("editorStore", () => {
  it("open loads content and clears error", async () => {
    await useEditorStore.getState().open("f1");
    const s = useEditorStore.getState();
    expect(s.openFile?.content).toBe("v1");
    expect(s.dirty).toBe(false);
  });

  it("open sets openError when file too large", async () => {
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "open_file")
        return Promise.reject({ code: "file_too_large", message: "文件过大" });
      return Promise.resolve(undefined);
    });
    await useEditorStore.getState().open("f1");
    expect(useEditorStore.getState().openError).toBe("文件过大");
    expect(useEditorStore.getState().openFile).toBeNull();
  });
});

describe("editorStore dirty guard", () => {
  it("open returns confirm and keeps current file when dirty", async () => {
    await useEditorStore.getState().open("f1");
    useEditorStore.getState().setContent("changed");
    const res = await useEditorStore.getState().open("f2");
    expect(res).toBe("confirm");
    expect(useEditorStore.getState().openFile?.resource.id).toBe("f1");
    expect(useEditorStore.getState().pendingOpenId).toBe("f2");
  });

  it("close returns confirm when dirty", async () => {
    await useEditorStore.getState().open("f1");
    useEditorStore.getState().setContent("changed");
    const res = await useEditorStore.getState().close();
    expect(res).toBe("confirm");
    expect(useEditorStore.getState().pendingClose).toBe(true);
    expect(useEditorStore.getState().openFile).not.toBeNull();
  });

  it("resolveClose(false) discards and closes", async () => {
    await useEditorStore.getState().open("f1");
    useEditorStore.getState().setContent("changed");
    await useEditorStore.getState().close();
    await useEditorStore.getState().resolveClose(false);
    expect(useEditorStore.getState().openFile).toBeNull();
    expect(useEditorStore.getState().pendingClose).toBe(false);
  });

  it("resolveClose(true) saves then closes", async () => {
    await useEditorStore.getState().open("f1");
    useEditorStore.getState().setContent("changed");
    await useEditorStore.getState().close();
    await useEditorStore.getState().resolveClose(true);
    expect(mockCall).toHaveBeenCalledWith(
      "save_file",
      expect.objectContaining({ resourceId: "f1" }),
    );
    expect(useEditorStore.getState().openFile).toBeNull();
  });

  it("cancelPending keeps current state", async () => {
    await useEditorStore.getState().open("f1");
    useEditorStore.getState().setContent("changed");
    await useEditorStore.getState().open("f2");
    useEditorStore.getState().cancelPending();
    expect(useEditorStore.getState().pendingOpenId).toBeNull();
    expect(useEditorStore.getState().openFile?.resource.id).toBe("f1");
    expect(useEditorStore.getState().content).toBe("changed");
  });
});
