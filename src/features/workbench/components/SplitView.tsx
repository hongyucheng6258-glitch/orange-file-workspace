/**
 * SplitView — 递归渲染布局树的可拖拽分割容器
 *
 * - SplitNode → 渲染方向排列的子节点，子节点之间有可拖拽的 resize handle
 * - PanelNode → 委托 PanelHost 渲染标签 + 内容
 */

import { useRef, useCallback, type ReactNode } from "react";
import type { LayoutNode, SplitNode, PanelNode } from "../lib/layoutModel";

interface SplitViewProps {
  node: LayoutNode;
  onSetSizes: (splitId: string, sizes: number[]) => void;
  renderPanel: (panel: PanelNode) => ReactNode;
}

const HANDLE = 4; // px

export function SplitView({ node, onSetSizes, renderPanel }: SplitViewProps) {
  if (node.type === "panel") {
    return <>{renderPanel(node)}</>;
  }
  return <SplitRenderer split={node} onSetSizes={onSetSizes} renderPanel={renderPanel} />;
}

// ─── SplitRenderer ──────────────────────────────────────────

interface SplitRendererProps {
  split: SplitNode;
  onSetSizes: (splitId: string, sizes: number[]) => void;
  renderPanel: (panel: PanelNode) => ReactNode;
}

function SplitRenderer({ split, onSetSizes, renderPanel }: SplitRendererProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const draggingRef = useRef<{ index: number; startPos: number; startSizes: number[] } | null>(null);

  const isRow = split.direction === "row";

  const handleMouseDown = useCallback(
    (e: React.MouseEvent, index: number) => {
      e.preventDefault();
      e.stopPropagation();
      draggingRef.current = {
        index,
        startPos: isRow ? e.clientX : e.clientY,
        startSizes: [...split.sizes],
      };

      const onMouseMove = (ev: MouseEvent) => {
        const drag = draggingRef.current;
        if (!drag || !containerRef.current) return;

        const rect = containerRef.current.getBoundingClientRect();
        const total = isRow ? rect.width : rect.height;
        if (total <= 0) return;

        const currentPos = isRow ? ev.clientX : ev.clientY;
        const delta = currentPos - drag.startPos;
        const deltaRatio = delta / total;

        // 在 index 和 index+1 之间分配
        const sizes = [...drag.startSizes];
        const minSize = 0.1; // 最小 10%
        let toMove = deltaRatio;

        // 从 index+1 取出（或还给）
        const available = sizes[index + 1] - minSize;
        if (deltaRatio > 0) {
          toMove = Math.min(deltaRatio, available);
        } else {
          const availableLeft = sizes[index] - minSize;
          toMove = Math.max(deltaRatio, -availableLeft);
        }

        sizes[index] += toMove;
        sizes[index + 1] -= toMove;
        onSetSizes(split.id, sizes);
      };

      const onMouseUp = () => {
        draggingRef.current = null;
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
      document.body.style.cursor = isRow ? "col-resize" : "row-resize";
      document.body.style.userSelect = "none";
    },
    [split.id, split.sizes, isRow, onSetSizes],
  );

  // ─── 构建子节点 + handle ───
  const children: ReactNode[] = [];
  for (let i = 0; i < split.children.length; i++) {
    const child = split.children[i];
    const size = split.sizes[i] ?? 1 / split.children.length;
    const flexBasis = `${size * 100}%`;

    children.push(
      <div
        key={child.id || `child-${i}`}
        className="split-child"
        style={{ flexBasis, flexGrow: 0, flexShrink: 0, overflow: "hidden", minWidth: 0, minHeight: 0 }}
      >
        <SplitView node={child} onSetSizes={onSetSizes} renderPanel={renderPanel} />
      </div>,
    );

    // 在子节点之间插入 resize handle
    if (i < split.children.length - 1) {
      children.push(
        <div
          key={`handle-${i}`}
          className={`split-handle split-handle-${split.direction}`}
          style={{
            flexBasis: `${HANDLE}px`,
            flexGrow: 0,
            flexShrink: 0,
          }}
          onMouseDown={(e) => handleMouseDown(e, i)}
          onDoubleClick={() => {
            // 双击 → 均分
            const sizes = [...split.sizes];
            const avg = (sizes[i] + sizes[i + 1]) / 2;
            sizes[i] = avg;
            sizes[i + 1] = avg;
            onSetSizes(split.id, sizes);
          }}
        />,
      );
    }
  }

  return (
    <div
      ref={containerRef}
      className={`split-container split-${split.direction}`}
      style={{ display: "flex", flexDirection: split.direction === "row" ? "row" : "column", width: "100%", height: "100%", overflow: "hidden" }}
    >
      {children}
    </div>
  );
}
