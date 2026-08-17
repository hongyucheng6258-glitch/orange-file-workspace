/**
 * 布局数据模型测试
 */

import { describe, it, expect } from "vitest";
import {
  createLayout,
  makeTab,
  makePanel,
  addTab,
  closeTab,
  setActiveTab,
  splitPanel,
  closePanel,
  moveTab,
  setSplitSizes,
  updateTab,
  getAllPanels,
  getAllTabs,
  countTabs,
  countPanels,
  findPanelByTab,
  getActiveTab,
  type LayoutNode,
} from "./layoutModel";

describe("layoutModel", () => {
  // ─── 基础工厂 ───
  describe("createLayout / makeTab / makePanel", () => {
    it("创建空布局（单面板无标签）", () => {
      const root = createLayout();
      expect(root.type).toBe("panel");
      if (root.type !== "panel") return;
      expect(root.tabs).toHaveLength(0);
      expect(root.activeTabId).toBeNull();
    });

    it("创建带首个标签的布局", () => {
      const tab = makeTab({ contentType: "terminal", title: "Terminal 1" });
      const root = createLayout(tab);
      expect(root.type).toBe("panel");
      if (root.type !== "panel") return;
      expect(root.tabs).toHaveLength(1);
      expect(root.tabs[0].title).toBe("Terminal 1");
      expect(root.activeTabId).toBe(tab.id);
    });

    it("makeTab 填充默认值", () => {
      const tab = makeTab({ contentType: "editor" });
      expect(tab.id).toBeTruthy();
      expect(tab.title).toBe("Untitled");
      expect(tab.icon).toBe("");
      expect(tab.params).toEqual({});
    });
  });

  // ─── addTab / setActiveTab ───
  describe("addTab / setActiveTab", () => {
    it("向面板添加标签", () => {
      const root = createLayout();
      if (root.type !== "panel") throw new Error("expected panel");
      const tab = makeTab({ contentType: "terminal", title: "T1" });
      const r = addTab(root, root.id, tab);
      expect(r.type).toBe("panel");
      if (r.type !== "panel") throw new Error("expected panel");
      expect(r.tabs).toHaveLength(1);
      expect(r.activeTabId).toBe(tab.id);
    });

    it("切换激活标签", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      const t2 = makeTab({ contentType: "terminal", title: "T2" });
      const r1 = addTab(root, root.id, t2);
      const r2 = setActiveTab(r1, root.id, t1.id);
      if (r2.type !== "panel") throw new Error("expected panel");
      expect(r2.activeTabId).toBe(t1.id);
    });

    it("切换到不存在的 tabId 不改变状态", () => {
      const tab = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(tab);
      const r = setActiveTab(root, root.id, "nonexistent");
      expect(r).toBe(root);
    });
  });

  // ─── closeTab ───
  describe("closeTab", () => {
    it("关闭非唯一标签后保留面板", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      const t2 = makeTab({ contentType: "terminal", title: "T2" });
      const r1 = addTab(root, root.id, t2);
      const r2 = closeTab(r1, root.id, t1.id);
      if (r2.type !== "panel") throw new Error("expected panel");
      expect(r2.tabs).toHaveLength(1);
      expect(r2.tabs[0].id).toBe(t2.id);
      expect(r2.activeTabId).toBe(t2.id);
    });

    it("关闭最后一个标签后面板被移除", () => {
      const tab = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(tab);
      const r = closeTab(root, root.id, tab.id);
      // 面板变为空 — 从父 split 移除；根面板无法移除所以保留
      if (r.type !== "panel") throw new Error("root panel stays");
      expect(r.tabs).toHaveLength(0);
    });

    it("关闭标签后 split 自动折叠为单面板", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const [r1, newPanelId] = splitPanel(root, root.id, "row", t2);
      expect(newPanelId).not.toBeNull();
      if (!newPanelId) return;

      // 关闭第一个面板的标签 → 第一个面板变空 → split 折叠
      const r2 = closeTab(r1, root.id, t1.id);
      // 现在 root 被移除，只留第二个面板
      expect(r2.type).toBe("panel");
      if (r2.type !== "panel") throw new Error("expected panel");
      expect(r2.tabs).toHaveLength(1);
      expect(r2.tabs[0].id).toBe(t2.id);
    });
  });

  // ─── splitPanel ───
  describe("splitPanel", () => {
    it("分割面板返回新面板 ID", () => {
      const tab = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(tab);
      if (root.type !== "panel") throw new Error("expected panel");
      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const [r, newPanelId] = splitPanel(root, root.id, "row", t2);
      expect(r.type).toBe("split");
      if (r.type !== "split") throw new Error("expected split");
      expect(r.direction).toBe("row");
      expect(r.children).toHaveLength(2);
      expect(r.sizes).toEqual([0.5, 0.5]);
      expect(newPanelId).not.toBeNull();
    });

    it("同方向分割合并到同一个 split", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      if (root.type !== "panel") throw new Error("expected panel");

      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const [r1, panel2Id] = splitPanel(root, root.id, "row", t2);
      if (!panel2Id) throw new Error("no panel id");

      const t3 = makeTab({ contentType: "preview", title: "P1" });
      const [r2, panel3Id] = splitPanel(r1, panel2Id, "row", t3);
      if (!panel3Id) throw new Error("no panel id");

      // 应该还是同一个 split，但 3 个子节点
      expect(r2.type).toBe("split");
      if (r2.type !== "split") throw new Error("expected split");
      expect(r2.children).toHaveLength(3);
      expect(r2.sizes).toHaveLength(3);
    });

    it("不同方向分割创建嵌套 split", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      if (root.type !== "panel") throw new Error("expected panel");

      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const [r1, panel2Id] = splitPanel(root, root.id, "row", t2);
      if (!panel2Id) throw new Error("no panel id");

      const t3 = makeTab({ contentType: "preview", title: "P1" });
      const [r2] = splitPanel(r1, panel2Id, "column", t3);

      // 外层是 row split，内层第二个子节点是 column split
      expect(r2.type).toBe("split");
      if (r2.type !== "split") throw new Error("expected split");
      expect(r2.direction).toBe("row");
      expect(r2.children[1].type).toBe("split");
    });
  });

  // ─── closePanel ───
  describe("closePanel", () => {
    it("关闭面板后 split 折叠", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const [r1, panel2Id] = splitPanel(root, root.id, "row", t2);
      if (!panel2Id) throw new Error("no panel id");

      const r2 = closePanel(r1, panel2Id);
      // split 折叠为单个子节点（原面板）
      expect(r2.type).toBe("panel");
      if (r2.type !== "panel") throw new Error("expected panel");
      expect(r2.tabs[0].id).toBe(t1.id);
    });
  });

  // ─── moveTab ───
  describe("moveTab", () => {
    it("跨面板移动标签", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      if (root.type !== "panel") throw new Error("expected panel");

      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const [r1, panel2Id] = splitPanel(root, root.id, "row", t2);
      if (!panel2Id) throw new Error("no panel id");

      // 把 t2 移到第一个面板
      const r2 = moveTab(r1, panel2Id, t2.id, root.id);
      // panel2 变空被移除，split 折叠
      expect(r2.type).toBe("panel");
      if (r2.type !== "panel") throw new Error("expected panel");
      expect(r2.tabs).toHaveLength(2);
      expect(r2.tabs.map((t) => t.id)).toEqual([t1.id, t2.id]);
    });

    it("同面板内移动标签", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const r1 = addTab(root, root.id, t2);
      const t3 = makeTab({ contentType: "preview", title: "P1" });
      const r2 = addTab(r1, root.id, t3);

      // 把 t3 移到位置 0
      const r3 = moveTab(r2, root.id, t3.id, root.id, 0);
      if (r3.type !== "panel") throw new Error("expected panel");
      expect(r3.tabs.map((t) => t.id)).toEqual([t3.id, t1.id, t2.id]);
    });
  });

  // ─── setSplitSizes ───
  describe("setSplitSizes", () => {
    it("更新分割比例并归一化", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const [r1] = splitPanel(root, root.id, "row", t2);
      if (r1.type !== "split") throw new Error("expected split");

      const r2 = setSplitSizes(r1, r1.id, [3, 1]);
      if (r2.type !== "split") throw new Error("expected split");
      // 3:1 → 0.75:0.25
      expect(r2.sizes[0]).toBeCloseTo(0.75, 5);
      expect(r2.sizes[1]).toBeCloseTo(0.25, 5);
    });
  });

  // ─── updateTab ───
  describe("updateTab", () => {
    it("更新标签标题", () => {
      const tab = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(tab);
      if (root.type !== "panel") throw new Error("expected panel");

      const r = updateTab(root, root.id, tab.id, { title: "Updated" });
      if (r.type !== "panel") throw new Error("expected panel");
      expect(r.tabs[0].title).toBe("Updated");
    });
  });

  // ─── 查询函数 ───
  describe("查询函数", () => {
    it("getAllPanels 返回所有面板", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const [r1] = splitPanel(root, root.id, "row", t2);

      const panels = getAllPanels(r1);
      expect(panels).toHaveLength(2);
    });

    it("getAllTabs 返回所有标签", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const [r1] = splitPanel(root, root.id, "row", t2);

      const tabs = getAllTabs(r1);
      expect(tabs).toHaveLength(2);
      expect(tabs.map((t) => t.id)).toContain(t1.id);
      expect(tabs.map((t) => t.id)).toContain(t2.id);
    });

    it("findPanelByTab 返回包含标签的面板", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const [r1, panel2Id] = splitPanel(root, root.id, "row", t2);
      if (!panel2Id) throw new Error("no panel id");

      const panel = findPanelByTab(r1, t2.id);
      expect(panel).not.toBeNull();
      expect(panel?.id).toBe(panel2Id);
    });

    it("getActiveTab 返回当前激活标签", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const panel = makePanel(t1);
      const active = getActiveTab(panel);
      expect(active?.id).toBe(t1.id);
    });

    it("countTabs / countPanels 统计", () => {
      const t1 = makeTab({ contentType: "terminal", title: "T1" });
      const root = createLayout(t1);
      const t2 = makeTab({ contentType: "editor", title: "E1" });
      const t3 = makeTab({ contentType: "preview", title: "P1" });
      let r: LayoutNode = addTab(root, root.id, t2);
      r = addTab(r, root.id, t3);

      expect(countTabs(r)).toBe(3);
      expect(countPanels(r)).toBe(1);
    });
  });
});
