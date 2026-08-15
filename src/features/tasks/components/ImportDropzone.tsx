import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { HardDrive, Link2, Upload, X } from "lucide-react";
import { call } from "../../../lib/tauri";
import { useFileStore } from "../../files/stores/fileStore";

type ImportMode = "managed" | "external";

export function ImportDropzone() {
  const [dragging, setDragging] = useState(false);
  const [pendingPaths, setPendingPaths] = useState<string[]>([]);
  const [showDialog, setShowDialog] = useState(false);
  const [mode, setMode] = useState<ImportMode>("managed");
  const [importing, setImporting] = useState(false);
  const parentId = useFileStore((s) => s.parentId);

  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;

    (async () => {
      unlisten = await win.onDragDropEvent((event) => {
        const t = event.payload.type;
        if (t === "over") {
          setDragging(true);
        } else if (t === "drop") {
          setDragging(false);
          setPendingPaths(event.payload.paths);
          setShowDialog(true);
        } else if (t === "leave") {
          setDragging(false);
        }
      });
    })();

    return () => {
      unlisten?.();
    };
  }, []);

  const startImport = useCallback(async () => {
    if (pendingPaths.length === 0) return;
    setImporting(true);
    try {
      await call("import_paths", {
        paths: pendingPaths,
        mode,
        parentId: parentId ?? null,
      });
      setShowDialog(false);
      setPendingPaths([]);
    } catch {
      // 导入错误由任务中心显示
    } finally {
      setImporting(false);
    }
  }, [pendingPaths, mode, parentId]);

  return (
    <>
      {dragging && (
        <div className="drop-overlay">
          <div className="drop-overlay-inner">
            <Upload size={36} />
            <span>松开以导入文件</span>
          </div>
        </div>
      )}

      {showDialog && (
        <div className="modal-mask" onClick={() => !importing && setShowDialog(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-head">
              <h3>导入 {pendingPaths.length} 个路径</h3>
              <button className="icon-btn" onClick={() => setShowDialog(false)}>
                <X size={15} />
              </button>
            </div>

            <div className="import-modes">
              <button
                className={`import-mode ${mode === "managed" ? "active" : ""}`}
                onClick={() => setMode("managed")}
              >
                <HardDrive size={18} />
                <span className="import-mode-name">复制到应用仓库</span>
                <span className="import-mode-desc">文件复制到 NexusFile 管理目录，稳定可靠</span>
              </button>
              <button
                className={`import-mode ${mode === "external" ? "active" : ""}`}
                onClick={() => setMode("external")}
              >
                <Link2 size={18} />
                <span className="import-mode-name">保留原位置</span>
                <span className="import-mode-desc">仅建立索引，文件保持在原目录</span>
              </button>
            </div>

            <div className="modal-actions">
              <button className="btn" disabled={importing} onClick={() => setShowDialog(false)}>
                取消
              </button>
              <button className="btn btn-primary" disabled={importing} onClick={startImport}>
                {importing ? "开始导入…" : "开始导入"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
