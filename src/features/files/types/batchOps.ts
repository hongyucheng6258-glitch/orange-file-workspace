/**
 * 批量操作类型 — 重复检测 / 批量重命名 / 操作撤销
 */

/** 重复文件条目 */
export interface DuplicateEntry {
  resource_id: string;
  name: string;
  path: string;
  size_bytes: number;
  source_type: string;
}

/** 重复文件分组 */
export interface DuplicateGroup {
  content_hash: string;
  size_bytes: number;
  entries: DuplicateEntry[];
}

/** 批量重命名预览条目 */
export interface RenameItem {
  resource_id: string;
  old_name: string;
  new_name: string;
  old_path: string;
  new_path: string;
  /** ok / conflict / invalid / missing */
  status: string;
  error: string | null;
}

/** 操作历史记录 */
export interface OperationHistory {
  id: string;
  operation_type: string;
  description: string | null;
  before_state: string;
  after_state: string;
  affected_count: number;
  created_at: number;
  undone_at: number | null;
}
