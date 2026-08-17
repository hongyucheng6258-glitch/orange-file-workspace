/**
 * 项目任务类型定义 — 与 Rust 端 ProjectTask / ProjectTaskLink 对应
 */

export type TaskStatus = "todo" | "in_progress" | "done" | "cancelled";
export type TaskPriority = "low" | "medium" | "high";
export type TaskLinkType = "reference" | "input" | "output";

/** 项目任务 */
export interface ProjectTask {
  id: string;
  project_id: string;
  title: string;
  description: string | null;
  status: TaskStatus;
  priority: TaskPriority;
  sort_order: number;
  due_date: number | null;
  created_at: number;
  updated_at: number;
  completed_at: number | null;
}

/** 任务-资源关联 */
export interface ProjectTaskLink {
  task_id: string;
  resource_id: string;
  link_type: TaskLinkType;
  created_at: number;
}

/** 更新任务参数 */
export interface UpdateTaskParams {
  title?: string;
  description?: string | null;
  status?: TaskStatus;
  priority?: TaskPriority;
  due_date?: number | null;
}
