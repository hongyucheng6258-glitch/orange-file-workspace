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
  pendingOpenId: string | null;
  pendingClose: boolean;

  open: (resourceId: string) => Promise<"opened" | "confirm">;
  setContent: (content: string) => void;
  save: () => Promise<"saved" | "conflict">;
  forceSave: () => Promise<void>;
  close: () => Promise<"closed" | "confirm">;
  resolveOpen: (save: boolean) => Promise<void>;
  resolveClose: (save: boolean) => Promise<void>;
  cancelPending: () => void;
  clearError: () => void;
  discardSession: () => Promise<void>;
}

export const useEditorStore = create<EditorState>((set, get) => ({
  openFile: null,
  content: "",
  dirty: false,
  saving: false,
  conflict: null,
  openError: null,
  pendingOpenId: null,
  pendingClose: false,

  open: async (resourceId) => {
    const { dirty } = get();
    if (dirty) {
      set({ pendingOpenId: resourceId });
      return "confirm";
    }
    set({ openError: null });
    try {
      const data = await call<OpenFile & { session: unknown }>("open_file", { resourceId });
      set({
        openFile: { resource: data.resource, content: data.content, path: data.path },
        content: data.content,
        dirty: false,
        conflict: null,
        pendingOpenId: null,
      });
      return "opened";
    } catch (e) {
      const err = e as { code?: string; message?: string };
      if (err.code === "file_too_large") {
        set({ openError: err.message ?? "文件过大，仅支持预览" });
        return "opened";
      }
      throw e;
    }
  },

  resolveOpen: async (save) => {
    const { pendingOpenId } = get();
    if (!pendingOpenId) return;
    if (save) {
      const status = await get().save();
      if (status === "conflict") {
        set({ pendingOpenId: null });
        return;
      }
    } else {
      // 放弃：清除 dirty，避免 open 再次触发确认
      set({ dirty: false });
    }
    await get().open(pendingOpenId);
  },

  cancelPending: () => set({ pendingOpenId: null, pendingClose: false }),

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
    const { dirty, openFile } = get();
    if (dirty && openFile) {
      set({ pendingClose: true });
      return "confirm";
    }
    await get().discardSession();
    set({ openFile: null, content: "", dirty: false, conflict: null, pendingClose: false });
    return "closed";
  },

  resolveClose: async (save) => {
    if (save) {
      const status = await get().save();
      if (status === "conflict") {
        set({ pendingClose: false });
        return;
      }
    } else {
      // 放弃：清除 dirty，避免 close 再次触发确认
      set({ dirty: false });
    }
    await get().close();
  },

  discardSession: async () => {
    const { openFile } = get();
    if (openFile) {
      try {
        await call("discard_session", { resourceId: openFile.resource.id });
      } catch {
        // 忽略
      }
    }
  },
}));
