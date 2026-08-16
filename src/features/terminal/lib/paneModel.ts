/**
 * 终端分屏数据模型（纯函数）。
 * 一个标签（Tab）内含多个窗格（Pane），每个 Pane 是一个独立终端会话。
 * direction 决定窗格排列方向：row = 左右分屏，column = 上下分屏。
 */

/** 分屏方向。 */
export type PaneDirection = "row" | "column";

/** 单个窗格（独立会话）的运行时状态。 */
export interface PaneMeta {
  key: number;
  sessionId: number | null;
  busy: boolean;
  exited: number | null;
  error: string | null;
  seq: number;
  /** 会话实际工作目录（spawn 返回）。 */
  cwd: string;
}

/** 标签：包含若干窗格。 */
export interface SplitTab<S extends string = string> {
  key: number;
  title: string;
  shell: S;
  initialCwd?: string;
  direction: PaneDirection;
  panes: PaneMeta[];
}

/** 创建一个初始窗格。 */
export function createPane(key: number): PaneMeta {
  return {
    key,
    sessionId: null,
    busy: false,
    exited: null,
    error: null,
    seq: 0,
    cwd: "",
  };
}

/** 创建一个只含单个窗格的标签。 */
export function createSplitTab<S extends string>(params: {
  key: number;
  title: string;
  shell: S;
  initialCwd?: string;
  paneKey: number;
}): SplitTab<S> {
  return {
    key: params.key,
    title: params.title,
    shell: params.shell,
    initialCwd: params.initialCwd,
    direction: "row",
    panes: [createPane(params.paneKey)],
  };
}

/** 在标签中查找窗格。 */
export function findPane<S extends string>(
  tab: SplitTab<S>,
  paneKey: number,
): PaneMeta | undefined {
  return tab.panes.find((p) => p.key === paneKey);
}

/** 更新标签中的某个窗格，返回新标签（不可变）。 */
export function updatePane<S extends string>(
  tab: SplitTab<S>,
  paneKey: number,
  patch: Partial<PaneMeta>,
): SplitTab<S> {
  return {
    ...tab,
    panes: tab.panes.map((p) => (p.key === paneKey ? { ...p, ...patch } : p)),
  };
}

/** 追加一个新窗格（分屏），返回新标签。 */
export function addPane<S extends string>(tab: SplitTab<S>, paneKey: number): SplitTab<S> {
  return {
    ...tab,
    panes: [...tab.panes, createPane(paneKey)],
  };
}

/** 设置分屏方向，返回新标签。 */
export function setDirection<S extends string>(
  tab: SplitTab<S>,
  direction: PaneDirection,
): SplitTab<S> {
  return { ...tab, direction };
}

/**
 * 移除一个窗格。
 * 返回 null 表示标签内已无窗格（应关闭整个标签）。
 */
export function removePane<S extends string>(
  tab: SplitTab<S>,
  paneKey: number,
): SplitTab<S> | null {
  const panes = tab.panes.filter((p) => p.key !== paneKey);
  if (panes.length === 0) return null;
  return { ...tab, panes };
}

/** 首个窗格（无窗格时返回 null）。 */
export function firstPane<S extends string>(tab: SplitTab<S>): PaneMeta | undefined {
  return tab.panes[0];
}

/** 标签内是否只有一个窗格（未分屏）。 */
export function isSinglePane<S extends string>(tab: SplitTab<S>): boolean {
  return tab.panes.length <= 1;
}
