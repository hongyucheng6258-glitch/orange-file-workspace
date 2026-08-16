import { call } from "./tauri";

export type DragOutStatus = {
  phase: "starting" | "completed" | "error";
  message: string;
};

function announce(status: DragOutStatus): void {
  window.dispatchEvent(
    new CustomEvent<DragOutStatus>("nexus:drag-status", { detail: status }),
  );
}

/** 启动系统级文件拖出（Windows 原生拖拽）。 */
export function startDragOut(ids: string[]): void {
  if (!("__TAURI_INTERNALS__" in window) || ids.length === 0) return;

  announce({ phase: "starting", message: "正在启动系统拖拽…" });
  call<number>("drag_out", { ids })
    .then(() => {
      announce({ phase: "completed", message: "拖拽已结束" });
    })
    .catch((error: Error) => {
      announce({
        phase: "error",
        message: error.message || "文件拖出失败",
      });
    });
}
