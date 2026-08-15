-- 0002_editor_sessions_unique.sql
-- editor_sessions.resource_id 需要 UNIQUE 才能支持 UPSERT (ON CONFLICT(resource_id))
CREATE UNIQUE INDEX idx_editor_sessions_resource_unique
ON editor_sessions(resource_id);
