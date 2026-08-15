import { useEffect, useState } from "react";
import { FileText, Plus } from "lucide-react";
import { usePageStore } from "../stores/pageStore";
import { PageEditor } from "../components/PageEditor";

export function PagePage() {
  const { tree, loadTree, openPage, createPage, currentPageId } = usePageStore();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");

  useEffect(() => {
    loadTree();
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
              onClick={() => openPage(r.id)}
            >
              <FileText size={13} />
              <span className="page-tree-name">{r.name}</span>
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
    </div>
  );
}
