import { create } from "zustand";
import type { JSONContent } from "@tiptap/core";
import type { Resource } from "../../../lib/types";
import { call } from "../../../lib/tauri";

/** TipTap 文档节点。 */
export type DocNode = JSONContent;

/** 空文档。 */
export const EMPTY_DOC: DocNode = { type: "doc", content: [] };

export interface PageDetail {
  resource: Resource;
  page: {
    resource_id: string;
    summary: string | null;
    content_version: number;
    save_state: string;
    content_json?: string | null;
  };
}

interface PageState {
  tree: Resource[];
  currentPageId: string | null;
  detail: PageDetail | null;
  document: DocNode;
  dirty: boolean;
  loading: boolean;
  saving: boolean;
  pendingTarget: string | null;

  loadTree: () => Promise<void>;
  openPage: (id: string) => Promise<"opened" | "confirm">;
  createPage: (name: string, parentId?: string | null) => Promise<void>;
  renamePage: (id: string, name: string) => Promise<void>;
  deletePage: (id: string) => Promise<void>;
  setDocument: (doc: DocNode) => void;
  save: () => Promise<void>;
  resolveOpen: (save: boolean) => Promise<void>;
  cancelOpen: () => void;
}

/** 解析页面内容 JSON，失败时返回空文档。 */
function parseDocument(raw: string | null | undefined): DocNode {
  if (!raw) return EMPTY_DOC;
  try {
    const parsed = JSON.parse(raw) as DocNode;
    if (parsed && parsed.type === "doc") return parsed;
    return EMPTY_DOC;
  } catch {
    return EMPTY_DOC;
  }
}

/** 递归提取文档纯文本，用于摘要。 */
export function extractPlainText(node: DocNode | null | undefined): string {
  if (!node) return "";
  if (typeof node.text === "string") return node.text;
  if (Array.isArray(node.content)) {
    return node.content.map(extractPlainText).join("\n");
  }
  return "";
}

export const usePageStore = create<PageState>((set, get) => ({
  tree: [],
  currentPageId: null,
  detail: null,
  document: EMPTY_DOC,
  dirty: false,
  loading: false,
  saving: false,
  pendingTarget: null,

  loadTree: async () => {
    const tree = await call<Resource[]>("list_pages", { parentId: null });
    set({ tree });
  },

  openPage: async (id) => {
    const { dirty, currentPageId } = get();
    if (dirty && currentPageId !== id) {
      set({ pendingTarget: id });
      return "confirm";
    }
    set({ loading: true, currentPageId: id });
    try {
      const detail = await call<PageDetail>("get_page", { resourceId: id });
      set({
        detail,
        document: parseDocument(detail.page.content_json),
        dirty: false,
        loading: false,
        pendingTarget: null,
      });
      return "opened";
    } catch (e) {
      set({ loading: false });
      throw e;
    }
  },

  resolveOpen: async (save) => {
    const { pendingTarget } = get();
    if (!pendingTarget) return;
    if (save) {
      await get().save();
      // 保存失败（异常）会中断，保持当前页面
    } else {
      // 放弃：清除 dirty，避免 openPage 再次触发确认
      set({ dirty: false });
    }
    await get().openPage(pendingTarget);
  },

  cancelOpen: () => set({ pendingTarget: null }),

  createPage: async (name, parentId = null) => {
    await call("create_page", { name, parentId });
    await get().loadTree();
  },

  renamePage: async (id, name) => {
    await call("rename_page", { resourceId: id, newName: name });
    await get().loadTree();
    if (get().currentPageId === id) {
      await get().openPage(id);
    }
  },

  deletePage: async (id) => {
    // 删除当前 dirty 页面时先保存，避免未保存修改随删除丢失
    if (get().dirty && get().currentPageId === id) {
      await get().save();
    }
    await call("delete_page", { resourceId: id });
    if (get().currentPageId === id) {
      set({ detail: null, document: EMPTY_DOC, currentPageId: null, dirty: false, pendingTarget: null });
    }
    await get().loadTree();
  },

  setDocument: (document) => set({ document, dirty: true }),

  save: async () => {
    const { currentPageId, document, dirty } = get();
    if (!currentPageId || !dirty) return;
    set({ saving: true });
    try {
      await call("save_page_document", {
        resourceId: currentPageId,
        contentJson: JSON.stringify(document),
        plainText: extractPlainText(document),
      });
      set({ dirty: false });
    } finally {
      set({ saving: false });
    }
  },
}));
