import { useCallback, useEffect, useState } from "react";
import { useLocation } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { File, FolderOpen, HardDrive, Link2, Upload, X } from "lucide-react";
import { call } from "../../../lib/tauri";
import { useFileStore } from "../../files/stores/fileStore";
import { useSettingsStore } from "../../settings/stores/settingsStore";
import { resolveImportParentId } from "../importTarget";
import { type TaskRecord, useTaskStore } from "../stores/taskStore";

type ImportMode = "managed" | "external";
export function ImportDropzone() {
  const [dragging, setDragging] = useState(false);
  const [pendingPaths, setPendingPaths] = useState<string[]>([]);
  const [showDialog, setShowDialog] = useState(false);
  const showPicker = useFileStore((s) => s.importPickerOpen);
  const setShowPicker = useFileStore((s) => s.setImportPickerOpen);
  const defaultMode = useSettingsStore((s) => s.settings?.general.default_import_mode ?? "managed");
  const [mode, setMode] = useState<ImportMode>(defaultMode);
  const [importing, setImporting] = useState(false);
  const [dragError, setDragError] = useState<string | null>(null);
  const [showDesktopHint, setShowDesktopHint] = useState(false);
  const parentId = useFileStore((s) => s.parentId);
  const addTask = useTaskStore((s) => s.addTask);
  const location = useLocation();
  const importParentId = resolveImportParentId(location.pathname, parentId);

  useEffect(() => {
    const onDragError = (event: Event) => {
      setDragError((event as CustomEvent<string>).detail || "文件拖出失败");
    };
    window.addEventListener("nexus:drag-error", onDragError);

    if (!("__TAURI_INTERNALS__" in window)) {
      return () => window.removeEventListener("nexus:drag-error", onDragError);
    }

    let disposed = false;
    let unlisten: (() => void) | undefined;

    getCurrentWebview().onDragDropEvent((event) => {
      if (disposed) return;
      if (event.payload.type === "enter" || event.payload.type === "over") {
        setDragging(true);
        return;
      }
      if (event.payload.type === "leave") {
        setDragging(false);
        return;
      }

      setDragging(false);
      const paths = event.payload.paths.filter(Boolean);
      if (paths.length === 0) {
        setDragError("系统没有返回拖入路径，请重新拖入文件或文件夹");
        return;
      }
      setDragError(null);
      setPendingPaths(paths);
      setMode(defaultMode);
      setShowDialog(true);
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    }).catch((error) => {
      if (!disposed) setDragError(`拖拽监听启动失败：${String(error)}`);
    });

    return () => {
      disposed = true;
      window.removeEventListener("nexus:drag-error", onDragError);
      unlisten?.();
    };
  }, []);

  // 打开系统选择框并进入导入模式弹窗。
  const pickAndImport = useCallback(async (directory: boolean) => {
    if (!("__TAURI_INTERNALS__" in window)) {
      setShowPicker(false);
      setShowDesktopHint(true);
      return;
    }

    try {
      const win = getCurrentWindow();
      await win.unminimize();
      await win.setFocus();
      const selected = await open({
        multiple: true,
        directory,
        title: directory ? "选择要导入的文件夹" : "选择要导入的文件",
      });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      setPendingPaths(paths);
      setShowPicker(false);
      setMode(defaultMode);
      setShowDialog(true);
    } catch (error) {
      setDragError(`无法打开系统选择框：${String(error)}`);
    }
  }, [setShowPicker]);

  const startImport = useCallback(async () => {
    if (pendingPaths.length === 0) return;
    setImporting(true);
    try {
      const task = await call<TaskRecord>("import_paths", {
        paths: pendingPaths,
        mode,
        parentId: importParentId,
      });
      addTask(task);
      setDragError(null);
      setShowDialog(false);
      setPendingPaths([]);
    } catch (error) {
      setDragError(`导入启动失败：${(error as Error).message}`);
    } finally {
      setImporting(false);
    }
  }, [pendingPaths, mode, importParentId, addTask]);

  return (
    <>
      {dragError && (
        <div className="drop-error" role="alert">
          <span>{dragError}</span>
          <button className="icon-btn" onClick={() => setDragError(null)} aria-label="关闭提示">
            <X size={15} />
          </button>
        </div>
      )}

      {dragging && (
        <div className="drop-overlay">
          <div className="drop-overlay-inner">
            <Upload size={36} />
            <span>松开以导入文件</span>
          </div>
        </div>
      )}

      {showPicker && (
        <div className="modal-mask" onClick={() => setShowPicker(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-head">
              <h3>导入</h3>
              <button className="icon-btn" onClick={() => setShowPicker(false)}>
                <X size={15} />
              </button>
            </div>
            <div className="import-modes">
              <button className="import-mode" onClick={() => pickAndImport(false)}>
                <File size={18} />
                <span className="import-mode-name">导入文件</span>
                <span className="import-mode-desc">选择一个或多个文件加入工作台</span>
              </button>
              <button className="import-mode" onClick={() => pickAndImport(true)}>
                <FolderOpen size={18} />
                <span className="import-mode-name">导入文件夹</span>
                <span className="import-mode-desc">递归导入文件夹内所有文件</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {showDesktopHint && (
        <div className="modal-mask" onClick={() => setShowDesktopHint(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-head">
              <h3>请在桌面应用中使用</h3>
              <button className="icon-btn" onClick={() => setShowDesktopHint(false)}>
                <X size={15} />
              </button>
            </div>
            <p className="modal-body-text">
              导入功能只能在 Orange 桌面窗口中运行。请关闭浏览器页面，切换到任务栏里的 Orange 窗口重试。
            </p>
            <div className="modal-actions">
              <button className="btn btn-primary" onClick={() => setShowDesktopHint(false)}>
                知道了
              </button>
            </div>
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
                <span className="import-mode-desc">文件复制到 Orange 管理目录，稳定可靠</span>
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
