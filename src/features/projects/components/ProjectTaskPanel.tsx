/**
 * 项目任务面板 — 显示在项目页面侧栏
 *
 * 功能：
 * - 任务列表（按状态分组：进行中 / 待办 / 已完成）
 * - 创建任务（输入框 + 回车）
 * - 切换状态（点击复选框）
 * - 编辑标题（双击）
 * - 删除任务
 * - 优先级标记（颜色点）
 * - 关联资源管理（展开任务详情）
 */

import { useEffect, useState } from "react";
import {
  CheckCircle2,
  Circle,
  Plus,
  Trash2,
  ChevronDown,
  ChevronRight,
  Flag,
  Link2,
  X,
  Folder,
  File as FileIcon,
} from "lucide-react";
import { useProjectTaskStore } from "../stores/projectTaskStore";
import { call } from "../../../lib/tauri";
import type { Resource } from "../../../lib/types";
import type { TaskStatus, TaskPriority } from "../types/projectTask";
import { useEditorStore } from "../stores/editorStore";

const PRIORITY_COLORS: Record<TaskPriority, string> = {
  low: "var(--text-tertiary)",
  medium: "var(--primary)",
  high: "#e05252",
};

/** 优先级按钮 */
function PriorityDot({
  priority,
  onChange,
}: {
  priority: TaskPriority;
  onChange: (p: TaskPriority) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="ptp-priority-wrapper" onBlur={() => setOpen(false)}>
      <button
        className="ptp-priority-dot"
        style={{ background: PRIORITY_COLORS[priority] }}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
        title={`优先级：${priority}`}
      />
      {open && (
        <div className="ptp-priority-menu">
          {(["high", "medium", "low"] as TaskPriority[]).map((p) => (
            <button
              key={p}
              className="ptp-priority-option"
              onClick={(e) => {
                e.stopPropagation();
                onChange(p);
                setOpen(false);
              }}
            >
              <span
                className="ptp-priority-dot"
                style={{ background: PRIORITY_COLORS[p] }}
              />
              {p === "high" ? "高" : p === "medium" ? "中" : "低"}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** 单个任务行 */
function TaskRow({
  taskId,
  onSelect,
  isSelected,
}: {
  taskId: string;
  onSelect: (id: string | null) => void;
  isSelected: boolean;
}) {
  const task = useProjectTaskStore((s) =>
    s.tasks.find((t) => t.id === taskId),
  );
  const patchTask = useProjectTaskStore((s) => s.patchTask);
  const removeTask = useProjectTaskStore((s) => s.removeTask);
  const links = useProjectTaskStore((s) =>
    isSelected ? s.taskLinks : [],
  );
  const addLink = useProjectTaskStore((s) => s.addLink);
  const removeLink = useProjectTaskStore((s) => s.removeLink);
  const openFile = useEditorStore((s) => s.open);
  const [editing, setEditing] = useState(false);
  const [editTitle, setEditTitle] = useState(task?.title ?? "");
  const [expanded, setExpanded] = useState(false);
  const [linkedResources, setLinkedResources] = useState<Resource[]>([]);
  const [showLinkInput, setShowLinkInput] = useState(false);
  const [linkPath, setLinkPath] = useState("");

  // 加载关联资源详情
  useEffect(() => {
    if (expanded && isSelected && links.length > 0) {
      void Promise.all(
        links.map((l) =>
          call<{ resource: Resource; locations: unknown[] }>("get_resource", { id: l.resource_id })
            .then((r) => r.resource)
            .catch(() => null),
        ),
      ).then((results) => {
        setLinkedResources(results.filter((r): r is Resource => r !== null));
      });
    }
  }, [expanded, isSelected, links]);

  if (!task) return null;
  const done = task.status === "done";

  const handleToggleStatus = () => {
    const next: TaskStatus = done ? "todo" : "done";
    void patchTask(task.id, { status: next });
  };

  const handleSaveTitle = () => {
    const t = editTitle.trim();
    if (t && t !== task.title) {
      void patchTask(task.id, { title: t });
    }
    setEditing(false);
  };

  const handleAddLink = async () => {
    const path = linkPath.trim();
    if (!path) return;
    // 通过路径查找资源
    try {
      const res = await call<Resource | null>("find_resource_by_path", { path });
      if (res) {
        await addLink(task.id, res.id, "reference");
      } else {
        // 资源不存在，提示
      }
    } catch {
      // 忽略
    }
    setLinkPath("");
    setShowLinkInput(false);
  };

  return (
    <div className={`ptp-task-row ${isSelected ? "selected" : ""}`}>
      <div className="ptp-task-main">
        <button
          className="ptp-task-checkbox"
          onClick={handleToggleStatus}
          title={done ? "标记为待办" : "标记为完成"}
        >
          {done ? <CheckCircle2 size={15} color="var(--primary)" /> : <Circle size={15} />}
        </button>
        <PriorityDot
          priority={task.priority}
          onChange={(p) => void patchTask(task.id, { priority: p })}
        />
        {editing ? (
          <input
            className="ptp-task-edit-input"
            value={editTitle}
            autoFocus
            onChange={(e) => setEditTitle(e.target.value)}
            onBlur={handleSaveTitle}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSaveTitle();
              if (e.key === "Escape") {
                setEditTitle(task.title);
                setEditing(false);
              }
            }}
          />
        ) : (
          <span
            className={`ptp-task-title ${done ? "done" : ""}`}
            onDoubleClick={() => {
              setEditTitle(task.title);
              setEditing(true);
            }}
            onClick={() => {
              onSelect(isSelected ? null : task.id);
              setExpanded(isSelected ? !expanded : true);
            }}
          >
            {task.title}
          </span>
        )}
        {links.length > 0 && (
          <span className="ptp-task-badge" title={`${links.length} 个关联资源`}>
            <Link2 size={11} />
            {links.length}
          </span>
        )}
        <button
          className="ptp-task-expand"
          onClick={(e) => {
            e.stopPropagation();
            if (!isSelected) {
              onSelect(task.id);
              setExpanded(true);
            } else {
              setExpanded((v) => !v);
            }
          }}
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </button>
        <button
          className="ptp-task-delete"
          onClick={(e) => {
            e.stopPropagation();
            void removeTask(task.id);
          }}
          title="删除任务"
        >
          <Trash2 size={12} />
        </button>
      </div>
      {expanded && isSelected && (
        <div className="ptp-task-detail">
          {task.description && (
            <div className="ptp-task-desc">{task.description}</div>
          )}
          {/* 关联资源列表 */}
          <div className="ptp-links-section">
            <div className="ptp-links-header">
              <span>关联资源</span>
              <button
                className="ptp-link-add-btn"
                onClick={() => setShowLinkInput((v) => !v)}
              >
                <Plus size={11} /> 添加
              </button>
            </div>
            {showLinkInput && (
              <div className="ptp-link-input-row">
                <input
                  className="ptp-link-input"
                  placeholder="输入文件路径..."
                  value={linkPath}
                  onChange={(e) => setLinkPath(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleAddLink();
                  }}
                />
                <button className="ptp-link-confirm" onClick={handleAddLink}>
                  确定
                </button>
              </div>
            )}
            <div className="ptp-links-list">
              {linkedResources.length === 0 && links.length === 0 && (
                <div className="ptp-links-empty">暂无关联资源</div>
              )}
              {linkedResources.map((res) => (
                <div key={res.id} className="ptp-link-item">
                  {res.kind === "folder" ? (
                    <Folder size={12} color="var(--folder)" />
                  ) : (
                    <FileIcon size={12} color="var(--text-tertiary)" />
                  )}
                  <span
                    className="ptp-link-name"
                    onClick={() => {
                      if (res.kind !== "folder") {
                        void openFile(res.id);
                      }
                    }}
                    title={res.name}
                  >
                    {res.name}
                  </span>
                  <button
                    className="ptp-link-remove"
                    onClick={() => void removeLink(task.id, res.id)}
                  >
                    <X size={11} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/** 任务面板主组件 */
export function ProjectTaskPanel({ projectId }: { projectId: string }) {
  const {
    tasks,
    loading,
    selectedTaskId,
    setActiveProject,
    addTask,
    selectTask,
  } = useProjectTaskStore();
  const [input, setInput] = useState("");
  const [filter, setFilter] = useState<TaskStatus | "all">("all");

  // 初始化：设置活跃项目
  useEffect(() => {
    setActiveProject(projectId);
  }, [projectId, setActiveProject]);

  const filteredTasks = filter === "all"
    ? tasks
    : tasks.filter((t) => t.status === filter);

  // 按状态分组统计
  const counts = {
    todo: tasks.filter((t) => t.status === "todo").length,
    in_progress: tasks.filter((t) => t.status === "in_progress").length,
    done: tasks.filter((t) => t.status === "done").length,
  };

  const handleCreate = async () => {
    const title = input.trim();
    if (!title) return;
    await addTask(title);
    setInput("");
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") void handleCreate();
  };

  return (
    <div className="project-task-panel">
      <div className="ptp-header">
        <Flag size={13} color="var(--primary)" />
        <span>任务</span>
        <span className="ptp-count">{tasks.length}</span>
      </div>

      {/* 创建任务输入 */}
      <div className="ptp-input-row">
        <input
          className="ptp-input"
          placeholder="添加任务..."
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <button className="ptp-add-btn" onClick={handleCreate} disabled={!input.trim()}>
          <Plus size={13} />
        </button>
      </div>

      {/* 筛选标签 */}
      <div className="ptp-filters">
        <button
          className={`ptp-filter ${filter === "all" ? "active" : ""}`}
          onClick={() => setFilter("all")}
        >
          全部 ({tasks.length})
        </button>
        <button
          className={`ptp-filter ${filter === "todo" ? "active" : ""}`}
          onClick={() => setFilter("todo")}
        >
          待办 ({counts.todo})
        </button>
        <button
          className={`ptp-filter ${filter === "in_progress" ? "active" : ""}`}
          onClick={() => setFilter("in_progress")}
        >
          进行中 ({counts.in_progress})
        </button>
        <button
          className={`ptp-filter ${filter === "done" ? "active" : ""}`}
          onClick={() => setFilter("done")}
        >
          已完成 ({counts.done})
        </button>
      </div>

      {/* 任务列表 */}
      <div className="ptp-list">
        {loading && <div className="ptp-loading">加载中...</div>}
        {!loading && filteredTasks.length === 0 && (
          <div className="ptp-empty">
            {tasks.length === 0 ? "暂无任务，添加一个开始管理" : "无匹配任务"}
          </div>
        )}
        {filteredTasks.map((t) => (
          <TaskRow
            key={t.id}
            taskId={t.id}
            onSelect={selectTask}
            isSelected={selectedTaskId === t.id}
          />
        ))}
      </div>
    </div>
  );
}
