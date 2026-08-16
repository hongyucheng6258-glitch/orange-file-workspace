import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { FileText, Plus, Trash2 } from "lucide-react";
import { usePageStore } from "../stores/pageStore";
import { PageEditor } from "../components/PageEditor";
import { ConfirmDialog } from "../../../components/ConfirmDialog";

export function PagePage() {
  const { tree, loadTree, openPage, createPage, currentPageId, pendingTarget, resolveOpen, cancelOpen } =
    usePageStore();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [showUnsaved, setShowUnsaved] = useState(false);
  const location = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    loadTree();
  }, []);

  // 从收藏跳转打开指定页面。
  useEffect(() => {
    const openId = (location.state as { openId?: string } | null)?.openId;
    if (openId) {
      void openPage(openId).then((res) => {
        if (res === "confirm") setShowUnsaved(true);
      });
      navigate(location.pathname, { replace: true, state: null });
    }
    // 仅挂载时处理一次
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleCreate = async () => {
    if (!name.trim()) return;
    await createPage(name.trim(), null);
    setCreating(false);
    setName("");
  };

  return (
    <div className="page-page">
      <aside className="page-tree">
        <div className="page-tree-head">
          <span className="page-tree-title">页面</span>
          <button className="icon-btn" title="新建页面" onClick={() => setCreating(true)}>
            <Plus size={15} />
          </button>
        </div>
        <div className="page-tree-list">
          {tree.length === 0 && (
            <div className="page-tree-empty">暂无页面</div>
          )}
          {tree.map((r) => (
            <div
              key={r.id}
              className={`page-tree-item ${r.id === currentPageId ? "active" : ""}`}
              onClick={() => {
                setConfirmDeleteId(null);
                void openPage(r.id).then((res) => {
                  if (res === "confirm") setShowUnsaved(true);
                });
              }}
            >
              <FileText size={13} />
              <span className="page-tree-name">{r.name}</span>
              <button
                className={`page-tree-del ${confirmDeleteId === r.id ? "confirming" : ""}`}
                title={confirmDeleteId === r.id ? "再次点击确认删除" : "删除页面"}
                onClick={(e) => {
                  e.stopPropagation();
                  if (confirmDeleteId === r.id) {
                    setConfirmDeleteId(null);
                    usePageStore.getState().deletePage(r.id);
                  } else {
                    setConfirmDeleteId(r.id);
                    setTimeout(() => {
                      setConfirmDeleteId((cur) => (cur === r.id ? null : cur));
                    }, 3000);
                  }
                }}
              >
                <Trash2 size={13} />
              </button>
            </div>
          ))}
        </div>
      </aside>

      <main className="page-main">
        <PageEditor />
      </main>

      {creating && (
        <div className="modal-mask" onClick={() => setCreating(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>新建页面</h3>
            <input
              className="input"
              placeholder="页面名称"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleCreate()}
              autoFocus
            />
            <div className="modal-actions">
              <button className="btn" onClick={() => setCreating(false)}>
                取消
              </button>
              <button className="btn btn-primary" onClick={handleCreate}>
                创建
              </button>
            </div>
          </div>
        </div>
      )}

      {showUnsaved && pendingTarget && (
        <ConfirmDialog
          title="未保存的修改"
          message="当前页面有未保存的修改，切换前是否保存？"
          onSave={() => {
            void resolveOpen(true).then(() => setShowUnsaved(false));
          }}
          onDiscard={() => {
            void resolveOpen(false).then(() => setShowUnsaved(false));
          }}
          onCancel={() => {
            cancelOpen();
            setShowUnsaved(false);
          }}
        />
      )}
    </div>
  );
}
