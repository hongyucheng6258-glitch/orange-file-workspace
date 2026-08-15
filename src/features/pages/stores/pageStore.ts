import { create } from "zustand";
import type { Resource } from "../../../lib/types";
import { call } from "../../../lib/tauri";

export interface PageBlock {
  id: string;
  page_id: string;
  parent_block_id: string | null;
  block_type: string;
  block_order: number;
  content_json: string;
  plain_text: string | null;
  created_at: number;
  updated_at: number;
}

export interface PageDetail {
  resource: Resource;
  page: {
    resource_id: string;
    summary: string | null;
    content_version: number;
    save_state: string;
  };
  blocks: PageBlock[];
}

interface PageState {
  tree: Resource[];
  currentPageId: string | null;
  detail: PageDetail | null;
  blocks: PageBlock[];
  dirty: boolean;
  loading: boolean;

  loadTree: () => Promise<void>;
  openPage: (id: string) => Promise<void>;
  createPage: (name: string, parentId?: string | null) => Promise<void>;
  renamePage: (id: string, name: string) => Promise<void>;
  setBlocks: (blocks: PageBlock[]) => void;
  save: () => Promise<void>;
}

export const usePageStore = create<PageState>((set, get) => ({
  tree: [],
  currentPageId: null,
  detail: null,
  blocks: [],
  dirty: false,
  loading: false,

  loadTree: async () => {
    const tree = await call<Resource[]>("list_pages", { parentId: null });
    set({ tree });
  },

  openPage: async (id) => {
    set({ loading: true, currentPageId: id });
    try {
      const detail = await call<PageDetail>("get_page", { resourceId: id });
      set({ detail, blocks: detail.blocks, dirty: false, loading: false });
    } catch (e) {
      set({ loading: false });
      throw e;
    }
  },

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

  setBlocks: (blocks) => set({ blocks, dirty: true }),

  save: async () => {
    const { currentPageId, blocks, dirty } = get();
    if (!currentPageId || !dirty) return;
    await call("save_page_blocks", {
      resourceId: currentPageId,
      blocks: blocks.map((b) => ({
        parent_block_id: b.parent_block_id,
        block_type: b.block_type,
        content_json: b.content_json,
        plain_text: b.plain_text,
      })),
    });
    set({ dirty: false });
    await get().loadTree();
  },
}));

/** 解析块的文本内容。 */
export function blockText(b: PageBlock): string {
  try {
    const data = JSON.parse(b.content_json) as { text?: string };
    return data.text ?? "";
  } catch {
    return "";
  }
}
