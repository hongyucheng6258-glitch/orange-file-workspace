import { create } from "zustand";
import type { Resource, ResourceDetail } from "../../../lib/types";
import { call } from "../../../lib/tauri";

export type ViewMode = "list" | "grid";
export type SortKey = "name" | "kind" | "updated_at";

interface FileState {
  parentId: string | null;
  breadcrumbs: Resource[];
  resources: Resource[];
  loading: boolean;
  error: string | null;
  selection: Set<string>;
  viewMode: ViewMode;
  sortKey: SortKey;
  sortAsc: boolean;

  loadChildren: (parentId: string | null) => Promise<void>;
  setViewMode: (mode: ViewMode) => void;
  setSort: (key: SortKey) => void;
  toggleSelect: (id: string) => void;
  clearSelection: () => void;
  selectMany: (ids: string[]) => void;
  createFolder: (name: string) => Promise<void>;
  rename: (id: string, name: string) => Promise<void>;
  trash: (ids: string[]) => Promise<void>;
}

function sortResources(list: Resource[], key: SortKey, asc: boolean): Resource[] {
  const dir = asc ? 1 : -1;
  return [...list].sort((a, b) => {
    if (a.kind === "folder" && b.kind !== "folder") return -1;
    if (a.kind !== "folder" && b.kind === "folder") return 1;
    if (key === "name") return a.name.localeCompare(b.name, "zh") * dir;
    if (key === "kind") return a.kind.localeCompare(b.kind) * dir;
    return (a.updated_at - b.updated_at) * dir;
  });
}

export const useFileStore = create<FileState>((set, get) => ({
  parentId: null,
  breadcrumbs: [],
  resources: [],
  loading: false,
  error: null,
  selection: new Set(),
  viewMode: "list",
  sortKey: "name",
  sortAsc: true,

  loadChildren: async (parentId) => {
    set({ loading: true, error: null, parentId, selection: new Set() });
    try {
      const resources = await call<Resource[]>("list_children", {
        parentId: parentId ?? null,
      });
      const { sortKey, sortAsc } = get();
      set({ resources: sortResources(resources, sortKey, sortAsc), loading: false });
    } catch (e) {
      set({ loading: false, error: (e as Error).message });
    }
  },

  setViewMode: (viewMode) => set({ viewMode }),
  setSort: (sortKey) => {
    const { sortAsc, resources } = get();
    const asc = sortKey === get().sortKey ? !sortAsc : true;
    set({ sortKey, sortAsc: asc, resources: sortResources(resources, sortKey, asc) });
  },

  toggleSelect: (id) => {
    const next = new Set(get().selection);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    set({ selection: next });
  },

  clearSelection: () => set({ selection: new Set() }),

  selectMany: (ids) => set({ selection: new Set(ids) }),

  createFolder: async (name) => {
    await call<Resource>("create_folder", {
      parentId: get().parentId ?? null,
      name,
    });
    await get().loadChildren(get().parentId);
  },

  rename: async (id, name) => {
    await call<Resource>("rename_resource", { id, newName: name });
    await get().loadChildren(get().parentId);
  },

  trash: async (ids) => {
    await call<number>("trash_resources", { ids });
    await get().loadChildren(get().parentId);
  },
}));

/** 加载单个资源详情（供详情面板使用）。 */
export async function fetchResourceDetail(id: string): Promise<ResourceDetail | null> {
  try {
    return await call<ResourceDetail>("get_resource", { id });
  } catch {
    return null;
  }
}
