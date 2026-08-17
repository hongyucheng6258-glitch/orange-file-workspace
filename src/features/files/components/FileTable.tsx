import { useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useNavigate } from "react-router-dom";
import {
  ChevronDown,
  ChevronRight,
  File as FileIcon,
  Folder,
  Image as ImageIcon,
  FileText,
  FileCode2,
  FileArchive,
  Film,
  Music,
  MoreHorizontal,
  Star,
  StarOff,
  Trash2,
  Pencil,
  FolderInput,
  TerminalSquare,
} from "lucide-react";
import { useFileStore } from "../stores/fileStore";
import type { Resource } from "../../../lib/types";
import { fileTypeName, formatSize, formatTime } from "../../../lib/tauri";
import { startDragOut } from "../../../lib/dragOut";
import { openResourceExternally, getResourcePath } from "../../../lib/openResource";
import { FileIconThumb } from "../../../components/FileIconThumb";

interface FileTableProps {
  onOpen: (r: Resource) => void;
  onSelect: (r: Resource | null) => void;
  onRename: (r: Resource) => void;
  onTrash: (rs: Resource[]) => void;
}

function kindIcon(r: Resource) {
  if (r.kind === "folder") return <Folder size={16} color="var(--folder)" />;
  const name = r.name.toLowerCase();
  if (/\.(png|jpe?g|gif|webp|svg|bmp|ico)$/.test(name))
    return <ImageIcon size={16} color="var(--image)" />;
  if (/\.(exe|lnk|msi|bat|cmd|url)$/.test(name))
    return (
      <FileIconThumb
        resourceId={r.id}
        size={16}
        fallback={<FileIcon size={16} color="var(--file)" />}
      />
    );
  if (/\.(mp4|webm|mov|avi|mkv)$/.test(name))
    return <Film size={16} color="var(--video)" />;
  if (/\.(mp3|wav|ogg|flac|m4a)$/.test(name))
    return <Music size={16} color="var(--audio)" />;
  if (/\.(zip|rar|7z|tar|gz)$/.test(name))
    return <FileArchive size={16} color="var(--archive)" />;
  if (/\.(rs|py|js|ts|tsx|jsx|go|java|c|h|cpp|cs|rb|php|json|xml|toml|yaml|yml|sh|sql|md)$/.test(name))
    return <FileCode2 size={16} color="var(--code)" />;
  if (/\.(txt|md|log|docx|pdf|csv)$/.test(name))
    return <FileText size={16} color="var(--file)" />;
  return <FileIcon size={16} color="var(--file)" />;
}

export const FILE_ROW_HEIGHT = 36;

