/**
 * 保存搜索 / 智能集合 — Zustand 状态管理
 */

import { create } from "zustand";
import type {
  SavedSearch,
  SearchHit,
  SearchFilters,
  CreateSavedSearchParams,
  UpdateSavedSearchParams,
} from "../types/savedSearch";
import {
  listSavedSearches,
  listPinnedSavedSearches,
  createSavedSearch as apiCreate,
  updateSavedSearch as apiUpdate,
  deleteSavedSearch as apiDelete,
  toggleSavedSearchPinned as apiTogglePin,
  reorderPinnedSavedSearches as apiReorder,
  executeSavedSearch as apiExecute,
  serializeFilters,
} from "../api/savedSearchApi";

interface SavedSearchState {
  /** 全部保存搜索列表 */
  searches: SavedSearch[];
  /** 仅已固定（智能集合）的列表 */
  pinned: SavedSearch[];
  /** 当前执行的搜索结果 */
  results: SearchHit[];
  /** 当前查看的搜索 ID */
  activeSearchId: string | null;
  loading: boolean;
  executing: boolean;
  error: string | null;

  loadAll: () => Promise<void>;
  loadPinned: () => Promise<void>;
  addSearch: (params: CreateSavedSearchParams) => Promise<SavedSearch>;
  patchSearch: (id: string, params: UpdateSavedSearchParams) => Promise<void>;
  removeSearch: (id: string) => Promise<void>;
  togglePin: (id: string, pinned: boolean) => Promise<void>;
  reorderPinned: (ids: string[]) => Promise<void>;
  runSearch: (id: string) => Promise<SearchHit[]>;
  selectSearch: (id: string | null) => void;
}

export const useSavedSearchStore = create<SavedSearchState>((set, get) => ({
  searches: [],
  pinned: [],
  results: [],
  activeSearchId: null,
  loading: false,
  executing: false,
  error: null,

  loadAll: async () => {
    set({ loading: true, error: null });
    try {
      const [searches, pinned] = await Promise.all([
        listSavedSearches(),
        listPinnedSavedSearches(),
      ]);
      set({ searches, pinned, loading: false });
    } catch (e) {
      set({ loading: false, error: (e as Error).message });
    }
  },

  loadPinned: async () => {
    try {
      const pinned = await listPinnedSavedSearches();
      set({ pinned });
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  addSearch: async (params) => {
    const search = await apiCreate({
      ...params,
      filters_json: params.filters_json ?? serializeFilters({} as SearchFilters),
    });
    set((s) => ({
      searches: [...s.searches, search].sort((a, b) => b.is_pinned ? 1 : 0 - (a.is_pinned ? 1 : 0)),
    }));
    if (search.is_pinned) {
      await get().loadPinned();
    }
    return search;
  },

  patchSearch: async (id, params) => {
    const updated = await apiUpdate(id, params);
    if (updated) {
      set((s) => ({
        searches: s.searches.map((ss) => (ss.id === id ? updated : ss)),
        pinned: s.pinned.map((ss) => (ss.id === id ? updated : ss)),
      }));
    }
  },

  removeSearch: async (id) => {
    await apiDelete(id);
    set((s) => ({
      searches: s.searches.filter((ss) => ss.id !== id),
      pinned: s.pinned.filter((ss) => ss.id !== id),
      activeSearchId: s.activeSearchId === id ? null : s.activeSearchId,
    }));
  },

  togglePin: async (id, pinned) => {
    await apiTogglePin(id, pinned);
    await get().loadAll();
  },

  reorderPinned: async (ids) => {
    await apiReorder(ids);
    await get().loadPinned();
  },

  runSearch: async (id) => {
    set({ executing: true, error: null, activeSearchId: id });
    try {
      const hits = await apiExecute(id);
      set({ results: hits, executing: false });
      return hits;
    } catch (e) {
      set({ executing: false, error: (e as Error).message });
      throw e;
    }
  },

  selectSearch: (id) => set({ activeSearchId: id }),
}));
