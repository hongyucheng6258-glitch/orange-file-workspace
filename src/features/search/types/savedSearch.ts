/**
 * 保存搜索 / 智能集合 — 前端类型定义
 */

/** 搜索筛选条件（与后端 SearchFilters 对应，JSON 序列化存储） */
export interface SearchFilters {
  /** 资源类型：file/folder/page/project */
  kinds?: string[];
  /** 仅收藏 */
  favorite_only?: boolean;
  /** 标签 ID 列表 */
  tags?: string[];
  /** 扩展名列表（不含点） */
  extensions?: string[];
  /** 日期范围（Unix 时间戳） */
  date_from?: number;
  date_to?: number;
}

/** 保存搜索实体 */
export interface SavedSearch {
  id: string;
  name: string;
  query: string | null;
  filters_json: string;
  color: string | null;
  icon: string | null;
  is_pinned: boolean;
  display_order: number;
  created_at: number;
  updated_at: number;
  last_executed_at: number | null;
}

/** 搜索结果条目（与后端 SearchHit 对应） */
export interface SearchHit {
  id: string;
  kind: string;
  name: string;
  parent_id: string | null;
  is_favorite: boolean;
  updated_at: number;
  path: string | null;
  source_type: string | null;
}

/** 创建保存搜索的参数 */
export interface CreateSavedSearchParams {
  name: string;
  query?: string | null;
  filters_json?: string;
  color?: string | null;
  icon?: string | null;
}

/** 更新保存搜索的参数 */
export interface UpdateSavedSearchParams {
  name?: string;
  query?: string | null;
  filters_json?: string;
  color?: string | null;
  icon?: string | null;
}
