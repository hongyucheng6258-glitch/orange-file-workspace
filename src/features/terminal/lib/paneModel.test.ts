import { describe, expect, it } from "vitest";
import {
  addPane,
  createPane,
  createSplitTab,
  findPane,
  firstPane,
  isSinglePane,
  removePane,
  setDirection,
  updatePane,
} from "./paneModel";

function makeTab() {
  return createSplitTab({
    key: 1,
    title: "PowerShell 1",
    shell: "powershell",
    paneKey: 10,
  });
}

describe("paneModel", () => {
  it("creates a tab with a single pane", () => {
    const tab = makeTab();
    expect(tab.panes).toHaveLength(1);
    expect(tab.direction).toBe("row");
    expect(tab.panes[0].key).toBe(10);
    expect(isSinglePane(tab)).toBe(true);
  });

  it("adds a pane without mutating input", () => {
    const tab = makeTab();
    const next = addPane(tab, 11);
    expect(next.panes).toHaveLength(2);
    expect(tab.panes).toHaveLength(1);
    expect(isSinglePane(next)).toBe(false);
  });

  it("finds pane by key", () => {
    const tab = addPane(makeTab(), 11);
    expect(findPane(tab, 11)?.sessionId).toBe(null);
    expect(findPane(tab, 99)).toBeUndefined();
  });

  it("updates a specific pane immutably", () => {
    const tab = addPane(makeTab(), 11);
    const next = updatePane(tab, 11, { sessionId: 7, busy: true, cwd: "C:\\x" });
    expect(findPane(next, 11)?.sessionId).toBe(7);
    expect(findPane(next, 10)?.sessionId).toBe(null);
    expect(findPane(tab, 11)?.sessionId).toBe(null); // 原对象未变
  });

  it("removes a pane; returns null when none left", () => {
    const tab = addPane(makeTab(), 11);
    const afterRemove = removePane(tab, 10);
    expect(afterRemove).not.toBeNull();
    expect(afterRemove!.panes).toHaveLength(1);
    expect(afterRemove!.panes[0].key).toBe(11);

    const last = removePane(afterRemove!, 11);
    expect(last).toBeNull();
  });

  it("sets direction immutably", () => {
    const tab = makeTab();
    const next = setDirection(tab, "column");
    expect(next.direction).toBe("column");
    expect(tab.direction).toBe("row");
  });

  it("firstPane returns first pane", () => {
    const tab = addPane(makeTab(), 11);
    expect(firstPane(tab)?.key).toBe(10);
  });

  it("createPane initializes state", () => {
    const p = createPane(42);
    expect(p).toEqual({
      key: 42,
      sessionId: null,
      busy: false,
      exited: null,
      error: null,
      seq: 0,
      cwd: "",
    });
  });
});
