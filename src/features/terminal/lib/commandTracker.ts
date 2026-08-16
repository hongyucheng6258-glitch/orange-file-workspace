/**
 * 终端命令跟踪器：从按键流中还原"完整命令"，回车时提交。
 * 处理退格、Ctrl+C 清空、控制序列（方向键等）跳过；不精确跟踪方向键编辑，
 * 但覆盖常见输入场景（打字 → 退格 → 回车）。
 */

export interface CommandTracker {
  /** 接收一段按键数据（与 xterm onData 相同）。 */
  push: (data: string) => void;
  /** 当前累积的输入（不含已提交部分）。 */
  current: () => string;
  /** 重置缓冲（会话重启/清屏时调用）。 */
  reset: () => void;
}

/** 回车提交回调。 */
export type CommandSubmit = (command: string) => void;

export function createCommandTracker(onSubmit: CommandSubmit): CommandTracker {
  let buf = "";
  // 简单 CSI/OSC 序列跳过状态：\x1b 后到字母/~ 为止视为控制序列。
  let inEscape = false;

  const push = (data: string) => {
    for (const ch of data) {
      if (inEscape) {
        // ESC 序列结束符：字母或 ~ 或 BEL。
        if (/[a-zA-Z~]|\x07/.test(ch)) inEscape = false;
        continue;
      }
      if (ch === "\x1b") {
        inEscape = true;
        continue;
      }
      if (ch === "\r") {
        const cmd = buf.trim();
        buf = "";
        if (cmd) onSubmit(cmd);
        continue;
      }
      if (ch === "\x7f") {
        buf = buf.slice(0, -1);
        continue;
      }
      if (ch === "\x03") {
        // Ctrl+C 取消当前行。
        buf = "";
        continue;
      }
      // 其他控制字符（\n、\t 等）不加入命令。
      if (ch >= " ") {
        buf += ch;
      }
    }
  };

  return {
    push,
    current: () => buf,
    reset: () => {
      buf = "";
      inEscape = false;
    },
  };
}
