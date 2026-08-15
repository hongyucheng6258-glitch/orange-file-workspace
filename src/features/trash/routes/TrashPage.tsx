import { useCallback, useEffect, useState } from "react";
import { RotateCcw, Trash2, Folder, FileText, Code2, AlertTriangle } from "lucide-react";
import type { Resource } from "../../../lib/types";
import { call, formatTime } from "../../../lib/tauri";

export function TrashPage() {
  const [items, setItems] = useState<Resource[]>([]);
  const [confirmId, setConfirmId] = useState<string | null>(null);

  const load = useCallback(async () => {
    const list = await call<Resource[]>("list_trash", {});
    setItems(list);
  }, []);

  useEffect(() => {
    load();
  }, []);

  const restore = async (id: string) => {
    await call("restore_resource", { id });
    await load();
  };

  const removeForever = async (id: string) => {
    await call<number>("delete_permanently", { ids: [id] });
    setConfirmId(null);
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
      <h2>回收站</h2>
      {items.length === 0 ? (
        <div className="empty-state">
          <span>回收站是空的</span>
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
              <span className="list-item-time">
                删除于 {formatTime(r.deleted_at)}
              </span>
              <div className="list-item-actions">
                <button className="btn btn-ghost" onClick={() => restore(r.id)}>
                  <RotateCcw size={13} /> 恢复
                </button>
                <button className="btn btn-danger" onClick={() => setConfirmId(r.id)}>
                  <Trash2 size={13} /> 永久删除
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {confirmId && (
        <div className="modal-mask" onClick={() => setConfirmId(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3 style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <AlertTriangle size={16} color="var(--danger)" /> 确认永久删除？
            </h3>
            <p style={{ margin: 0, fontSize: 13, color: "var(--text-secondary)" }}>
              该操作会同时删除磁盘上的文件，且无法恢复。
            </p>
            <div className="modal-actions">
              <button className="btn" onClick={() => setConfirmId(null)}>
                取消
              </button>
              <button
                className="btn btn-danger"
                onClick={() => confirmId && removeForever(confirmId)}
              >
                永久删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
