/**
 * Workbench — 工作台主容器
 *
 * 读取布局 Store，递归渲染 SplitView 树。
 * 每个 PanelNode 委托 PanelHost 渲染标签 + 内容。
 */

import { useCallback } from "react";
import type { UseBoundStore, StoreApi } from "zustand";
import type { PanelNode } from "../lib/layoutModel";
import { useLayoutStore, type LayoutStore } from "../stores/layoutStore";
import { useWorkbenchContext } from "../stores/workbenchContext";
import { SplitView } from "./SplitView";
import { PanelHost } from "./PanelHost";

type StoreHook = UseBoundStore<StoreApi<LayoutStore>>;

interface WorkbenchProps {
  /** 可选自定义 store（默认使用全局 useLayoutStore） */
  store?: StoreHook;
}

export function Workbench({ store }: WorkbenchProps) {
  const useStore = store ?? useLayoutStore;
  const root = useStore((s) => s.root);
  const setSplitSizes = useStore((s) => s.setSplitSizes);
  const setActiveTab = useStore((s) => s.setActiveTab);
  const closeTab = useStore((s) => s.closeTab);
  const splitPanel = useStore((s) => s.splitPanel);
  const closePanel = useStore((s) => s.closePanel);
  const openTab = useStore((s) => s.openTab);
  const cwd = useWorkbenchContext((s) => s.cwd);

  const handleSetSizes = useCallback(
    (splitId: string, sizes: number[]) => setSplitSizes(splitId, sizes),
    [setSplitSizes],
  );

  const renderPanel = useCallback(
    (panel: PanelNode) => {
      return (
        <PanelHost
          panel={panel}
          onActivateTab={(tabId) => setActiveTab(panel.id, tabId)}
          onCloseTab={(tabId) => closeTab(panel.id, tabId)}
          onSplit={(direction) => {
            splitPanel(panel.id, direction);
          }}
          onClosePanel={
            panel.tabs.length === 0 ? () => closePanel(panel.id) : undefined
          }
          onAddTerminal={() =>
            openTab(
              panel.id,
              "terminal",
              "Terminal",
              { shell: "powershell", cwd: cwd ?? undefined },
              "terminal",
            )
          }
          onAddFileTree={() =>
            openTab(panel.id, "filetree", "文件树", {}, "filetree")
          }
        />
      );
    },
    [setActiveTab, closeTab, splitPanel, closePanel, openTab, cwd],
  );

  return (
    <div className="wb-root">
      <SplitView node={root} onSetSizes={handleSetSizes} renderPanel={renderPanel} />
    </div>
  );
}
