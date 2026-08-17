/**
 * 布局数据模型 — 纯函数树操作
 *
 * 树结构：
 *   LayoutNode = SplitNode | PanelNode
 *   SplitNode  — 内部节点，direction 决定子节点排列方向，sizes 为比例值（和为 1）
 *   PanelNode  — 叶子节点，持有多个 Tab，activeTabId 指向当前激活标签
 */

// ─── 类型定义 ──────────────────────────────────────────────

export type ContentType =
  | "terminal"
  | "editor"
  | "filetree"
  | "preview"
  | "browser"
  | "log"
  | "image"
  | "welcome";

export interface Tab {
  id: string;
  title: string;
  icon: string;
  contentType: ContentType;
  params: Record<string, unknown>;
}

export interface PanelNode {
  type: "panel";
  id: string;
  tabs: Tab[];
  activeTabId: string | null;
}

export interface SplitNode {
  type: "split";
  id: string;
  direction: "row" | "column";
  children: LayoutNode[];
  sizes: number[];
}

export type LayoutNode = SplitNode | PanelNode;

// ─── ID 生成 ───────────────────────────────────────────────

let _seq = 0;
export function nextId(prefix: string): string {
  _seq += 1;
  return `${prefix}_${Date.now().toString(36)}_${_seq}`;
}

// ─── 工厂函数 ───────────────────────────────────────────────

export function makeTab(partial: Partial<Tab> & { contentType: ContentType }): Tab {
  return {
    id: nextId("tab"),
    title: partial.title ?? "Untitled",
    icon: partial.icon ?? "",
    contentType: partial.contentType,
    params: partial.params ?? {},
  };
}

export function makePanel(tab?: Tab): PanelNode {
  return {
    type: "panel",
    id: nextId("panel"),
    tabs: tab ? [tab] : [],
    activeTabId: tab?.id ?? null,
  };
}

export function makeSplit(
  direction: "row" | "column",
  children: LayoutNode[],
): SplitNode {
  const n = children.length;
  return {
    type: "split",
    id: nextId("split"),
    direction,
    children,
    sizes: Array(n).fill(1 / n),
  };
}

/** 创建初始布局：单面板 + 可选首个标签 */
export function createLayout(firstTab?: Tab): LayoutNode {
  return makePanel(firstTab);
}

// ─── 树遍历 ─────────────────────────────────────────────────

/** 深度优先查找 PanelNode */
export function findPanel(root: LayoutNode, panelId: string): PanelNode | null {
  if (root.type === "panel") {
    return root.id === panelId ? root : null;
  }
  for (const child of root.children) {
    const found = findPanel(child, panelId);
    if (found) return found;
  }
  return null;
}

/** 深度优先查找 SplitNode */
export function findSplit(root: LayoutNode, splitId: string): SplitNode | null {
  if (root.type === "panel") return null;
  if (root.id === splitId) return root;
  for (const child of root.children) {
    const found = findSplit(child, splitId);
    if (found) return found;
  }
  return null;
}

/** 查找包含指定 panelId 的父 SplitNode */
export function findParentSplit(
  root: LayoutNode,
  panelId: string,
): SplitNode | null {
  if (root.type === "panel") return null;
  for (const child of root.children) {
    if (child.type === "panel" && child.id === panelId) return root;
    const found = findParentSplit(child, panelId);
    if (found) return found;
  }
  return null;
}

/** 列出所有 PanelNode */
export function getAllPanels(root: LayoutNode): PanelNode[] {
  if (root.type === "panel") return [root];
  return root.children.flatMap(getAllPanels);
}

/** 列出所有 Tab */
export function getAllTabs(root: LayoutNode): Tab[] {
  return getAllPanels(root).flatMap((p) => p.tabs);
}

/** 查找包含指定 tabId 的 PanelNode */
export function findPanelByTab(root: LayoutNode, tabId: string): PanelNode | null {
  return getAllPanels(root).find((p) => p.tabs.some((t) => t.id === tabId)) ?? null;
}

// ─── 不可变树操作 ───────────────────────────────────────────

/**
 * 对树中匹配 panelId 的节点执行替换。
 * 如果替换函数返回 null，则从父节点中移除该子节点。
 * 返回 [newRoot, replaced] — replaced 为是否发生了替换。
 */
