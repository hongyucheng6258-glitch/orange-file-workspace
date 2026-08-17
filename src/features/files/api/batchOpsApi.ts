/**
 * 批量操作 API — 重复检测 / 批量重命名 / 操作撤销
 */

import { call } from "../../../lib/tauri";
import type {
  DuplicateGroup,
  RenameItem,
  OperationHistory,
} from "../types/batchOps";

// ── 重复文件检测 ──

export async function findDuplicates(): Promise<DuplicateGroup[]> {
  return call<DuplicateGroup[]>("find_duplicates", {});
}

export async function getHashStats(): Promise<[number, number]> {
  return call<[number, number]>("get_hash_stats", {});
}

export async function hashResources(ids: string[]): Promise<number> {
  return call<number>("hash_resources", { ids });
}

/** 永久删除指定资源（磁盘文件 + 数据库记录），不可恢复。用于删除重复副本。 */
export async function deletePermanently(ids: string[]): Promise<number> {
  return call<number>("delete_permanently", { ids });
}

// ── 批量重命名 ──

export async function previewBatchRename(
  items: [string, string][],
): Promise<RenameItem[]> {
  return call<RenameItem[]>("preview_batch_rename", { items });
}

export async function executeBatchRename(
  items: RenameItem[],
  description?: string,
): Promise<OperationHistory> {
  return call<OperationHistory>("execute_batch_rename", {
    items,
    description: description ?? null,
  });
}

// ── 操作历史 / 撤销 ──

export async function listOperationHistory(
  limit?: number,
): Promise<OperationHistory[]> {
  return call<OperationHistory[]>("list_operation_history", {
    limit: limit ?? 20,
  });
}

export async function undoOperation(opId: string): Promise<void> {
  await call<void>("undo_operation", { op_id: opId });
}
