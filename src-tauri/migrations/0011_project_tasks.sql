-- Phase 2.3: 项目任务与文件、页面关联
-- 用户在项目内创建的工作项（todo），可关联文件和页面资源

CREATE TABLE project_tasks (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'todo'
        CHECK(status IN ('todo', 'in_progress', 'done', 'cancelled')),
    priority TEXT NOT NULL DEFAULT 'medium'
        CHECK(priority IN ('low', 'medium', 'high')),
    sort_order INTEGER NOT NULL DEFAULT 0,
    due_date INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    completed_at INTEGER
);

CREATE INDEX idx_project_tasks_project ON project_tasks(project_id, sort_order);
CREATE INDEX idx_project_tasks_status ON project_tasks(project_id, status);

-- 任务与资源的关联（文件、页面）
CREATE TABLE project_task_links (
    task_id TEXT NOT NULL REFERENCES project_tasks(id) ON DELETE CASCADE,
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL DEFAULT 'reference'
        CHECK(link_type IN ('reference', 'input', 'output')),
    created_at INTEGER NOT NULL,
    PRIMARY KEY(task_id, resource_id)
);

CREATE INDEX idx_task_links_resource ON project_task_links(resource_id);