function replacePanel(
  root: LayoutNode,
  panelId: string,
  fn: (panel: PanelNode) => PanelNode | null,
): [LayoutNode, boolean] {
  if (root.type === "panel") {
    if (root.id === panelId) {
      const result = fn(root);
      if (result === null) {
        // 从父中移除 — 但根节点无法自我移除
        return [root, false];
      }
      return [result, true];
    }
    return [root, false];
  }

  let changed = false;
  const newChildren: LayoutNode[] = [];
  const newSizes: number[] = [];

  for (let i = 0; i < root.children.length; i++) {
    const child = root.children[i];
    if (child.type === "panel" && child.id === panelId) {
      const result = fn(child);
      if (result !== null) {
        newChildren.push(result);
        newSizes.push(root.sizes[i]);
      }
      // result === null → 跳过（移除）
      changed = true;
    } else {
      const [newChild, subChanged] = replacePanel(child, panelId, fn);
      newChildren.push(newChild);
      newSizes.push(root.sizes[i]);
      if (subChanged) changed = true;
    }
  }

  if (!changed) return [root, false];

  // 如果只剩一个子节点，折叠 split（用唯一子节点替代）
  if (newChildren.length === 1) {
    return [newChildren[0], true];
  }

  // 重新归一化 sizes
  const total = newSizes.reduce((a, b) => a + b, 0);
  if (total > 0) {
    for (let i = 0; i < newSizes.length; i++) {
      newSizes[i] /= total;
    }
  }

  return [{ ...root, children: newChildren, sizes: newSizes }, true];
}

/** 递归替换 split 节点 */
function replaceSplit(
  root: LayoutNode,
  splitId: string,
  fn: (split: SplitNode) => SplitNode | null,
): [LayoutNode, boolean] {
  if (root.type === "panel") return [root, false];

  if (root.id === splitId) {
    const result = fn(root);
    if (result === null) return [root, false];
    return [result, true];
  }

  let changed = false;
  const newChildren: LayoutNode[] = [];
  for (const child of root.children) {
    const [newChild, subChanged] = replaceSplit(child, splitId, fn);
    newChildren.push(newChild);
    if (subChanged) changed = true;
  }

  if (!changed) return [root, false];
  return [{ ...root, children: newChildren }, true];
}

// ─── 公共操作 ───────────────────────────────────────────────

/** 向面板添加标签并设为激活 */
export function addTab(root: LayoutNode, panelId: string, tab: Tab): LayoutNode {
  const [newRoot] = replacePanel(root, panelId, (p) => ({
    ...p,
    tabs: [...p.tabs, tab],
    activeTabId: tab.id,
  }));
  return newRoot;
}

/** 设置面板的激活标签 */
export function setActiveTab(
  root: LayoutNode,
  panelId: string,
  tabId: string,
): LayoutNode {
  const [newRoot] = replacePanel(root, panelId, (p) =>
    p.tabs.some((t) => t.id === tabId) ? { ...p, activeTabId: tabId } : p,
  );
  return newRoot;
}

/** 关闭面板中的指定标签。标签关闭后面板为空时自动移除面板。 */
export function closeTab(
  root: LayoutNode,
  panelId: string,
  tabId: string,
): LayoutNode {
  // 根面板无法被移除，单独处理：清空标签但保留面板
  if (root.type === "panel" && root.id === panelId) {
    const tabs = root.tabs.filter((t) => t.id !== tabId);
    if (tabs.length === 0) {
      return { ...root, tabs: [], activeTabId: null };
    }
    const activeTabId =
      root.activeTabId === tabId ? tabs[0].id : root.activeTabId;
    return { ...root, tabs, activeTabId };
  }

  const [newRoot] = replacePanel(root, panelId, (p) => {
    const tabs = p.tabs.filter((t) => t.id !== tabId);
    if (tabs.length === 0) {
      // 面板变空 — 从父中移除
      return null;
    }
    const activeTabId =
      p.activeTabId === tabId ? tabs[0].id : p.activeTabId;
    return { ...p, tabs, activeTabId };
  });

  return newRoot;
}

/**
 * 分割面板：在指定面板的位置插入一个同方向的 SplitNode，
 * 原面板 + 新面板作为两个子节点。
 * 返回 [newRoot, newPanelId]。
 */
