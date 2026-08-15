import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useLocation, useNavigate } from "react-router-dom";
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
import { fetchResourceDetail } from "../stores/fileStore";
import { call } from "../../../lib/tauri";
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
    setImportPickerOpen,
  } = useFileStore();

  const [crumbs, setCrumbs] = useState<Resource[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<Resource | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [creating, setCreating] = useState(false);
  const [folderName, setFolderName] = useState("");
  const renameInputRef = useRef<HTMLInputElement>(null);

  const currentParentId = crumbs.length > 0 ? crumbs[crumbs.length - 1].id : null;

  // 从收藏/搜索结果跳转打开指定资源。
  const location = useLocation();
  const navigate = useNavigate();
  const openId = (location.state as { openId?: string } | null)?.openId;

  useEffect(() => {
    if (!openId) return;
    (async () => {
      try {
        const [ancestors, detail] = await Promise.all([
          call<Resource[]>("get_ancestors", { id: openId }),
          fetchResourceDetail(openId),
        ]);
        if (!detail) return;
        if (detail.resource.kind === "folder") {
          setCrumbs([...ancestors, detail.resource]);
          setSelectedId(null);
        } else {
          setCrumbs(ancestors);
          // 等 currentParentId 变更引起的列表加载完成后再选中，避免被清空
          setTimeout(() => setSelectedId(openId), 0);
        }
      } catch {
        // ignore
      } finally {
        navigate(location.pathname, { replace: true, state: null });
      }
    })();
  }, [openId]);

  useEffect(() => {
    loadChildren(currentParentId);
    setSelectedId(null);
  }, [currentParentId]);

  // 导入/后台任务完成后由 Rust 广播，自动刷新当前列表。
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ parent_id: string | null }>("resource-changed", (event) => {
      if (event.payload.parent_id === currentParentId) {
        loadChildren(currentParentId);
      }
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, [currentParentId, loadChildren]);

  // 双保险：导入任务结束（成功/失败）时刷新列表；导入进行中每 2 秒节流刷新，
  // 让正在写入的文件夹内容逐渐显示出来。
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let lastRefresh = 0;
    listen<{ status: string }>("task-progress", (e) => {
      const now = Date.now();
      if (
        e.payload.status === "completed" ||
        e.payload.status === "failed" ||
        e.payload.status === "cancelled" ||
        (e.payload.status === "running" && now - lastRefresh > 2000)
      ) {
        lastRefresh = now;
        loadChildren(currentParentId);
      }
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, [currentParentId, loadChildren]);

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
            <button
              className="btn btn-primary"
              onClick={() => setImportPickerOpen(true)}
            >
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
    </div>
  );
}
