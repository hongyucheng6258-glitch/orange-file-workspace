import { useMemo } from "react";
import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { call } from "../../../lib/tauri";

export interface TaskRecord {
  id: string;
  task_type: string;
  status: "queued" | "running" | "paused" | "completed" | "failed" | "cancelled";
  title: string;
  total_count: number | null;
  completed_count: number;
  failed_count: number;
  payload_json: string | null;
  error_json: string | null;
  created_at: number;
  started_at: number | null;
  finished_at: number | null;
  updated_at: number;
}

interface TaskProgressEvent {
  task_id: string;
  status: string;
  completed: number;
  failed: number;
  total: number;
}

interface TaskState {
  tasks: TaskRecord[];
  addTask: (task: TaskRecord) => void;
  refresh: () => Promise<void>;
  cancel: (id: string) => Promise<void>;
  startListening: () => Promise<() => void>;
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasks: [],

  addTask: (task) => {
    set((state) => ({
      tasks: [task, ...state.tasks.filter((item) => item.id !== task.id)],
    }));
  },

  refresh: async () => {
    try {
      const tasks = await call<TaskRecord[]>("list_tasks", {});
      set({ tasks });
    } catch {
      // 忽略：应用未就绪
    }
  },

  cancel: async (id) => {
    await call<void>("cancel_task", { taskId: id });
    await get().refresh();
  },

  startListening: async () => {
    await get().refresh();
    const unlisten = await listen<TaskProgressEvent>("task-progress", (e) => {
      const p = e.payload;
      set((state) => ({
        tasks: state.tasks.map((t) =>
          t.id === p.task_id
            ? {
                ...t,
                status: p.status as TaskRecord["status"],
                completed_count: p.completed,
                failed_count: p.failed,
                total_count: p.total,
              }
            : t,
        ),
      }));
      // 完成/失败时刷新列表以同步最终状态
      if (p.status === "completed" || p.status === "failed" || p.status === "cancelled") {
        get().refresh();
      }
    });
    return unlisten;
  },
}));

/** 活跃任务（进行中），用于底部任务条。 */
export function useActiveTasks() {
  const tasks = useTaskStore((s) => s.tasks);
  return useMemo(
    () => tasks.filter((t) => t.status === "queued" || t.status === "running" || t.status === "paused"),
    [tasks],
  );
}
