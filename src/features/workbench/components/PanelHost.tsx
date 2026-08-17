/**
 * PanelHost — 面板内容渲染
 *
 * 渲染标签栏 + 激活标签的内容。
 * 内容通过 ContentRegistry 查找对应的渲染函数。
 */

import { type ReactNode } from "react";
import type { PanelNode, Tab } from "../lib/layoutModel";
import { TabBar } from "./TabBar";

// ─── 内容注册表 ─────────────────────────────────────────────

export type ContentRenderer = (tab: Tab, panelId: string) => ReactNode;

const registry = new Map<string, ContentRenderer>();

/** 注册内容渲染器 */
export function registerContent(type: string, renderer: ContentRenderer) {
  registry.set(type, renderer);
}

/** 获取渲染器 */
function getRenderer(type: string): ContentRenderer | undefined {
  return registry.get(type);
}

// ─── PanelHost 组件 ─────────────────────────────────────────

interface PanelHostProps {
  panel: PanelNode;
  onActivateTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onSplit?: (direction: "row" | "column") => void;
  onClosePanel?: () => void;
  onAddTerminal?: () => void;
  onAddFileTree?: () => void;
}

export function PanelHost({
  panel,
  onActivateTab,
  onCloseTab,
  onSplit,
  onClosePanel,
  onAddTerminal,
  onAddFileTree,
}: PanelHostProps) {
  const activeTab = panel.activeTabId
    ? panel.tabs.find((t) => t.id === panel.activeTabId) ?? null
    : null;

  let content: ReactNode = null;
  if (activeTab) {
    const renderer = getRenderer(activeTab.contentType);
    content = renderer ? renderer(activeTab, panel.id) : (
      <div className="wb-panel-empty">
        <span>未知内容类型: {activeTab.contentType}</span>
      </div>
    );
  } else {
    content = (
      <div className="wb-panel-empty">
        <span className="wb-panel-empty-hint">无打开的标签</span>
      </div>
    );
  }

  return (
    <div className="wb-panel">
      <TabBar
        panel={panel}
        onActivate={onActivateTab}
        onClose={onCloseTab}
        onSplit={onSplit}
        onClosePanel={onClosePanel}
        onAddTerminal={onAddTerminal}
        onAddFileTree={onAddFileTree}
      />
      <div className="wb-panel-body">{content}</div>
    </div>
  );
}
