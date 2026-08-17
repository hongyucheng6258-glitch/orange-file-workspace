-- Phase 2.4: 保存搜索和智能集合
-- 用户保存的搜索条件，可作为智能集合固定在侧边栏

CREATE TABLE IF NOT EXISTS saved_searches (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    query TEXT,
    -- 查询定义 JSON: {kinds, favorite_only, tags, extensions, date_from, date_to}
    filters_json TEXT NOT NULL DEFAULT '{}',
    color TEXT,
    icon TEXT,
    is_pinned INTEGER NOT NULL DEFAULT 0,
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_executed_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_saved_searches_pinned
ON saved_searches(is_pinned, display_order);

CREATE INDEX IF NOT EXISTS idx_saved_searches_name
ON saved_searches(name);
