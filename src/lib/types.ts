/** 与 Rust 端模型对应的类型定义。 */

export type ResourceKind = "file" | "folder" | "page" | "project";
export type SourceType = "managed" | "external";

export interface Resource {
  id: string;
  kind: ResourceKind;
  name: string;
  parent_id: string | null;
  is_favorite: boolean;
  is_deleted: boolean;
  created_at: number;
  updated_at: number;
  deleted_at: number | null;
}

export interface ResourceLocation {
  id: string;
  resource_id: string;
  source_type: SourceType;
  path: string;
  canonical_path: string | null;
  file_size: number | null;
  modified_at: number | null;
  created_at: number;
  last_verified_at: number | null;
  content_hash: string | null;
  hash_algorithm: string | null;
  is_available: boolean;
}

export interface FileMetadata {
  resource_id: string;
  extension: string | null;
  mime_type: string | null;
  size_bytes: number;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  encoding: string | null;
  line_count: number | null;
  is_binary: boolean;
  preview_kind: string | null;
  metadata_json: string | null;
}

export interface ResourceDetail {
  resource: Resource;
  locations: ResourceLocation[];
}

export interface AppErrorPayload {
  code: string;
  message: string;
}

export type CommandResult<T> = T;
