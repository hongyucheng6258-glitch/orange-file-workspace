/**
 * 布局 Store — Zustand 状态管理
 *
 * 管理布局树（LayoutNode）及所有面板/标签操作。
 * 支持多组独立的布局（例如终端工作台、项目工作台各持一组）。
 */

import { create } from "zustand";
import {
  type LayoutNode,
  type Tab,
  type ContentType,
  type PanelNode,
  createLayout,
  makeTab,
  addTab as modelAddTab,
  closeTab as modelCloseTab,
  setActiveTab as modelSetActiveTab,
  splitPanel as modelSplitPanel,
  closePanel as modelClosePanel,
  moveTab as modelMoveTab,
  setSplitSizes as modelSetSplitSizes,
  updateTab as modelUpdateTab,
  findPanel,
  findPanelByTab,
  getAllPanels,
  getAllTabs,
  countTabs,
  countPanels,
  getActiveTab,
} from "../lib/layoutModel";

// ─── 类型 ───────────────────────────────────────────────────

export interface LayoutActions {
  /** 初始化布局（如果当前为空） */
  initLayout: (firstTab?: Tab) => void;
  /** 重置为单面板布局 */
  resetLayout: (firstTab?: Tab) => void;

  /** 向指定面板添加标签 */
  addTab: (panelId: string, tab: Tab) => void;
  /** 快捷添加标签：自动创建 Tab 并加入面板 */
  openTab: (
    panelId: string,
    contentType: ContentType,
    title: string,
    params?: Record<string, unknown>,
    icon?: string,
  ) => string;
  /** 关闭面板中的指定标签 */
  closeTab: (panelId: string, tabId: string) => void;
  /** 设置面板激活标签 */
  setActiveTab: (panelId: string, tabId: string) => void;
  /** 分割面板，返回新面板 ID */
  splitPanel: (
    panelId: string,
    direction: "row" | "column",
    newTab?: Tab,
  ) => string | null;
  /** 关闭整个面板 */
  closePanel: (panelId: string) => void;
  /** 在面板间移动标签 */
  moveTab: (
    fromPanelId: string,
    tabId: string,
    toPanelId: string,
    index?: number,
  ) => void;
  /** 更新分割节点比例 */
  setSplitSizes: (splitId: string, sizes: number[]) => void;
  /** 更新标签信息 */
  updateTab: (
    panelId: string,
    tabId: string,
    updates: Partial<Tab>,
  ) => void;

  // ─── 查询方法 ───
  getPanel: (panelId: string) => PanelNode | null;
  getPanelByTab: (tabId: string) => PanelNode | null;
  getActiveTabOf: (panelId: string) => Tab | null;
  getAllPanels: () => PanelNode[];
  getAllTabs: () => Tab[];
  countTabs: () => number;
  countPanels: () => number;
  findTab: (tabId: string) => Tab | null;
}

export type LayoutStore = {
  root: LayoutNode;
} & LayoutActions;

// ─── Store 创建 ─────────────────────────────────────────────

export function createLayoutStore() {
  return create<LayoutStore>((set, get) => ({
    root: createLayout(),

    initLayout: (firstTab) => {
      if (countTabs(get().root) > 0) return;
      set({ root: createLayout(firstTab) });
    },

    resetLayout: (firstTab) => {
      set({ root: createLayout(firstTab) });
    },

    addTab: (panelId, tab) => {
      const root = get().root;
      set({ root: modelAddTab(root, panelId, tab) });
    },

    openTab: (panelId, contentType, title, params, icon) => {
      const tab = makeTab({ contentType, title, params, icon });
      get().addTab(panelId, tab);
      return tab.id;
    },

    closeTab: (panelId, tabId) => {
      const root = get().root;
      set({ root: modelCloseTab(root, panelId, tabId) });
    },

    setActiveTab: (panelId, tabId) => {
      const root = get().root;
      set({ root: modelSetActiveTab(root, panelId, tabId) });
    },

    splitPanel: (panelId, direction, newTab) => {
      const root = get().root;
      const [newRoot, newPanelId] = modelSplitPanel(root, panelId, direction, newTab);
      set({ root: newRoot });
      return newPanelId;
    },

    closePanel: (panelId) => {
      const root = get().root;
      set({ root: modelClosePanel(root, panelId) });
    },

    moveTab: (fromPanelId, tabId, toPanelId, index) => {
      const root = get().root;
      set({ root: modelMoveTab(root, fromPanelId, tabId, toPanelId, index) });
    },

    setSplitSizes: (splitId, sizes) => {
      const root = get().root;
      set({ root: modelSetSplitSizes(root, splitId, sizes) });
    },

    updateTab: (panelId, tabId, updates) => {
      const root = get().root;
      set({ root: modelUpdateTab(root, panelId, tabId, updates) });
    },

    // ─── 查询 ───
    getPanel: (panelId) => findPanel(get().root, panelId),
    getPanelByTab: (tabId) => findPanelByTab(get().root, tabId),
    getActiveTabOf: (panelId) => {
      const panel = findPanel(get().root, panelId);
      return panel ? getActiveTab(panel) : null;
    },
    getAllPanels: () => getAllPanels(get().root),
    getAllTabs: () => getAllTabs(get().root),
    countTabs: () => countTabs(get().root),
    countPanels: () => countPanels(get().root),
    findTab: (tabId) => getAllTabs(get().root).find((t) => t.id === tabId) ?? null,
  }));
}

// ─── 全局默认 Store ─────────────────────────────────────────

/**
 * 全局工作台布局 Store。
 * 用于通用分屏场景（如终端工作台、独立编辑器面板等）。
 */
export const useLayoutStore = createLayoutStore();