export function FileTable({ onOpen, onSelect, onRename, onTrash }: FileTableProps) {
  const { resources, selection, toggleSelect, clearSelection, selectMany, sortKey, sortAsc, setSort, toggleFavorite } =
    useFileStore();
  const [menu, setMenu] = useState<{ x: number; y: number; resource: Resource } | null>(null);
  const navigate = useNavigate();
  const bodyRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: resources.length,
    getScrollElement: () => bodyRef.current,
    estimateSize: () => FILE_ROW_HEIGHT,
    overscan: 12,
  });
  const virtualItems = virtualizer.getVirtualItems();

  /** “在终端打开”：解析资源物理路径后跳转终端页（无可用路径则用默认目录）。 */
  const openTerminal = async (r: Resource) => {
    const path = await getResourcePath(r.id);
    navigate(path ? `/terminal?cwd=${encodeURIComponent(path)}` : "/terminal");
  };

  const onRowClick = (r: Resource, e: React.MouseEvent) => {
    if (e.shiftKey && selection.size > 0) {
      const idx = resources.findIndex((x) => x.id === r.id);
      const firstIdx = resources.findIndex((x) => selection.has(x.id));
      if (idx >= 0 && firstIdx >= 0) {
        const [lo, hi] = [Math.min(idx, firstIdx), Math.max(idx, firstIdx)];
        selectMany(resources.slice(lo, hi + 1).map((x) => x.id));
        return;
      }
    }
    if (e.ctrlKey || e.metaKey) {
      toggleSelect(r.id);
    } else {
      clearSelection();
      toggleSelect(r.id);
    }
    onSelect(r);
  };

  const onDoubleClick = (r: Resource) => {
    if (r.kind === "folder") onOpen(r);
    else openResourceExternally(r.id);
  };

  const onContext = (e: React.MouseEvent, r: Resource) => {
    e.preventDefault();
    if (!selection.has(r.id)) {
      clearSelection();
      toggleSelect(r.id);
      onSelect(r);
    }
    setMenu({ x: e.clientX, y: e.clientY, resource: r });
  };

  const sortHeader = (key: "name" | "kind" | "updated_at", label: string) => (
    <button
      className="th-btn"
      onClick={() => setSort(key)}
      title={`按${label}排序`}
    >
      {label}
      {sortKey === key &&
        (sortAsc ? (
          <ChevronDown size={12} strokeWidth={2.5} />
        ) : (
          <ChevronRight size={12} strokeWidth={2.5} className="sort-rotated" />
        ))}
    </button>
  );

  return (
    <div className="file-table-wrap">
      <div className="file-table" role="grid">
        <div className="file-table-head" role="row">
          <span className="col-name">{sortHeader("name", "名称")}</span>
          <span className="col-kind">{sortHeader("kind", "类型")}</span>
          <span className="col-size">大小</span>
          <span className="col-time">{sortHeader("updated_at", "修改时间")}</span>
          <span className="col-actions" />
        </div>
        <div className="file-table-body" role="rowgroup" ref={bodyRef}>
          <div
            style={{
              position: "relative",
              height: `${virtualizer.getTotalSize()}px`,
              width: "100%",
            }}
          >
            {virtualItems.map((vi) => {
              const r = resources[vi.index];
              const selected = selection.has(r.id);
              return (
                <div
                  key={r.id}
                  role="row"
                  draggable
                  className={`file-row ${selected ? "selected" : ""}`}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: FILE_ROW_HEIGHT,
                    transform: `translateY(${vi.start}px)`,
                  }}
                  onClick={(e) => onRowClick(r, e)}
                  onDoubleClick={() => onDoubleClick(r)}
                  onContextMenu={(e) => onContext(e, r)}
                  onDragStart={(e) => {
                    e.preventDefault();
                    startDragOut(selection.has(r.id) ? [...selection] : [r.id]);
                  }}
                >
                  <span className="col-name">
                    <span className="row-icon">{kindIcon(r)}</span>
                    <span className="row-name" title={r.name}>
                      {r.name}
                    </span>
                  </span>
                  <span className="col-kind">
                    {r.kind === "folder" ? "文件夹" : fileTypeName(r.name)}
                  </span>
                  <span className="col-size">
                    {r.kind === "folder" ? "-" : formatSize(undefined)}
                  </span>
                  <span className="col-time">{formatTime(r.updated_at)}</span>
                  <span className="col-actions">
                    {r.kind === "folder" && (
                      <button
                        className="icon-btn"
                        title="在终端打开"
                        onClick={(e) => {
                          e.stopPropagation();
                          void openTerminal(r);
                        }}
                      >
                        <TerminalSquare size={14} />
                      </button>
                    )}
                    <button
                      className="icon-btn"
                      title={r.is_favorite ? "取消收藏" : "收藏"}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleFavorite(r.id);
                      }}
                    >
                      {r.is_favorite ? (
                        <Star size={14} color="var(--warning)" />
                      ) : (
                        <StarOff size={14} />
                      )}
                    </button>
                    <button
                      className="icon-btn"
                      title="更多"
                      onClick={(e) => {
                        e.stopPropagation();
                        const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
                        setMenu({ x: rect.left - 160, y: rect.bottom + 4, resource: r });
                      }}
                    >
                      <MoreHorizontal size={15} />
                    </button>
                  </span>
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {menu && (
        <div
          className="context-menu"
          style={{ left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
          onContextMenu={(e) => e.preventDefault()}
        >
          <button
            className="menu-item"
            onClick={() => {
              if (menu.resource.kind === "folder") {
                onOpen(menu.resource);
              } else {
                openResourceExternally(menu.resource.id);
              }
              setMenu(null);
            }}
          >
            <FolderInput size={14} /> 打开
          </button>
          <button
            className="menu-item"
            onClick={() => {
              onRename(menu.resource);
              setMenu(null);
            }}
          >
            <Pencil size={14} /> 重命名
          </button>
          {menu.resource.kind === "folder" && (
            <button
              className="menu-item"
              onClick={() => {
                void openTerminal(menu.resource);
                setMenu(null);
              }}
            >
              <TerminalSquare size={14} /> 在终端打开
            </button>
          )}
          <div className="menu-sep" />
          <button
            className="menu-item danger"
            onClick={() => {
              onTrash([menu.resource]);
              setMenu(null);
            }}
          >
            <Trash2 size={14} /> 移入回收站
          </button>
        </div>
      )}
    </div>
  );
}
