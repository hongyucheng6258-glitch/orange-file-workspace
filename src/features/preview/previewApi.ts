import { convertFileSrc } from "@tauri-apps/api/core";

import { call } from "../../lib/tauri";

export interface CsvPreviewData {
  headers: string[];
  rows: string[][];
  total_rows: number;
  truncated: boolean;
}

export interface ArchiveEntry {
  path: string;
  size: number;
  is_dir: boolean;
  modified: string | null;
}

export interface ArchiveInfo {
  format: string;
  entry_count: number;
  total_uncompressed: number;
  entries: ArchiveEntry[];
}

/** 获取资源的预览类型。 */
export function getPreviewKind(resourceId: string): Promise<string | null> {
  return call<string | null>("get_preview_kind", { resourceId });
}

/** 获取资源的物理文件路径。 */
export function getResourcePath(resourceId: string): Promise<string | null> {
  return call<string | null>("get_resource_path", { resourceId });
}

/** 获取 CSV 预览数据。 */
export function getCsvPreview(resourceId: string): Promise<CsvPreviewData> {
  return call<CsvPreviewData>("get_csv_preview", { resourceId });
}

/** 列出压缩包内容。 */
export function getArchiveListing(
  resourceId: string,
  maxEntries?: number,
): Promise<ArchiveInfo> {
  return call<ArchiveInfo>("get_archive_listing", {
    resourceId,
    maxEntries: maxEntries ?? 1000,
  });
}

/** 将本地文件路径转换为 asset 协议 URL（用于 img/video/iframe src）。 */
export function assetUrl(filePath: string): string {
  return convertFileSrc(filePath);
}
