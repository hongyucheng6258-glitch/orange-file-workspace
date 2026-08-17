/**
 * TabBar — 通用标签栏组件
 *
 * 支持点击切换、关闭、右键菜单（分割/关闭）。
 */

import type { Tab, PanelNode } from "../lib/layoutModel";

interface TabBarProps {
  panel: PanelNode;
  onActivate: (tabId: string) => void;
  onClose: (tabId: string) => void;
  onSplit?: (direction: "row" | "column") => void;
  onClosePanel?: () => void;
}

const ICON_MAP: Record<string, string> = {
  terminal: "▣",
  editor: "🗎",
  filetree: "⊟",
  preview: "◉",
  browser: "⌖",
  log: "☰",
  image: "🖼",
  welcome: "★",
};

export function TabBar({ panel, onActivate, onClose, onSplit, onClosePanel }: TabBarProps) {
  const { tabs, activeTabId } = panel;

  return (
    <div className="wb-tabbar" onContextMenu={(e) => e.preventDefault()}>
      <div className="wb-tabbar-tabs">
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          const icon = tab.icon || ICON_MAP[tab.contentType] || "•";
          return (
            <div
              key={tab.id}
              className={`wb-tab ${isActive ? "wb-tab-active" : ""}`}
              onClick={() => onActivate(tab.id)}
              onMouseDown={(e) => {
                if (e.button === 1) {
                  e.preventDefault();
                  onClose(tab.id);
                }
              }}
              title={tab.title}
            >
              <span className="wb-tab-icon">{icon}</span>
              <span className="wb-tab-title">{tab.title}</span>
              {tabs.length > 1 && (
                <button
                  className="wb-tab-close"
                  onClick={(e) => {
                    e.stopPropagation();
                    onClose(tab.id);
                  }}
                >
                  ×
                </button>
              )}
            </div>
          );
        })}
      </div>

      {onSplit && (
        <div className="wb-tabbar-actions">
          <button
            className="wb-tabbar-btn"
            title="水平分割"
            onClick={() => onSplit("row")}
          >
            ◧
          </button>
          <button
            className="wb-tabbar-btn"
            title="垂直分割"
            onClick={() => onSplit("column")}
          >
            ◨
          </button>
          {onClosePanel && tabs.length === 0 && (
            <button
              className="wb-tabbar-btn"
              title="关闭面板"
              onClick={onClosePanel}
            >
              ✕
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export type { Tab };
