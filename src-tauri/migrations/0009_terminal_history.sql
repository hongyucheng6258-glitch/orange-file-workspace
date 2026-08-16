-- 终端命令历史：按 shell 持久化用户执行过的命令，供方向键/历史面板恢复。
CREATE TABLE IF NOT EXISTS terminal_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    shell TEXT NOT NULL,
    command TEXT NOT NULL,
    cwd TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_terminal_history_shell_time
    ON terminal_history (shell, created_at DESC);
