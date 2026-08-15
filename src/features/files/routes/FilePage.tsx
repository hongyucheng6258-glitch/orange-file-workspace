import { useEffect, useRef, useState } from "react";
import {
  ChevronRight,
  FolderPlus,
  LayoutGrid,
  List,
  RefreshCw,
  Upload,
  ChevronLeft,
  ChevronRight as ArrowRight,
} from "lucide-react";
import { useFileStore } from "../stores/fileStore";
import { FileTable } from "../components/FileTable";
import { FileGrid } from "../components/FileGrid";
import { DetailPanel } from "../../../components/DetailPanel";
import { ImportDropzone } from "../../tasks/components/ImportDropzone";
import type { Resource } from "../../../lib/types";

export function FilePage() {
  const {
    resources,
    loading,
    error,
    viewMode,
    setViewMode,
    loadChildren,
    createFolder,
    rename,
    trash,
    selection,
  } = useFileStore();

  const [crumbs, setCrumbs] = useState<Resource[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<Resource | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [creating, setCreating] = useState(false);
  const [folderName, setFolderName] = useState("");
  const renameInputRef = useRef<HTMLInputElement>(null);

  const currentParentId = crumbs.length > 0 ? crumbs[crumbs.length - 1].id : null;

  useEffect(() => {
    loadChildren(currentParentId);
    setSelectedId(null);
  }, [currentParentId]);

  const openFolder = (r: Resource) => {
    setCrumbs((prev) => [...prev, r]);
  };

  const navigateTo = (idx: number) => {
    setCrumbs((prev) => prev.slice(0, idx));
  };

  const goBack = () => {
    setCrumbs((prev) => prev.slice(0, -1));
  };

  const handleCreateFolder = async () => {
    if (!folderName.trim()) return;
    try {
      await createFolder(folderName.trim());
      setCreating(false);
      setFolderName("");
    } catch {
      // 错误由 store 显示
    }
  };

  const handleRename = async () => {
    if (!renaming || !renameValue.trim()) return;
    try {
      await rename(renaming.id, renameValue.trim());
      setRenaming(null);
    } catch {
      // ignore
    }
  };

  const handleTrash = async (rs: Resource[]) => {
    await trash(rs.map((r) => r.id));
  };

  return (
    <div className="file-page">
      <div className="file-main">
        {/* 工具栏 */}
        <div className="file-toolbar">
          <div className="nav-group">
            <button
              className="icon-btn"
              disabled={crumbs.length === 0}
              onClick={goBack}
              title="后退"
            >
              <ChevronLeft size={16} />
            </button>
            <button className="icon-btn" title="前进" disabled>
              <ArrowRight size={16} />
            </button>
          </div>

          <div className="breadcrumb">
            <button
              className={`crumb ${crumbs.length === 0 ? "active" : ""}`}
              onClick={() => navigateTo(0)}
            >
              我的文件
            </button>
            {crumbs.map((c, i) => (
              <span key={c.id} className="crumb-item">
                <ChevronRight size={13} className="crumb-sep" />
                <button
                  className={`crumb ${i === crumbs.length - 1 ? "active" : ""}`}
                  onClick={() => navigateTo(i + 1)}
                >
                  {c.name}
                </button>
              </span>
            ))}
          </div>

          <div className="toolbar-right">
            <button className="icon-btn" title="刷新" onClick={() => loadChildren(currentParentId)}>
              <RefreshCw size={15} />
            </button>
            <div className="view-toggle">
              <button
                className={`icon-btn ${viewMode === "list" ? "active" : ""}`}
                onClick={() => setViewMode("list")}
                title="列表视图"
              >
                <List size={15} />
              </button>
              <button
                className={`icon-btn ${viewMode === "grid" ? "active" : ""}`}
                onClick={() => setViewMode("grid")}
                title="网格视图"
              >
                <LayoutGrid size={15} />
              </button>
            </div>
            <div className="toolbar-sep" />
            <button className="btn" onClick={() => setCreating(true)}>
              <FolderPlus size={14} /> 新建文件夹
            </button>
            <button className="btn btn-primary">
              <Upload size={14} /> 导入
            </button>
          </div>
        </div>

        {/* 内容区 */}
        <div className="file-content">
          {loading ? (
            <div className="empty-state">
              <div className="empty-icon" />
              <span>加载中…</span>
            </div>
          ) : error ? (
            <div className="empty-state">
              <span style={{ color: "var(--danger)" }}>{error}</span>
            </div>
          ) : resources.length === 0 ? (
            <div className="empty-state">
              <div className="empty-icon">
                <FolderPlus size={22} />
              </div>
              <span>此文件夹为空</span>
              <span style={{ fontSize: 12 }}>
                拖入文件或点击「导入」添加内容
              </span>
            </div>
          ) : viewMode === "list" ? (
            <FileTable
              onOpen={openFolder}
              onSelect={(r) => setSelectedId(r ? r.id : null)}
              onRename={(r) => {
                setRenaming(r);
                setRenameValue(r.name);
                setTimeout(() => renameInputRef.current?.focus(), 0);
              }}
              onTrash={handleTrash}
            />
          ) : (
            <FileGrid
              onOpen={openFolder}
              onSelect={(r) => setSelectedId(r ? r.id : null)}
            />
          )}
        </div>

        {/* 重命名对话框 */}
        {renaming && (
          <div
            className="modal-mask"
            onClick={() => setRenaming(null)}
          >
            <div className="modal" onClick={(e) => e.stopPropagation()}>
              <h3>重命名</h3>
              <input
                ref={renameInputRef}
                className="input"
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleRename()}
                style={{ width: "100%" }}
              />
              <div className="modal-actions">
                <button className="btn" onClick={() => setRenaming(null)}>
                  取消
                </button>
                <button className="btn btn-primary" onClick={handleRename}>
                  确定
                </button>
              </div>
            </div>
          </div>
        )}

        {/* 新建文件夹对话框 */}
        {creating && (
          <div className="modal-mask" onClick={() => setCreating(false)}>
            <div className="modal" onClick={(e) => e.stopPropagation()}>
              <h3>新建文件夹</h3>
              <input
                className="input"
                value={folderName}
                placeholder="文件夹名称"
                onChange={(e) => setFolderName(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleCreateFolder()}
                style={{ width: "100%" }}
              />
              <div className="modal-actions">
                <button className="btn" onClick={() => setCreating(false)}>
                  取消
                </button>
                <button className="btn btn-primary" onClick={handleCreateFolder}>
                  创建
                </button>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* 右侧详情栏 */}
      <DetailPanel resourceId={selectedId ?? (selection.size === 1 ? [...selection][0] : null)} />
      <ImportDropzone />
    </div>
  );
}
