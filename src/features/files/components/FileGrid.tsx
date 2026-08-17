import { useRef, useState, useEffect } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  File as FileIcon,
  Folder,
  Image as ImageIcon,
  FileText,
  FileCode2,
  FileArchive,
  Film,
  Music,
} from "lucide-react";
import { useFileStore } from "../stores/fileStore";
import type { Resource } from "../../../lib/types";
import { formatTime } from "../../../lib/tauri";
import { Thumbnail } from "../../../components/Thumbnail";
import { FileIconThumb } from "../../../components/FileIconThumb";
import { startDragOut } from "../../../lib/dragOut";
import { openResourceExternally } from "../../../lib/openResource";

interface FileGridProps {
  onOpen: (r: Resource) => void;
  onSelect: (r: Resource | null) => void;
}

const GRID_GAP = 10;
const GRID_CARD_W = 160;
const GRID_CARD_H = 122;

function gridThumb(r: Resource) {
  if (r.kind === "folder") return <Folder size={28} color="var(--folder)" />;
  const name = r.name.toLowerCase();
  if (/\.(png|jpe?g|gif|webp|svg|bmp)$/.test(name))
    return <ImageIcon size={28} color="var(--image)" />;
  if (/\.(exe|lnk|msi|bat|cmd|url)$/.test(name))
    return (
      <FileIconThumb
        resourceId={r.id}
        size={28}
        fallback={<FileIcon size={28} color="var(--file)" />}
      />
    );
  if (/\.(mp4|webm|mov|avi)$/.test(name))
    return <Film size={28} color="var(--video)" />;
  if (/\.(mp3|wav|ogg|flac)$/.test(name))
    return <Music size={28} color="var(--audio)" />;
  if (/\.(zip|rar|7z|tar|gz)$/.test(name))
    return <FileArchive size={28} color="var(--archive)" />;
  if (/\.(rs|py|js|ts|tsx|jsx|go|java|c|h|cpp|cs|rb|php|json|xml|toml|yaml|sh|sql|md)$/.test(name))
    return <FileCode2 size={28} color="var(--code)" />;
  if (/\.(txt|md|log|docx|pdf|csv)$/.test(name))
    return <FileText size={28} color="var(--file)" />;
  return <FileIcon size={28} color="var(--file)" />;
}

export function FileGrid({ onOpen, onSelect }: FileGridProps) {
  const { resources, selection, toggleSelect, clearSelection } = useFileStore();
  const containerRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  // 用 ResizeObserver 跟踪容器宽度，换算列数实现二维虚拟化
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const measure = () => setWidth(el.clientWidth);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const columns = Math.max(1, Math.floor((width + GRID_GAP) / (GRID_CARD_W + GRID_GAP)));
  const cardW = columns > 0 ? Math.max(60, (width - (columns - 1) * GRID_GAP) / columns) : 0;
  const rowEstimate = GRID_CARD_H + GRID_GAP;

  const rowVirtualizer = useVirtualizer({
    count: Math.ceil(resources.length / columns),
    getScrollElement: () => containerRef.current,
    estimateSize: () => rowEstimate,
    overscan: 4,
  });
  const rowItems = rowVirtualizer.getVirtualItems();

  const onClick = (r: Resource, e: React.MouseEvent) => {
    if (e.ctrlKey || e.metaKey) {
      toggleSelect(r.id);
    } else {
      clearSelection();
      toggleSelect(r.id);
    }
    onSelect(r);
  };

  return (
    <div className="file-grid" ref={containerRef}>
      <div
        style={{
          position: "relative",
          height: `${rowVirtualizer.getTotalSize()}px`,
          width: "100%",
        }}
      >
        {rowItems.map((rvi) => {
          const startIdx = rvi.index * columns;
          const countInRow = Math.min(columns, resources.length - startIdx);
          const rowTop = rvi.start;
          return Array.from({ length: countInRow }, (_, i) => {
            const idx = startIdx + i;
            const r = resources[idx];
            return (
              <div
                key={r.id}
                draggable
                className={`grid-card ${selection.has(r.id) ? "selected" : ""}`}
                style={{
                  position: "absolute",
                  top: rowTop,
                  left: i * (cardW + GRID_GAP),
                  width: cardW,
                  height: GRID_CARD_H,
                }}
                onClick={(e) => onClick(r, e)}
                onDoubleClick={() => {
                  if (r.kind === "folder") onOpen(r);
                  else openResourceExternally(r.id);
                }}
                onDragStart={(e) => {
                  e.preventDefault();
                  startDragOut(selection.has(r.id) ? [...selection] : [r.id]);
                }}
              >
                <div className="grid-thumb">
                  {r.kind === "folder" ? (
                    <Folder size={28} color="var(--folder)" />
                  ) : /\.(png|jpe?g|gif|webp|bmp)$/i.test(r.name) ? (
                    <Thumbnail resourceId={r.id} name={r.name} size={52} />
                  ) : (
                    gridThumb(r)
                  )}
                </div>
                <div className="grid-name" title={r.name}>
                  {r.name}
                </div>
                <div className="grid-meta">
                  {r.kind === "folder" ? "文件夹" : formatTime(r.updated_at)}
                </div>
              </div>
            );
          });
        })}
      </div>
    </div>
  );
}
