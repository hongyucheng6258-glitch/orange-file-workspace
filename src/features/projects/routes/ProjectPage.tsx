import { useCallback, useEffect, useState } from "react";
import { Code2, Folder, File as FileIcon, FolderOpen, ChevronRight, ChevronDown, Plus } from "lucide-react";
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
            <button
              key={p.id}
              className={`project-item ${current?.id === p.id ? "active" : ""}`}
              onClick={() => selectProject(p)}
            >
              <Code2 size={14} color="var(--primary)" />
              <span className="project-item-name">{p.name}</span>
            </button>
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
