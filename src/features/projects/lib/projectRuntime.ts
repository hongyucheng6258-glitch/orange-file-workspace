import { listen } from "@tauri-apps/api/event";
import { call } from "../../../lib/tauri";

// ---- DTO 类型（与 Rust serde camelCase 对齐） ----

export type RunState = "starting" | "running" | "stopping" | "exited" | "failed";
export type OutputStream = "stdout" | "stderr";
export type RuntimeKind = "node" | "python" | "rust";

export interface RuntimeCandidate {
  label: string;
  executable: string;
  args: string[];
  confidence: number;
  /** 相对项目根的工作目录（子项目候选）；缺省 = 项目根。 */
  cwd?: string | null;
}

export interface DetectionResult {
  runtimeKind: RuntimeKind | null;
  candidates: RuntimeCandidate[];
  diagnostics: string[];
}

export interface RunConfig {
  projectId: string;
  executable: string;
  args: string[];
  cwd: string;
  envOverrides: Record<string, string | null>;
  expectedPort: number | null;
  previewScheme: string;
}

export interface RunSummary {
  executable: string;
  args: string[];
  cwd: string;
  env: Record<string, string | null>;
  expected_port: number | null;
  preview_scheme: string;
}

export interface RunSnapshot {
  runId: string;
  projectId: string;
  state: RunState;
  cwd: string;
  pid: number | null;
  startedAt: number | null;
  exitCode: number | null;
  errorCode: string | null;
  errorMessage: string | null;
  stopReason: string | null;
  summary: RunSummary;
}

export interface LogEntry {
  seq: number;
  stream: OutputStream;
  text: string;
  truncated: boolean;
}

export interface LogPage {
  entries: LogEntry[];
  nextSeq: number;
}

export interface ConfirmationPreview {
  confirmationId: string;
  summary: RunSummary;
  expiresInSeconds: number;
}

export interface ConfirmationGrant {
  confirmationId: string;
  confirmationHash: string;
}

// ---- 事件 payload ----

export interface StatusPayload {
  runId: string;
  projectId: string;
  state: RunState;
  pid: number | null;
  exitCode: number | null;
  errorCode: string | null;
  errorMessage: string | null;
}

export interface OutputPayload {
  runId: string;
  projectId: string;
  seq: number;
  stream: OutputStream;
  text: string;
  truncated: boolean;
}

export interface ExitedPayload {
  runId: string;
  projectId: string;
  exitCode: number;
  stopReason: string | null;
}

export interface ErrorPayload {
  runId: string;
  projectId: string;
  errorCode: string;
  errorMessage: string;
}

// ---- 命令封装 ----

export function detectProjectRuntime(projectId: string): Promise<DetectionResult> {
  return call<DetectionResult>("detect_project_runtime", { projectId });
}

export function prepareRunConfirmation(
  projectId: string,
  config: RunConfig,
): Promise<ConfirmationPreview> {
  return call<ConfirmationPreview>("prepare_run_confirmation", { projectId, config });
}

export function confirmRunConfig(confirmationId: string): Promise<ConfirmationGrant> {
  return call<ConfirmationGrant>("confirm_run_config", { confirmationId });
}

export function startProjectProcess(
  projectId: string,
  config: RunConfig,
  confirmationHash: string,
): Promise<RunSnapshot> {
  return call<RunSnapshot>("start_project_process", {
    projectId,
    config,
    confirmationHash,
  });
}

export function stopProjectProcess(runId: string): Promise<RunSnapshot> {
  return call<RunSnapshot>("stop_project_process", { runId });
}

export function restartProjectProcess(runId: string): Promise<RunSnapshot> {
  return call<RunSnapshot>("restart_project_process", { runId });
}

export function getProjectRun(projectId: string): Promise<RunSnapshot | null> {
  return call<RunSnapshot | null>("get_project_run", { projectId });
}

/** 项目运行实例条目：快照 + 相对项目根的 cwd（与运行候选对齐）。 */
export interface ProjectRunEntry {
  runId: string;
  cwdRel: string;
  snapshot: RunSnapshot;
}

/** 查询项目全部运行实例（活动 + 各 cwd 最近终态），用于恢复并行子项目。 */
export function listProjectRunsByProject(projectId: string): Promise<ProjectRunEntry[]> {
  return call<ProjectRunEntry[]>("list_project_runs_by_project", { projectId });
}

export function getProcessLogs(runId: string, afterSeq: number): Promise<LogPage> {
  return call<LogPage>("get_process_logs", { runId, afterSeq });
}

// ---- 事件订阅 ----

export interface ProcessEventHandlers {
  onStatus: (p: StatusPayload) => void;
  onOutput: (p: OutputPayload) => void;
  onExited: (p: ExitedPayload) => void;
  onError: (p: ErrorPayload) => void;
}

const EVENT_STATUS = "project-process://status";
const EVENT_OUTPUT = "project-process://output";
const EVENT_EXITED = "project-process://exited";
const EVENT_ERROR = "project-process://error";

/** 订阅项目进程事件，返回取消订阅函数。 */
export async function subscribeProjectProcess(
  handlers: ProcessEventHandlers,
): Promise<() => void> {
  const offs = await Promise.all([
    listen<StatusPayload>(EVENT_STATUS, (e) => handlers.onStatus(e.payload)),
    listen<OutputPayload>(EVENT_OUTPUT, (e) => handlers.onOutput(e.payload)),
    listen<ExitedPayload>(EVENT_EXITED, (e) => handlers.onExited(e.payload)),
    listen<ErrorPayload>(EVENT_ERROR, (e) => handlers.onError(e.payload)),
  ]);
  return () => {
    for (const off of offs) off();
  };
}

// ---- 纯函数（可测试） ----

/** 按 seq 合并日志：去重 + 排序。 */
export function mergeLogs(existing: LogEntry[], incoming: LogEntry[]): LogEntry[] {
  const map = new Map<number, LogEntry>();
  for (const entry of existing) map.set(entry.seq, entry);
  for (const entry of incoming) map.set(entry.seq, entry);
  return [...map.values()].sort((a, b) => a.seq - b.seq);
}

/** 过滤属于指定 runId 的输出事件。 */
export function filterOutputsByRun(
  payloads: OutputPayload[],
  runId: string,
): OutputPayload[] {
  return payloads.filter((p) => p.runId === runId);
}

/** 根据运行快照派生按钮可用状态。 */
export function deriveRunActions(snap: RunSnapshot | null): {
  canStart: boolean;
  canStop: boolean;
  canRestart: boolean;
} {
  if (!snap) return { canStart: true, canStop: false, canRestart: false };
  switch (snap.state) {
    case "starting":
    case "running":
      return { canStart: false, canStop: true, canRestart: true };
    case "stopping":
      return { canStart: false, canStop: false, canRestart: false };
    case "exited":
    case "failed":
      return { canStart: true, canStop: false, canRestart: true };
  }
}
