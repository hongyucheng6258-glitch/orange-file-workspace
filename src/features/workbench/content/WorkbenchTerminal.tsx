/**
 * Workbench 终端渲染器 — 在面板中管理单个终端会话
 *
 * 从 tab.params 读取 cwd / shell，spawn → write → resize → close
 */

import { useRef, useEffect, useCallback } from "react";
import { Channel } from "@tauri-apps/api/core";
import { TerminalView, type TerminalViewHandle } from "../../terminal/components/TerminalView";
import {
  spawnTerminal,
  terminalWrite,
  terminalResize,
  terminalClose,
  type TerminalEvent,
  type TerminalShell,
} from "../../terminal/lib/terminal";
import { recordTerminalHistory } from "../../terminal/lib/terminal";

interface Props {
  params: Record<string, unknown>;
}

export function WorkbenchTerminal({ params }: Props) {
  const handleRef = useRef<TerminalViewHandle | null>(null);
  const sessionIdRef = useRef<number | null>(null);

  const shell = (params.shell as TerminalShell) ?? "powershell";
  const cwd = params.cwd as string | undefined;

  // onReady: xterm 就绪后 spawn 后端会话
  const onReady = useCallback(
    (handle: TerminalViewHandle) => {
      handleRef.current = handle;
      const channel = new Channel<TerminalEvent>();
      const cols = handle.term.cols;
      const rows = handle.term.rows;

      spawnTerminal({ shell, cwd, cols, rows, channel })
        .then((info) => {
          sessionIdRef.current = info.session_id;
          // 接收后端输出 → 写入 xterm
          channel.onmessage = (event) => {
            if (event.type === "output" && Array.isArray(event.data)) {
              const bytes = new Uint8Array(event.data);
              handle.term.write(bytes);
            } else if (event.type === "exit") {
              const code = event.data;
              handle.term.write(
                `\r\n\x1b[90m[进程退出${code !== null ? ` 代码 ${code}` : ""}]\x1b[0m\r\n`,
              );
            }
          };
        })
        .catch((e) => {
          handle.term.write(`\r\n\x1b[31m启动失败: ${String(e)}\x1b[0m\r\n`);
        });
    },
    [shell, cwd],
  );

  // onData: 用户输入 → 写入后端
  const onData = useCallback((data: string) => {
    const sid = sessionIdRef.current;
    if (sid !== null) void terminalWrite(sid, data);
  }, []);

  // onResize: 窗口尺寸变化 → 同步后端
  const onResize = useCallback((cols: number, rows: number) => {
    const sid = sessionIdRef.current;
    if (sid !== null) void terminalResize(sid, cols, rows);
  }, []);

  // onCommand: 命令执行 → 记录历史
  const onCommand = useCallback(
    (command: string) => {
      void recordTerminalHistory({ shell, command, cwd });
    },
    [shell, cwd],
  );

  // 卸载时关闭会话
  useEffect(() => {
    return () => {
      const sid = sessionIdRef.current;
      if (sid !== null) void terminalClose(sid);
    };
  }, []);

  return (
    <TerminalView
      onReady={onReady}
      onData={onData}
      onResize={onResize}
      onCommand={onCommand}
      className="wb-terminal"
    />
  );
}
