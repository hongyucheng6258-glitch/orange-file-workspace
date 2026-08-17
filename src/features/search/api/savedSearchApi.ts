/**
 * 保存搜索 API — 封装 Tauri 命令调用
 */

import { call } from "../../../lib/tauri";
import type {
  SavedSearch,
  SearchHit,
  SearchFilters,
  CreateSavedSearchParams,
  UpdateSavedSearchParams,
} from "../types/savedSearch";

export async function createSavedSearch(
  params: CreateSavedSearchParams,
): Promise<SavedSearch> {
  return call<SavedSearch>("create_saved_search", {
    name: params.name,
    query: params.query ?? null,
    filters_json: params.filters_json ?? "{}",
    color: params.color ?? null,
    icon: params.icon ?? null,
  });
}

export async function listSavedSearches(): Promise<SavedSearch[]> {
  return call<SavedSearch[]>("list_saved_searches", {});
}

export async function listPinnedSavedSearches(): Promise<SavedSearch[]> {
  return call<SavedSearch[]>("list_pinned_saved_searches", {});
}

export async function updateSavedSearch(
  id: string,
  params: UpdateSavedSearchParams,
): Promise<SavedSearch | null> {
  return call<SavedSearch | null>("update_saved_search", {
    id,
    name: params.name,
    query: params.query,
    filters_json: params.filters_json,
    color: params.color,
    icon: params.icon,
  });
}

export async function deleteSavedSearch(id: string): Promise<void> {
  await call<void>("delete_saved_search", { id });
}

export async function toggleSavedSearchPinned(
  id: string,
  pinned: boolean,
): Promise<SavedSearch | null> {
  return call<SavedSearch | null>("toggle_saved_search_pinned", { id, pinned });
}

export async function reorderPinnedSavedSearches(ids: string[]): Promise<void> {
  await call<void>("reorder_pinned_saved_searches", { ids });
}

export async function executeSavedSearch(
  id: string,
  limit?: number,
): Promise<SearchHit[]> {
  return call<SearchHit[]>("execute_saved_search", { id, limit: limit ?? 200 });
}

/** 辅助：将 SearchFilters 序列化为 JSON 字符串 */
export function serializeFilters(filters: SearchFilters): string {
  return JSON.stringify(filters);
}

/** 辅助：从 JSON 字符串反序列化 SearchFilters */
export function deserializeFilters(json: string): SearchFilters {
  try {
    return JSON.parse(json) as SearchFilters;
  } catch {
    return {};
  }
}
