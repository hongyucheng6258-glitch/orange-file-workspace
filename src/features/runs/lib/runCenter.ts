import { call } from "../../../lib/tauri";
import type { Resource } from "../../../lib/types";
import type { RunSnapshot } from "../../projects/lib/projectRuntime";

/** 列出运行实例：活动 + 清理中；includeExited 时追加已退出记录。 */
export function listProjectRuns(includeExited: boolean): Promise<RunSnapshot[]> {
  return call<RunSnapshot[]>("list_project_runs", { includeExited });
}

/** 拉取项目列表用于 run_id → 项目名称映射。 */
export async function loadProjectNames(): Promise<Record<string, string>> {
  try {
    const projects = await call<Resource[]>("list_projects");
    const map: Record<string, string> = {};
    for (const p of projects) map[p.id] = p.name;
    return map;
  } catch {
    return {};
  }
}

/** 时间戳（秒）→ 本地时间字符串；空值返回 "—"。 */
export function formatRunTime(ts: number | null | undefined): string {
  if (!ts) return "—";
  const d = new Date(ts * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** 运行时长（秒）：从 started_at 到 now；空值返回 null。 */
export function runDurationSeconds(startedAt: number | null | undefined): number | null {
  if (!startedAt) return null;
  return Math.max(0, Math.floor(Date.now() / 1000) - startedAt);
}

/** 运行状态中文标签。 */
export function runStateLabel(state: string): string {
  switch (state) {
    case "starting":
      return "启动中";
    case "running":
      return "运行中";
    case "stopping":
      return "停止中";
    case "exited":
      return "已退出";
    case "failed":
      return "失败";
    default:
      return state;
  }
}

/** 从快照摘要中提取端口。 */
export function expectedPortOf(snap: RunSnapshot): number | null {
  return snap.summary.expected_port ?? null;
}

/** 从快照摘要中提取命令摘要。 */
export function commandLineOf(snap: RunSnapshot): string {
  const exe = snap.summary.executable ?? "";
  const args = snap.summary.args ?? [];
  return [exe, ...args].filter(Boolean).join(" ");
}
