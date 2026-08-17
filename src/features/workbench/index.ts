/**
 * Workbench Feature — 公共导出
 */

export { Workbench } from "./components/Workbench";
export { SplitView } from "./components/SplitView";
export { PanelHost, registerContent } from "./components/PanelHost";
export { TabBar } from "./components/TabBar";
export { useLayoutStore, createLayoutStore } from "./stores/layoutStore";
export { useWorkbenchContext } from "./stores/workbenchContext";
export type { LayoutStore } from "./stores/layoutStore";
export type { WorkbenchContextState } from "./stores/workbenchContext";
export * from "./lib/layoutModel";
