import { useCallback, useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Code2, Folder, File as FileIcon, FolderOpen, ChevronRight, ChevronDown, Plus, Trash2 } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { Resource } from "../../../lib/types";
import { call } from "../../../lib/tauri";
import { useEditorStore } from "../stores/editorStore";
import { CodeEditor } from "../components/CodeEditor";

/** 文件树节点：懒加载子目录。 */
function TreeNode({
  resource,
  depth,
  onOpenFile,
}: {
  resource: Resource;
  depth: number;
  onOpenFile: (id: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<Resource[]>([]);
  const [loaded, setLoaded] = useState(false);

  const isFolder = resource.kind === "folder";

  const toggle = useCallback(async () => {
    if (!isFolder) return;
    const next = !expanded;
    setExpanded(next);
    if (next && !loaded) {
      try {
        const items = await call<Resource[]>("list_project_files", {
          projectId: "",
          parentId: resource.id,
        });
        setChildren(items);
        setLoaded(true);
      } catch {
        // ignore
      }
    }
  }, [isFolder, expanded, loaded, resource.id]);

  if (isFolder) {
    return (
      <div>
        <button
          className="tree-node"
          style={{ paddingLeft: 6 + depth * 14 }}
          onClick={toggle}
        >
          {expanded ? (
            <ChevronDown size={12} className="tree-arrow" />
          ) : (
            <ChevronRight size={12} className="tree-arrow" />
          )}
          {expanded ? (
            <FolderOpen size={14} color="var(--folder)" />
          ) : (
            <Folder size={14} color="var(--folder)" />
          )}
          <span className="tree-name">{resource.name}</span>
        </button>
        {expanded &&
          children.map((c) => (
            <TreeNode key={c.id} resource={c} depth={depth + 1} onOpenFile={onOpenFile} />
          ))}
      </div>
    );
  }

  return (
    <button
      className="tree-node file"
      style={{ paddingLeft: 22 + depth * 14 }}
      onClick={() => onOpenFile(resource.id)}
    >
      <FileIcon size={13} className="tree-file-icon" />
      <span className="tree-name">{resource.name}</span>
    </button>
  );
}

export function ProjectPage() {
  const [projects, setProjects] = useState<Resource[]>([]);
  const [current, setCurrent] = useState<Resource | null>(null);
  const [tree, setTree] = useState<Resource[]>([]);
  const [importing, setImporting] = useState(false);
  const openFile = useEditorStore((s) => s.open);
  const location = useLocation();
  const navigate = useNavigate();

  const loadProjects = useCallback(async () => {
    const list = await call<Resource[]>("list_projects", {});
    setProjects(list);
  }, []);

  useEffect(() => {
    loadProjects();
  }, []);

  const selectProject = useCallback(async (p: Resource) => {
    setCurrent(p);
    const items = await call<Resource[]>("list_project_files", {
      projectId: p.id,
      parentId: null,
    });
    setTree(items);
  }, []);

  // 从收藏跳转打开指定项目。
  useEffect(() => {
    const openId = (location.state as { openId?: string } | null)?.openId;
    if (openId && projects.length > 0) {
      const p = projects.find((x) => x.id === openId);
      if (p) selectProject(p);
      navigate(location.pathname, { replace: true, state: null });
    }
  }, [projects, location.state, location.pathname, navigate, selectProject]);

  const importProject = useCallback(async () => {
    setImporting(true);
    try {
      const selected = await openDialog({ directory: true, title: "选择项目目录" });
      if (typeof selected === "string") {
        await call("import_project", { rootPath: selected, name: null });
        await loadProjects();
      }
    } finally {
      setImporting(false);
    }
  }, [loadProjects]);

  const deleteProject = useCallback(
    async (p: Resource) => {
      if (!window.confirm(`删除代码项目「${p.name}」？其下的文件记录将移入回收站，可在回收站恢复。`)) {
        return;
      }
      try {
        await call("delete_project", { projectId: p.id });
        if (current?.id === p.id) {
          setCurrent(null);
          setTree([]);
        }
        await loadProjects();
      } catch {
        // 忽略删除失败
      }
    },
    [current, loadProjects],
  );

  return (
    <div className="project-page">
      <aside className="project-list">
        <div className="project-list-head">
          <span>代码项目</span>
          <button className="icon-btn" onClick={importProject} disabled={importing} title="导入项目">
            <Plus size={15} />
          </button>
        </div>
        <div className="project-list-body">
          {projects.length === 0 && (
            <div className="page-tree-empty">暂无项目，点击 + 导入</div>
          )}
          {projects.map((p) => (
            <div
              key={p.id}
              className={`project-item-row ${current?.id === p.id ? "active" : ""}`}
            >
              <button className="project-item" onClick={() => selectProject(p)}>
                <Code2 size={14} color="var(--primary)" />
                <span className="project-item-name">{p.name}</span>
              </button>
              <button
                className="icon-btn project-item-delete"
                title="删除项目"
                onClick={() => deleteProject(p)}
              >
                <Trash2 size={13} />
              </button>
            </div>
          ))}
        </div>
      </aside>

      <aside className="project-tree">
        <div className="project-tree-head">
          <span>{current?.name ?? "文件"}</span>
        </div>
        <div className="project-tree-body">
          {tree.map((r) => (
            <TreeNode key={r.id} resource={r} depth={0} onOpenFile={openFile} />
          ))}
        </div>
      </aside>

      <main className="project-editor">
        <CodeEditor />
      </main>
    </div>
  );
}
