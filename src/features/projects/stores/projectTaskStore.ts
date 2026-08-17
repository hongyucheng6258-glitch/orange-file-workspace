/**
 * 项目任务 Store — Zustand 状态管理
 *
 * 按项目 ID 缓存任务列表，支持 CRUD + 重排序 + 资源关联。
 */

import { create } from "zustand";
import {
  createProjectTask,
  listProjectTasks,
  updateProjectTask,
  deleteProjectTask,
  reorderProjectTasks,
  linkTaskResource,
  unlinkTaskResource,
  listTaskLinks,
} from "../api/projectTaskApi";
import type {
  ProjectTask,
  ProjectTaskLink,
  TaskStatus,
  TaskPriority,
  TaskLinkType,
} from "../types/projectTask";

interface ProjectTaskStore {
  /** 当前激活的项目 ID */
  activeProjectId: string | null;
  /** 当前项目的任务列表 */
  tasks: ProjectTask[];
  /** 当前选中任务 ID */
  selectedTaskId: string | null;
  /** 选中任务的资源关联列表 */
  taskLinks: ProjectTaskLink[];
  /** 加载状态 */
  loading: boolean;
  /** 错误信息 */
  error: string | null;

  /** 加载项目任务 */
  loadTasks: (projectId: string) => Promise<void>;
  /** 切换项目 */
  setActiveProject: (projectId: string | null) => void;
  /** 创建任务 */
  addTask: (
    title: string,
    description?: string,
    priority?: TaskPriority,
  ) => Promise<ProjectTask | null>;
  /** 更新任务 */
  patchTask: (
    id: string,
    params: {
      title?: string;
      description?: string | null;
      status?: TaskStatus;
      priority?: TaskPriority;
      due_date?: number | null;
    },
  ) => Promise<void>;
  /** 删除任务 */
  removeTask: (id: string) => Promise<void>;
  /** 重排序 */
  reorder: (taskIds: string[]) => Promise<void>;
  /** 选中任务 */
  selectTask: (id: string | null) => Promise<void>;
  /** 关联资源 */
  addLink: (
    taskId: string,
    resourceId: string,
    linkType?: TaskLinkType,
  ) => Promise<void>;
  /** 取消关联 */
  removeLink: (taskId: string, resourceId: string) => Promise<void>;
}

export const useProjectTaskStore = create<ProjectTaskStore>((set, get) => ({
  activeProjectId: null,
  tasks: [],
  selectedTaskId: null,
  taskLinks: [],
  loading: false,
  error: null,

  loadTasks: async (projectId) => {
    set({ loading: true, error: null, activeProjectId: projectId });
    try {
      const tasks = await listProjectTasks(projectId);
      set({ tasks, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  setActiveProject: (projectId) => {
    if (projectId === get().activeProjectId) return;
    set({
      activeProjectId: projectId,
      tasks: [],
      selectedTaskId: null,
      taskLinks: [],
    });
    if (projectId) {
      void get().loadTasks(projectId);
    }
  },

  addTask: async (title, description, priority) => {
    const projectId = get().activeProjectId;
    if (!projectId) return null;
    try {
      const task = await createProjectTask(projectId, title, description, priority);
      set({ tasks: [...get().tasks, task] });
      return task;
    } catch (e) {
      set({ error: String(e) });
      return null;
    }
  },

  patchTask: async (id, params) => {
    try {
      const updated = await updateProjectTask(id, params);
      if (updated) {
        set({
          tasks: get().tasks.map((t) => (t.id === id ? updated : t)),
        });
      }
    } catch (e) {
      set({ error: String(e) });
    }
  },

  removeTask: async (id) => {
    try {
      await deleteProjectTask(id);
      set({
        tasks: get().tasks.filter((t) => t.id !== id),
        selectedTaskId: get().selectedTaskId === id ? null : get().selectedTaskId,
        taskLinks: get().selectedTaskId === id ? [] : get().taskLinks,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  reorder: async (taskIds) => {
    try {
      await reorderProjectTasks(taskIds);
      // 按传入顺序重排本地 tasks
      const map = new Map(get().tasks.map((t) => [t.id, t]));
      const reordered: ProjectTask[] = [];
      for (const id of taskIds) {
        const t = map.get(id);
        if (t) reordered.push({ ...t, sort_order: reordered.length });
      }
      // 追加不在 taskIds 中的任务
      for (const t of get().tasks) {
        if (!taskIds.includes(t.id)) reordered.push(t);
      }
      set({ tasks: reordered });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  selectTask: async (id) => {
    set({ selectedTaskId: id, taskLinks: [] });
    if (id) {
      try {
        const links = await listTaskLinks(id);
        set({ taskLinks: links });
      } catch {
        // ignore
      }
    }
  },

  addLink: async (taskId, resourceId, linkType) => {
    try {
      await linkTaskResource(taskId, resourceId, linkType);
      const links = await listTaskLinks(taskId);
      set({ taskLinks: links });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  removeLink: async (taskId, resourceId) => {
    try {
      await unlinkTaskResource(taskId, resourceId);
      const links = await listTaskLinks(taskId);
      set({ taskLinks: links });
    } catch (e) {
      set({ error: String(e) });
    }
  },
}));
