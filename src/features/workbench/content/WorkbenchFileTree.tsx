/**
 * Workbench 文件树渲染器 — 在面板中浏览受管文件库
 *
 * 功能：
 * - 懒加载子目录（list_children）
 * - 点击文件 → 在同面板或相邻面板打开编辑器
 * - 点击文件夹 → 设为当前工作目录
 * - 右键文件夹 → "在终端打开" 创建终端标签
 */

import { useState, useCallback, useEffect } from "react";
import {
  ChevronRight,
  ChevronDown,
  Folder,
  FolderOpen,
  File as FileIcon,
  Terminal as TerminalIcon,
} from "lucide-react";
import type { Resource } from "../../../lib/types";
import { call } from "../../../lib/tauri";
import { getResourcePath } from "../../../lib/openResource";
import { useLayoutStore } from "../stores/layoutStore";
import { useWorkbenchContext } from "../stores/workbenchContext";
import type { Tab } from "../lib/layoutModel";

interface Props {
  params: Record<string, unknown>;
  panelId: string;
}

/** 文件树节点 */
function TreeNode({
  resource,
  depth,
  panelId,
  onOpenFile,
  onSelectFolder,
}: {
  resource: Resource;
  depth: number;
  panelId: string;
  onOpenFile: (res: Resource) => void;
  onSelectFolder: (res: Resource) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<Resource[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [loading, setLoading] = useState(false);

  const isFolder = resource.kind === "folder";

  const toggle = useCallback(async () => {
    if (!isFolder) return;
    const next = !expanded;
    setExpanded(next);
    if (next && !loaded) {
      setLoading(true);
      try {
        const items = await call<Resource[]>("list_children", {
          parentId: resource.id,
        });
        setChildren(items);
        setLoaded(true);
      } catch {
        // ignore
      } finally {
        setLoading(false);
      }
    }
  }, [isFolder, expanded, loaded, resource.id]);

  const handleClick = useCallback(() => {
    if (isFolder) {
      onSelectFolder(resource);
      void toggle();
    } else {
      onOpenFile(resource);
    }
  }, [isFolder, onSelectFolder, onOpenFile, resource, toggle]);

  const handleTerminal = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      const path = await getResourcePath(resource.id);
      const store = useLayoutStore.getState();
      store.openTab(panelId, "terminal", "Terminal", {
        shell: "powershell",
        cwd: path ?? undefined,
      });
    },
    [resource.id, panelId],
  );

  const paddingLeft = 6 + depth * 14 + (isFolder ? 0 : 16);

  return (
    <div>
      <div
        className="wb-treenode-row"
        style={{ paddingLeft }}
        onClick={handleClick}
      >
        {isFolder && (
          <span className="wb-treenode-arrow">
            {loading ? "…" : expanded ? (
              <ChevronDown size={12} />
            ) : (
              <ChevronRight size={12} />
            )}
          </span>
        )}
        {isFolder ? (
          expanded ? (
            <FolderOpen size={14} color="var(--folder)" />
          ) : (
            <Folder size={14} color="var(--folder)" />
          )
        ) : (
          <FileIcon size={13} className="wb-treenode-file-icon" />
        )}
        <span className="wb-treenode-name">{resource.name}</span>
        {isFolder && (
          <button
            className="wb-treenode-action"
            onClick={handleTerminal}
            title="在终端打开"
          >
            <TerminalIcon size={13} />
          </button>
        )}
      </div>
      {expanded &&
        children.map((c) => (
          <TreeNode
            key={c.id}
            resource={c}
            depth={depth + 1}
            panelId={panelId}
            onOpenFile={onOpenFile}
            onSelectFolder={onSelectFolder}
          />
        ))}
    </div>
  );
}

export function WorkbenchFileTree({ params, panelId }: Props) {
  const [roots, setRoots] = useState<Resource[]>([]);
  const [loading, setLoading] = useState(true);
  const setCwd = useWorkbenchContext((s) => s.setCwd);
  const openTab = useLayoutStore((s) => s.openTab);
  const splitPanel = useLayoutStore((s) => s.splitPanel);

  // 加载根级资源
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    call<Resource[]>("list_children", { parentId: null })
      .then((items) => {
        if (!cancelled) setRoots(items);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // 点击文件 → 在同面板打开编辑器
  const handleOpenFile = useCallback(
    (res: Resource) => {
      openTab(panelId, "editor", res.name, { resourceId: res.id }, "file");
    },
    [openTab, panelId],
  );

  // 点击文件夹 → 设为当前工作目录
  const handleSelectFolder = useCallback(
    async (res: Resource) => {
      const path = await getResourcePath(res.id);
      setCwd(path, res.id, res.name);
    },
    [setCwd],
  );

  // 初始选中根目录
  useEffect(() => {
    if (roots.length > 0) {
      const firstFolder = roots.find((r) => r.kind === "folder");
      if (firstFolder) {
        void handleSelectFolder(firstFolder);
      }
    }
  }, [roots, handleSelectFolder]);

  if (loading) {
    return (
      <div className="wb-filetree">
        <div className="wb-filetree-loading">加载中…</div>
      </div>
    );
  }

  if (roots.length === 0) {
    return (
      <div className="wb-filetree">
        <div className="wb-filetree-empty">
          文件库为空
          <br />
          <span className="wb-filetree-empty-hint">
            请先在文件管理页导入文件
          </span>
        </div>
      </div>
    );
  }

  return (
    <div className="wb-filetree">
      <div className="wb-filetree-header">文件库</div>
      <div className="wb-filetree-body">
        {roots.map((r) => (
          <TreeNode
            key={r.id}
            resource={r}
            depth={0}
            panelId={panelId}
            onOpenFile={handleOpenFile}
            onSelectFolder={handleSelectFolder}
          />
        ))}
      </div>
    </div>
  );
}
