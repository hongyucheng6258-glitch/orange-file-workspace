-- 项目运行历史：已退出运行记录落库，日志不持久化。
-- 保留策略（每项目 20 条 / 7 天）由应用层 prune 执行，此处仅建表与索引。
CREATE TABLE IF NOT EXISTS project_run_history (
    run_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    project_key TEXT NOT NULL,
    executable TEXT NOT NULL,
    args_json TEXT NOT NULL,
    cwd TEXT NOT NULL,
    env_json TEXT NOT NULL DEFAULT '{}',
    expected_port INTEGER,
    preview_scheme TEXT NOT NULL DEFAULT 'http',
    state TEXT NOT NULL,
    exit_code INTEGER,
    error_code TEXT,
    error_message TEXT,
    stop_reason TEXT,
    started_at INTEGER NOT NULL,
    exited_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_project_run_history_project
    ON project_run_history(project_key, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_project_run_history_exited
    ON project_run_history(exited_at);
