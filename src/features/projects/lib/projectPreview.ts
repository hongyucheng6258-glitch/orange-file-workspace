import { listen } from "@tauri-apps/api/event";
import { call } from "../../../lib/tauri";

// ---- DTO（与 Rust serde camelCase 对齐） ----

export type PreviewSource = "config" | "args" | "log";
export type PortOwnership = "confirmed" | "unconfirmed";

export interface PreviewTarget {
  runId: string;
  projectId: string;
  url: string;
  scheme: string;
  host: string;
  port: number;
  path: string;
  source: PreviewSource;
  ownership: PortOwnership;
}

// ---- 命令封装 ----

/** 打开项目运行预览：解析目标 → 校验端口监听 → 校验 Job 归属。 */
export function openProjectPreview(runId: string): Promise<PreviewTarget> {
  return call<PreviewTarget>("open_project_preview", { runId });
}

// ---- 事件订阅 ----

const EVENT_PREVIEW_READY = "project-preview://ready";

/** 订阅预览就绪事件，返回取消订阅函数。 */
export async function subscribePreviewReady(
  handler: (target: PreviewTarget) => void,
): Promise<() => void> {
  const off = await listen<PreviewTarget>(EVENT_PREVIEW_READY, (e) =>
    handler(e.payload),
  );
  return off;
}

// ---- 纯函数 ----

/** 预览来源的中文标签。 */
export function previewSourceLabel(source: PreviewSource): string {
  switch (source) {
    case "config":
      return "配置端口";
    case "args":
      return "命令行参数";
    case "log":
      return "日志地址";
  }
}

/** 归属状态中文标签。 */
export function ownershipLabel(ownership: PortOwnership): string {
  return ownership === "confirmed" ? "端口归属已确认" : "端口归属未确认";
}

/** 预览按钮是否可用：运行中才可打开预览。 */
export function canOpenPreview(
  runState: string | null | undefined,
): boolean {
  return runState === "running";
}
