import { useEffect, useState } from "react";
import {
  File as FileIcon,
  Folder,
  Star,
  StarOff,
  HardDrive,
  Link as LinkIcon,
  FolderOpen,
  FileText,
  FileCode2,
  FileArchive,
  Image as ImageIcon,
  ExternalLink,
  Tag,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
import { fetchResourceDetail, toggleFavoriteResource } from "../features/files/stores/fileStore";
import type { ResourceDetail } from "../lib/types";
import { fileTypeName, formatSize, formatTime } from "../lib/tauri";
import { Thumbnail } from "./Thumbnail";
import { openResourceExternally } from "../lib/openResource";
import { FileIconThumb } from "./FileIconThumb";
import { TagManager } from "./TagManager";

/** 从托管副本路径中提取盘符用于友好显示（如 "C 盘"）。 */
function managedDirLabel(path: string): string {
  const m = /^([A-Za-z]):/.exec(path);
  return m ? `${m[1].toUpperCase()} 盘` : "C 盘";
}

function typeIcon(resourceId: string, kind: string, name: string) {
  if (kind === "folder") return <Folder size={36} color="var(--folder)" />;
  const n = name.toLowerCase();
  if (/\.(png|jpe?g|gif|webp|svg)$/.test(n))
    return <ImageIcon size={36} color="var(--image)" />;
  if (/\.(exe|lnk|msi|bat|cmd|url)$/.test(n))
    return (
      <FileIconThumb
        resourceId={resourceId}
        size={36}
        fallback={<FileIcon size={36} color="var(--file)" />}
      />
    );
  if (/\.(zip|rar|7z|tar|gz)$/.test(n))
    return <FileArchive size={36} color="var(--archive)" />;
  if (/\.(rs|py|js|ts|go|java|c|json|md|toml|yaml)$/.test(n))
    return <FileCode2 size={36} color="var(--code)" />;
  if (/\.(txt|pdf|docx|csv)$/.test(n))
    return <FileText size={36} color="var(--file)" />;
  return <FileIcon size={36} color="var(--file)" />;
}

export function DetailPanel({ resourceId }: { resourceId: string | null }) {
  const [detail, setDetail] = useState<ResourceDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [showTags, setShowTags] = useState(false);

  useEffect(() => {
    if (!resourceId) {
      setDetail(null);
      return;
    }
    setLoading(true);
    fetchResourceDetail(resourceId).then((d) => {
      setDetail(d);
      setLoading(false);
    });
  }, [resourceId]);

  const handleToggleFavorite = async () => {
    if (!detail) return;
    try {
      const isFavorite = await toggleFavoriteResource(detail.resource.id);
      setDetail({
        ...detail,
        resource: { ...detail.resource, is_favorite: isFavorite },
      });
    } catch {
      // 忽略收藏失败
    }
  };

  return (
    <aside className="detail-panel">
      {!resourceId ? (
        <div className="detail-empty">
          <span>选择一个文件查看详情</span>
        </div>
      ) : loading || !detail ? (
        <div className="detail-empty">
          <span>加载中…</span>
        </div>
      ) : (
        <div className="detail-body">
          <div className="detail-header">
            <span className="detail-title">详细信息</span>
            <div className="detail-header-actions">
              {detail.resource.kind !== "folder" && (
                <button
                  className="icon-btn"
                  title="用系统默认程序打开"
                  onClick={() => {
                    openResourceExternally(detail.resource.id);
                  }}
                >
                  <ExternalLink size={14} />
                </button>
              )}
              <button
                className="icon-btn"
                title={detail.resource.is_favorite ? "取消收藏" : "收藏"}
                onClick={handleToggleFavorite}
              >
                {detail.resource.is_favorite ? (
                  <Star size={14} color="var(--warning)" />
                ) : (
                  <StarOff size={14} />
                )}
              </button>
            </div>
          </div>

          <div className="detail-preview">
            {/\.(png|jpe?g|gif|webp|bmp)$/i.test(detail.resource.name) ? (
              <Thumbnail resourceId={detail.resource.id} name={detail.resource.name} size={120} />
            ) : (
              typeIcon(detail.resource.id, detail.resource.kind, detail.resource.name)
            )}
            <span className="detail-preview-name">{detail.resource.name}</span>
            <span className="detail-preview-type">
              {detail.resource.kind === "folder"
                ? "文件夹"
                : fileTypeName(detail.resource.name)}
            </span>
          </div>

          <div className="detail-section">
            <div className="detail-row">
              <span className="detail-label">大小</span>
              <span className="detail-value">
                {detail.resource.kind === "folder"
                  ? "-"
                  : formatSize(detail.locations[0]?.file_size ?? null)}
              </span>
            </div>
            <div className="detail-row">
              <span className="detail-label">修改时间</span>
              <span className="detail-value">
                {formatTime(detail.resource.updated_at)}
              </span>
            </div>
            <div className="detail-row">
              <span className="detail-label">创建时间</span>
              <span className="detail-value">
                {formatTime(detail.resource.created_at)}
              </span>
            </div>
          </div>

          <div className="detail-section">
            <div className="detail-section-title">位置</div>
            {detail.locations.map((loc) => (
              <div key={loc.id} className="detail-location">
                <span className="detail-loc-icon">
                  {loc.source_type === "managed" ? (
                    <HardDrive size={13} />
                  ) : (
                    <LinkIcon size={13} />
                  )}
                </span>
                <span className="detail-loc-path" title={loc.path}>
                  {loc.source_type === "managed"
                    ? `应用数据目录（${managedDirLabel(loc.path)}）中的托管副本`
                    : loc.path}
                </span>
                <span
                  className={`tag ${loc.is_available ? "" : "tag-warning"}`}
                >
                  {loc.source_type === "managed" ? "托管" : "引用"}
                  {!loc.is_available && " · 失效"}
                </span>
              </div>
            ))}
          </div>

          {/* Phase 1: 标签管理 */}
          <div className="detail-section">
            <button
              className="detail-section-toggle"
              onClick={() => setShowTags((v) => !v)}
            >
              {showTags ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
              <Tag size={13} />
              <span>标签</span>
            </button>
            {showTags && (
              <div className="detail-tag-manager">
                <TagManager resourceId={detail.resource.id} embedded />
              </div>
            )}
          </div>

          {detail.resource.kind === "folder" && (
            <div className="detail-section">
              <div className="detail-section-title">快捷操作</div>
              <button className="btn" style={{ width: "100%" }}>
                <FolderOpen size={14} /> 在资源管理器中显示
              </button>
            </div>
          )}
        </div>
      )}
    </aside>
  );
}
