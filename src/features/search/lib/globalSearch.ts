import { call } from "../../../lib/tauri";
import { listen } from "@tauri-apps/api/event";

export interface GlobalSearchHit {
  key: string;
  kind: "file" | "folder" | "app" | "page" | "project";
  name: string;
  path: string | null;
  source: "windows" | "local_index" | "app_index" | "nexus";
  matched_field: "name" | "path";
  modified_at: number | null;
  icon_source: string | null;
  is_offline: boolean;
  actions: string[];
  score: number;
}

export interface SearchBatch {
  search_id: number;
  hits: GlobalSearchHit[];
  sources: Record<string, { error?: string }>;
  index_incomplete: boolean;
}

export function startSearch(
  query: string,
  limit?: number,
): Promise<SearchBatch> {
  return call<SearchBatch>("start_global_search", { query, limit: limit ?? 100 });
}

export function cancelSearch(searchId: number): Promise<void> {
  return call<void>("cancel_global_search", { searchId });
}

export function openResult(key: string): Promise<void> {
  return call<void>("open_search_result", { key });
}

export function revealResult(key: string): Promise<void> {
  return call<void>("reveal_search_result", { key });
}

/** 订阅后台补批事件，返回取消订阅函数。 */
export function onSearchBatch(
  searchId: number,
  cb: (hits: GlobalSearchHit[]) => void,
): Promise<() => void> {
  return listen<{ search_id: number; hits: GlobalSearchHit[] }>(
    "global-search://batch",
    (e) => {
      if (e.payload.search_id === searchId) cb(e.payload.hits);
    },
  ).then((unlisten) => unlisten);
}
