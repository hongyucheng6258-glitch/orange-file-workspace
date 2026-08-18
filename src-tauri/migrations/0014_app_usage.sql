-- 软件使用时间统计：按前台活跃时间累计。
-- apps: 已识别的应用（按规范化可执行路径归并）。
-- daily: 按日期 + 应用聚合的使用秒数。

CREATE TABLE IF NOT EXISTS app_usage_apps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    canonical_path TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name TEXT NOT NULL,
    process_name TEXT NOT NULL DEFAULT '',
    first_seen INTEGER NOT NULL,
    last_seen INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_app_usage_apps_path
    ON app_usage_apps (canonical_path COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS app_usage_daily (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    app_id INTEGER NOT NULL,
    date_ymd INTEGER NOT NULL,
    active_seconds INTEGER NOT NULL DEFAULT 0,
    last_active_at INTEGER NOT NULL DEFAULT 0,
    UNIQUE (app_id, date_ymd),
    FOREIGN KEY (app_id) REFERENCES app_usage_apps (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_usage_daily_date
    ON app_usage_daily (date_ymd DESC);
CREATE INDEX IF NOT EXISTS idx_app_usage_daily_app
    ON app_usage_daily (app_id, date_ymd DESC);
