import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockCall = vi.fn();
vi.mock("../../../lib/tauri", () => ({
  call: (...args: unknown[]) => mockCall(...args),
}));

import { EMPTY_DOC, usePageStore } from "./pageStore";

const detailP1 = {
  resource: { id: "p1", kind: "page", name: "P1", created_at: 1, updated_at: 1 },
  page: {
    resource_id: "p1",
    summary: null,
    content_version: 1,
    save_state: "clean",
    content_json: null,
  },
};
const detailP2 = {
  resource: { id: "p2", kind: "page", name: "P2", created_at: 1, updated_at: 1 },
  page: {
    resource_id: "p2",
    summary: null,
    content_version: 1,
    save_state: "clean",
    content_json: null,
  },
};

beforeEach(() => {
  mockCall.mockReset();
  mockCall.mockImplementation((cmd: string) => {
    if (cmd === "list_pages") return Promise.resolve([]);
    if (cmd === "get_page") return Promise.resolve(detailP1);
    if (cmd === "save_page_document") return Promise.resolve(undefined);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  usePageStore.setState({
    tree: [],
    currentPageId: null,
    detail: null,
    document: EMPTY_DOC,
    dirty: false,
    loading: false,
    saving: false,
    pendingTarget: null,
  });
});

describe("pageStore dirty guard", () => {
  it("openPage switches when clean", async () => {
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "get_page") return Promise.resolve(detailP2);
      return Promise.resolve([]);
    });
    const res = await usePageStore.getState().openPage("p2");
    expect(res).toBe("opened");
    expect(usePageStore.getState().currentPageId).toBe("p2");
  });

  it("openPage returns confirm and keeps current page when dirty", async () => {
    await usePageStore.getState().openPage("p1");
    usePageStore.getState().setDocument({ type: "doc", content: [{ type: "paragraph" }] });
    const res = await usePageStore.getState().openPage("p2");
    expect(res).toBe("confirm");
    expect(usePageStore.getState().currentPageId).toBe("p1");
    expect(usePageStore.getState().pendingTarget).toBe("p2");
  });

  it("resolveOpen(true) saves then opens pending target", async () => {
    await usePageStore.getState().openPage("p1");
    usePageStore.getState().setDocument({ type: "doc", content: [{ type: "paragraph" }] });
    await usePageStore.getState().openPage("p2");
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "get_page") return Promise.resolve(detailP2);
      return Promise.resolve(undefined);
    });
    await usePageStore.getState().resolveOpen(true);
    expect(mockCall).toHaveBeenCalledWith("save_page_document", expect.any(Object));
    expect(usePageStore.getState().currentPageId).toBe("p2");
    expect(usePageStore.getState().pendingTarget).toBeNull();
  });

  it("resolveOpen(false) discards and opens pending target", async () => {
    await usePageStore.getState().openPage("p1");
    usePageStore.getState().setDocument({ type: "doc", content: [{ type: "paragraph" }] });
    await usePageStore.getState().openPage("p2");
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "get_page") return Promise.resolve(detailP2);
      return Promise.resolve(undefined);
    });
    await usePageStore.getState().resolveOpen(false);
    expect(mockCall).not.toHaveBeenCalledWith("save_page_document", expect.any(Object));
    expect(usePageStore.getState().currentPageId).toBe("p2");
  });

  it("cancelOpen keeps current page and clears pending", async () => {
    await usePageStore.getState().openPage("p1");
    usePageStore.getState().setDocument({ type: "doc", content: [{ type: "paragraph" }] });
    await usePageStore.getState().openPage("p2");
    usePageStore.getState().cancelOpen();
    expect(usePageStore.getState().currentPageId).toBe("p1");
    expect(usePageStore.getState().pendingTarget).toBeNull();
  });
});
