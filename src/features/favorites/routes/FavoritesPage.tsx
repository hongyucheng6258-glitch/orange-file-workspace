import { useCallback, useEffect, useState } from "react";
import { StarOff, Folder, FileText, Code2 } from "lucide-react";
import type { Resource } from "../../../lib/types";
import { call, formatTime } from "../../../lib/tauri";

export function FavoritesPage() {
  const [items, setItems] = useState<Resource[]>([]);

  const load = useCallback(async () => {
    const list = await call<Resource[]>("list_favorites", {});
    setItems(list);
  }, []);

  useEffect(() => {
    load();
  }, []);

  const unfavorite = async (id: string) => {
    await call<boolean>("toggle_favorite", { id });
    await load();
  };

  const icon = (r: Resource) =>
    r.kind === "folder" ? (
      <Folder size={15} color="var(--folder)" />
    ) : r.kind === "page" ? (
      <FileText size={15} color="var(--primary)" />
    ) : r.kind === "project" ? (
      <Code2 size={15} color="var(--code)" />
    ) : (
      <FileText size={15} color="var(--file)" />
    );

  return (
    <div className="list-page">
      <h2>收藏</h2>
      {items.length === 0 ? (
        <div className="empty-state">
          <span>还没有收藏任何内容</span>
          <span style={{ fontSize: 12 }}>点击文件或页面上的星标即可收藏</span>
        </div>
      ) : (
        <div className="list-page-items">
          {items.map((r) => (
            <div key={r.id} className="list-page-item">
              {icon(r)}
              <span className="list-item-name">{r.name}</span>
              <span className="list-item-kind">
                {r.kind === "file"
                  ? "文件"
                  : r.kind === "folder"
                    ? "文件夹"
                    : r.kind === "page"
                      ? "页面"
                      : "项目"}
              </span>
              <span className="list-item-time">{formatTime(r.updated_at)}</span>
              <button className="icon-btn" title="取消收藏" onClick={() => unfavorite(r.id)}>
                <StarOff size={14} />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
