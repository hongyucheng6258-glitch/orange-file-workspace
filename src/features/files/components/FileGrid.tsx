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

interface FileGridProps {
  onOpen: (r: Resource) => void;
  onSelect: (r: Resource | null) => void;
}

function gridThumb(r: Resource) {
  if (r.kind === "folder") return <Folder size={28} color="var(--folder)" />;
  const name = r.name.toLowerCase();
  if (/\.(png|jpe?g|gif|webp|svg|bmp)$/.test(name))
    return <ImageIcon size={28} color="var(--image)" />;
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
    <div className="file-grid">
      {resources.map((r) => (
        <div
          key={r.id}
          className={`grid-card ${selection.has(r.id) ? "selected" : ""}`}
          onClick={(e) => onClick(r, e)}
          onDoubleClick={() => r.kind === "folder" && onOpen(r)}
        >
          <div className="grid-thumb">{gridThumb(r)}</div>
          <div className="grid-name" title={r.name}>
            {r.name}
          </div>
          <div className="grid-meta">
            {r.kind === "folder" ? "文件夹" : formatTime(r.updated_at)}
          </div>
        </div>
      ))}
    </div>
  );
}
