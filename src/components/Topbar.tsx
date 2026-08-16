import { useEffect, useRef, useState } from "react";
import {
  Search,
  ChevronDown,
  SlidersHorizontal,
  Folder,
  File as FileIcon,
  Image as ImageIcon,
  FileText,
  FileCode2,
  FileArchive,
  Film,
  Music,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { call, formatSize, formatTime } from "../lib/tauri";
import { FileIconThumb } from "./FileIconThumb";
import type { DashboardStats, RecentItem, ResourceKind } from "../lib/types";

function kindIcon(kind: ResourceKind, name: string) {
  if (kind === "folder") return <Folder size={15} color="var(--folder)" />;
  const n = name.toLowerCase();
  if (/\.(png|jpe?g|gif|webp|svg|bmp|ico)$/.test(n))
    return <ImageIcon size={15} color="var(--image)" />;
  if (/\.(mp4|webm|mov|avi|mkv)$/.test(n))
    return <Film size={15} color="var(--video)" />;
  if (/\.(mp3|wav|ogg|flac|m4a)$/.test(n))
    return <Music size={15} color="var(--audio)" />;
  if (/\.(zip|rar|7z|tar|gz)$/.test(n))
    return <FileArchive size={15} color="var(--archive)" />;
  if (/\.(rs|py|js|ts|tsx|jsx|go|java|c|h|cpp|cs|rb|php|json|xml|toml|yaml|sh|sql|md)$/.test(n))
    return <FileCode2 size={15} color="var(--code)" />;
  if (/\.(txt|md|log|docx|pdf|csv)$/.test(n))
    return <FileText size={15} color="var(--file)" />;
  return <FileIcon size={15} color="var(--file)" />;
}

export function Topbar() {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const [recent, setRecent] = useState<RecentItem[] | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (query.trim()) {
      navigate(`/search?q=${encodeURIComponent(query.trim())}`);
    }
  };

  // 下拉打开时加载最近使用，关闭时丢弃过期结果。
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    call<DashboardStats>("dashboard_stats")
      .then((stats) => {
        if (!cancelled) setRecent(stats.recent);
      })
      .catch(() => {
        if (!cancelled) setRecent([]);
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  // 点击外部或按 Esc 关闭下拉。
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const openResource = (item: RecentItem) => {
    setOpen(false);
    navigate("/files", { state: { openId: item.id } });
  };

  return (
    <header className="topbar">
      <form className="topbar-search" onSubmit={submit}>
        <Search size={15} className="search-icon" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索文件、应用、页面、项目…"
        />
        <button type="submit" className="search-kbd" title="搜索">
          <SlidersHorizontal size={14} />
        </button>
      </form>
      <div className="topbar-right">
        <div className="topbar-recent" ref={rootRef}>
          <button
            className={`btn btn-ghost recent-toggle ${open ? "active" : ""}`}
            onClick={() => setOpen((v) => !v)}
            aria-haspopup="menu"
            aria-expanded={open}
          >
            最近 <ChevronDown size={13} />
          </button>
          {open && (
            <div className="recent-menu" role="menu">
              <div className="recent-menu-head">
                <span>最近使用</span>
                <button
                  className="btn-link"
                  onClick={() => {
                    setOpen(false);
                    navigate("/files");
                  }}
                >
                  查看全部
                </button>
              </div>
              {recent === null ? (
                <div className="recent-menu-empty">加载中…</div>
              ) : recent.length === 0 ? (
                <div className="recent-menu-empty">暂无最近使用</div>
              ) : (
                <div className="recent-menu-list">
                  {recent.slice(0, 8).map((item) => (
                    <button
                      key={item.id}
                      className="recent-menu-item"
                      role="menuitem"
                      onClick={() => openResource(item)}
                      title={item.name}
                    >
                      <span className="recent-menu-icon">
                        {item.kind === "folder" ? (
                          kindIcon(item.kind, item.name)
                        ) : (
                          <FileIconThumb
                            resourceId={item.id}
                            size={15}
                            fallback={kindIcon(item.kind, item.name)}
                          />
                        )}
                      </span>
                      <span className="recent-menu-name">{item.name}</span>
                      <span className="recent-menu-meta">
                        {item.kind === "folder"
                          ? "文件夹"
                          : formatSize(item.file_size ?? null)}
                      </span>
                      <span className="recent-menu-time">
                        {formatTime(item.updated_at)}
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </header>
  );
}
