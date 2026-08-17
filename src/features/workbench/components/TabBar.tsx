/**
 * TabBar — 通用标签栏组件
 *
 * 支持点击切换、关闭、右键菜单（分割/关闭）。
 * + 按钮可创建新终端/文件树标签。
 */

import { useState, useRef, useEffect } from "react";
import type { Tab, PanelNode } from "../lib/layoutModel";

interface TabBarProps {
  panel: PanelNode;
  onActivate: (tabId: string) => void;
  onClose: (tabId: string) => void;
  onSplit?: (direction: "row" | "column") => void;
  onClosePanel?: () => void;
  onAddTerminal?: () => void;
  onAddFileTree?: () => void;
}

const ICON_MAP: Record<string, string> = {
  terminal: "▣",
  editor: "🗎",
  filetree: "⊟",
  preview: "◉",
  browser: "⌖",
  log: "☰",
  image: "🖼",
  pdf: "📄",
  csv: "▦",
  archive: "📦",
  video: "▶",
  audio: "♪",
  markdown: "M",
  welcome: "★",
};

export function TabBar({
  panel,
  onActivate,
  onClose,
  onSplit,
  onClosePanel,
  onAddTerminal,
  onAddFileTree,
}: TabBarProps) {
  const { tabs, activeTabId } = panel;
  const [showMenu, setShowMenu] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!showMenu) return;
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setShowMenu(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [showMenu]);

  const hasAdd = onAddTerminal || onAddFileTree;

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

      <div className="wb-tabbar-actions">
        {hasAdd && (
          <div className="wb-tabbar-add" ref={menuRef}>
            <button
              className="wb-tabbar-btn"
              title="新建标签"
              onClick={() => setShowMenu((v) => !v)}
            >
              +
            </button>
            {showMenu && (
              <div className="wb-tabbar-menu">
                {onAddTerminal && (
                  <button
                    className="wb-tabbar-menu-item"
                    onClick={() => {
                      setShowMenu(false);
                      onAddTerminal();
                    }}
                  >
                    <span className="wb-tabbar-menu-icon">▣</span>
                    新终端
                  </button>
                )}
                {onAddFileTree && (
                  <button
                    className="wb-tabbar-menu-item"
                    onClick={() => {
                      setShowMenu(false);
                      onAddFileTree();
                    }}
                  >
                    <span className="wb-tabbar-menu-icon">⊟</span>
                    文件树
                  </button>
                )}
              </div>
            )}
          </div>
        )}
        {onSplit && (
          <>
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
          </>
        )}
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
    </div>
  );
}

export type { Tab };
