import { Channel, invoke } from "@tauri-apps/api/core";

/** 支持的 Shell 白名单（与后端一致）。 */
export type TerminalShell = "powershell" | "cmd" | "gitbash" | "wsl";

/** 后端探测的 Shell 信息（终端页下拉数据源）。 */
export interface TerminalShellInfo {
  id: TerminalShell;
  label: string;
  available: boolean;
}

/** 后端 Channel 事件：output 携带原始字节数组，exit 携带退出码（未知为 null）。 */
export interface TerminalEvent {
  type: "output" | "exit";
  data: number[] | number | null;
}

export interface TerminalSessionInfo {
  session_id: number;
  cwd: string;
  shell: string;
}

/** 启动终端会话，返回会话信息（session_id/cwd/shell）。 */
export function spawnTerminal(params: {
  shell: TerminalShell;
  cwd?: string;
  cols: number;
  rows: number;
  channel: Channel<TerminalEvent>;
}): Promise<TerminalSessionInfo> {
  return invoke("terminal_spawn", {
    shell: params.shell,
    cwd: params.cwd,
    cols: params.cols,
    rows: params.rows,
    channel: params.channel,
  });
}

/** 向会话写入输入（UTF-8）。 */
export function terminalWrite(sessionId: number, data: string): Promise<void> {
  return invoke("terminal_write", { sessionId, data });
}

/** 调整会话窗口大小。 */
export function terminalResize(
  sessionId: number,
  cols: number,
  rows: number,
): Promise<void> {
  return invoke("terminal_resize", { sessionId, cols, rows });
}

/** 结束会话（幂等）。 */
export function terminalClose(sessionId: number): Promise<void> {
  return invoke("terminal_close", { sessionId });
}

/** 列出存活会话。 */
export function terminalList(): Promise<TerminalSessionInfo[]> {
  return invoke("terminal_list");
}

/** 探测本机可用 Shell（内置 + Git Bash/WSL）。 */
export function listTerminalShells(): Promise<TerminalShellInfo[]> {
  return invoke("terminal_list_shells");
}

/** 单条命令历史记录。 */
export interface TerminalHistoryEntry {
  id: number;
  shell: string;
  command: string;
  cwd: string;
  created_at: number;
}

/** 记录一条命令历史（后端负责空命令忽略与连续去重）。 */
export function recordTerminalHistory(params: {
  shell: TerminalShell;
  command: string;
  cwd?: string;
}): Promise<void> {
  return invoke("terminal_history_record", {
    shell: params.shell,
    command: params.command,
    cwd: params.cwd,
  });
}

/** 查询某 Shell 的命令历史（按时间倒序）。 */
export function listTerminalHistory(
  shell: TerminalShell,
  limit?: number,
): Promise<TerminalHistoryEntry[]> {
  return invoke("terminal_history_list", { shell, limit });
}

/** 清空命令历史（shell 缺省时清空全部）。 */
export function clearTerminalHistory(shell?: TerminalShell): Promise<void> {
  return invoke("terminal_history_clear", { shell });
}