export function splitPanel(
  root: LayoutNode,
  panelId: string,
  direction: "row" | "column",
  newTab?: Tab,
): [LayoutNode, string | null] {
  const newPanel = makePanel(newTab);
  let newPanelId: string | null = null;

  function walk(node: LayoutNode): LayoutNode {
    if (node.type === "panel") {
      if (node.id !== panelId) return node;
      // 目标面板：创建新 split 包裹原面板 + 新面板
      const split = makeSplit(direction, [node, newPanel]);
      newPanelId = newPanel.id;
      return split;
    }
    // 检查目标面板是否是此 split 的直接子节点
    const idx = node.children.findIndex(
      (c) => c.type === "panel" && c.id === panelId,
    );
    if (idx >= 0 && node.direction === direction) {
      // 同方向 — 直接插入新面板到子节点列表
      const newChildren = [...node.children];
      const newSizes = [...node.sizes];
      const oldSize = newSizes[idx];
      newChildren.splice(idx + 1, 0, newPanel);
      newSizes.splice(idx + 1, 0, 0);
      newSizes[idx] = oldSize / 2;
      newSizes[idx + 1] = oldSize / 2;
      newPanelId = newPanel.id;
      return { ...node, children: newChildren, sizes: newSizes };
    }
    // 递归遍历子节点
    let changed = false;
    const newChildren = node.children.map((child) => {
      const newChild = walk(child);
      if (newChild !== child) changed = true;
      return newChild;
    });
    if (!changed) return node;
    return { ...node, children: newChildren };
  }

  const newRoot = walk(root);
  return [newRoot, newPanelId];
}

/**
 * 关闭整个面板（所有标签）。
 * 如果面板是 split 的唯一子节点，用另一个子节点替换 split。
 */
export function closePanel(root: LayoutNode, panelId: string): LayoutNode {
  const [newRoot] = replacePanel(root, panelId, () => null);
  return newRoot;
}

/**
 * 在面板间移动标签。
 * fromPanelId 源面板, tabId 要移动的标签,
 * toPanelId 目标面板, index 插入位置（可选，默认末尾）。
 */
export function moveTab(
  root: LayoutNode,
  fromPanelId: string,
  tabId: string,
  toPanelId: string,
  index?: number,
): LayoutNode {
  if (fromPanelId === toPanelId) {
    // 同面板内移动
    const [r] = replacePanel(root, fromPanelId, (p) => {
      const idx = p.tabs.findIndex((t) => t.id === tabId);
      if (idx < 0) return p;
      const tabs = [...p.tabs];
      const [tab] = tabs.splice(idx, 1);
      const insertAt = index ?? tabs.length;
      tabs.splice(insertAt, 0, tab);
      return { ...p, tabs, activeTabId: tabId };
    });
    return r;
  }

  // 跨面板移动：先取出 tab，再插入
  let movingTab: Tab | null = null;
  const [r1] = replacePanel(root, fromPanelId, (p) => {
    const tab = p.tabs.find((t) => t.id === tabId);
    if (!tab) return p;
    movingTab = tab;
    const tabs = p.tabs.filter((t) => t.id !== tabId);
    if (tabs.length === 0) return null;
    const activeTabId =
      p.activeTabId === tabId ? tabs[0].id : p.activeTabId;
    return { ...p, tabs, activeTabId };
  });

  if (!movingTab) return r1;

  const [r2] = replacePanel(r1, toPanelId, (p) => {
    const tabs = [...p.tabs];
    const insertAt = index ?? tabs.length;
    tabs.splice(insertAt, 0, movingTab!);
    return { ...p, tabs, activeTabId: movingTab!.id };
  });

  return r2;
}

/** 更新分割节点的比例 */
export function setSplitSizes(
  root: LayoutNode,
  splitId: string,
  sizes: number[],
): LayoutNode {
  const [newRoot] = replaceSplit(root, splitId, (s) => {
    if (s.children.length !== sizes.length) return s;
    // 归一化
    const total = sizes.reduce((a, b) => a + b, 0);
    const normalized = total > 0 ? sizes.map((v) => v / total) : sizes;
    return { ...s, sizes: normalized };
  });
  return newRoot;
}

/** 更新标签信息（标题等） */
export function updateTab(
  root: LayoutNode,
  panelId: string,
  tabId: string,
  updates: Partial<Tab>,
): LayoutNode {
  const [newRoot] = replacePanel(root, panelId, (p) => ({
    ...p,
    tabs: p.tabs.map((t) =>
      t.id === tabId ? { ...t, ...updates } : t,
    ),
  }));
  return newRoot;
}

/** 获取面板中的激活标签 */
export function getActiveTab(panel: PanelNode): Tab | null {
  if (!panel.activeTabId) return null;
  return panel.tabs.find((t) => t.id === panel.activeTabId) ?? null;
}

/** 统计布局中的标签总数 */
export function countTabs(root: LayoutNode): number {
  return getAllPanels(root).reduce((sum, p) => sum + p.tabs.length, 0);
}

/** 统计布局中的面板总数 */
export function countPanels(root: LayoutNode): number {
  return getAllPanels(root).length;
}
