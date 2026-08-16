import { create } from "zustand";
import type { Resource } from "../../../lib/types";
import { call } from "../../../lib/tauri";

export interface OpenFile {
  resource: Resource;
  content: string;
  path: string;
}

interface EditorState {
  openFile: OpenFile | null;
  content: string;
  dirty: boolean;
  saving: boolean;
  conflict: { message: string; current_size: number } | null;
  openError: string | null;

  open: (resourceId: string) => Promise<void>;
  setContent: (content: string) => void;
  save: () => Promise<"saved" | "conflict">;
  forceSave: () => Promise<void>;
  close: () => Promise<void>;
  clearError: () => void;
}

export const useEditorStore = create<EditorState>((set, get) => ({
  openFile: null,
  content: "",
  dirty: false,
  saving: false,
  conflict: null,
  openError: null,

  open: async (resourceId) => {
    set({ openError: null });
    try {
      const data = await call<OpenFile & { session: unknown }>("open_file", { resourceId });
      set({
        openFile: { resource: data.resource, content: data.content, path: data.path },
        content: data.content,
        dirty: false,
        conflict: null,
      });
    } catch (e) {
      const err = e as { code?: string; message?: string };
      if (err.code === "file_too_large") {
        set({ openError: err.message ?? "文件过大，仅支持预览" });
        return;
      }
      throw e;
    }
  },

  clearError: () => set({ openError: null }),

  setContent: (content) => {
    const { openFile } = get();
    set({ content, dirty: content !== (openFile?.content ?? "") });
  },

  save: async () => {
    const { openFile, content, dirty } = get();
    if (!openFile || !dirty) return "saved";
    set({ saving: true });
    try {
      const res = await call<{ status: "saved" | "conflict"; message?: string; current_size?: number }>(
        "save_file",
        { resourceId: openFile.resource.id, content },
      );
      if (res.status === "conflict") {
        set({
          conflict: { message: res.message ?? "文件已被外部修改", current_size: res.current_size ?? 0 },
        });
        return "conflict";
      }
      set({ dirty: false, openFile: { ...openFile, content } });
      return "saved";
    } finally {
      set({ saving: false });
    }
  },

  forceSave: async () => {
    const { openFile, content } = get();
    if (!openFile) return;
    set({ saving: true });
    try {
      await call("save_file_force", { resourceId: openFile.resource.id, content });
      set({
        conflict: null,
        dirty: false,
        openFile: { ...openFile, content },
      });
    } finally {
      set({ saving: false });
    }
  },

  close: async () => {
    const { openFile } = get();
    if (openFile) {
      try {
        await call("discard_session", { resourceId: openFile.resource.id });
      } catch {
        // 忽略
      }
    }
    set({ openFile: null, content: "", dirty: false, conflict: null });
  },
}));
