-- 操作历史表 — 记录批量操作的前后状态，支持撤销
CREATE TABLE IF NOT EXISTS operation_history (
    id TEXT PRIMARY KEY,
    operation_type TEXT NOT NULL CHECK(operation_type IN (
        'batch_rename', 'batch_move', 'batch_delete', 'single_rename', 'single_move'
    )),
    description TEXT,
    -- 操作前的快照（JSON 数组：[{ resource_id, old_name, old_parent_id, old_path }]）
    before_state TEXT NOT NULL DEFAULT '[]',
    -- 操作后的快照（JSON 数组：[{ resource_id, new_name, new_parent_id, new_path }]）
    after_state TEXT NOT NULL DEFAULT '[]',
    -- 影响的资源数量
    affected_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    undone_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_operation_history_created
    ON operation_history(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_operation_history_undone
    ON operation_history(undone_at);
