import { call } from "./tauri";

/**
 * 启动系统级文件拖出（Windows OLE DoDragDrop）。
 * 必须在元素的 onDragStart 中调用，并先调用 e.preventDefault() 抑制 HTML5 拖拽，
 * 由 Rust 端接管鼠标，将文件拖到资源管理器、桌面等外部目标。
 */
export function startDragOut(ids: string[]): void {
  if (!("__TAURI_INTERNALS__" in window) || ids.length === 0) return;
  call<number>("drag_out", { ids }).catch((error: Error) => {
    window.dispatchEvent(
      new CustomEvent("nexus:drag-error", {
        detail: error.message || "文件拖出失败",
      }),
    );
  });
}
