/**
 * ProjectSessionManager - 工作区会话自动保存与恢复
 *
 * 职责：
 * 1. 项目打开时恢复上次会话（打开的文件 ID）
 * 2. 文件打开/关闭时自动保存会话
 * 3. 离开项目时更新最后访问时间
 *
 * 无渲染，纯逻辑组件。
 */

import { useEffect, useRef } from "react";
import { workspaceSessionsApi } from "../../../api/phase1";
import { useEditorStore } from "../stores/editorStore";

interface ProjectSessionManagerProps {
  projectId: string;
}

export function ProjectSessionManager({
  projectId,
}: ProjectSessionManagerProps) {
  const openFile = useEditorStore((s) => s.openFile);
  const open = useEditorStore((s) => s.open);

  const hasRestored = useRef(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 1. 项目打开时加载并恢复会话
  useEffect(() => {
    hasRestored.current = false;
    if (!projectId) return;

    let cancelled = false;
    (async () => {
      try {
        const session = await workspaceSessionsApi.get(projectId);
        if (cancelled || !session) {
          hasRestored.current = true;
          return;
        }

        // 恢复上次打开的文件
        if (session.active_file_id) {
          await open(session.active_file_id);
        }
      } catch {
        // 会话不存在或加载失败时静默
      } finally {
        hasRestored.current = true;
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [projectId, open]);

  // 2. 文件打开/关闭时自动保存会话（带防抖）
  useEffect(() => {
    if (!hasRestored.current || !projectId) return;

    if (saveTimer.current) {
      clearTimeout(saveTimer.current);
    }

    saveTimer.current = setTimeout(() => {
      const fileId = openFile?.resource.id;
      const fileName = openFile?.resource.name;
      const filePath = openFile?.path;

      const tabsData = fileId
        ? JSON.stringify([{ id: fileId, name: fileName, path: filePath }])
        : undefined;

      workspaceSessionsApi
        .save(projectId, tabsData, fileId ?? undefined)
        .catch(() => {});
    }, 800);

    return () => {
      if (saveTimer.current) {
        clearTimeout(saveTimer.current);
      }
    };
  }, [projectId, openFile]);

  // 3. 组件卸载（项目切换）时更新最后访问时间
  useEffect(() => {
    return () => {
      if (projectId) {
        workspaceSessionsApi.markRestored(projectId).catch(() => {});
      }
    };
  }, [projectId]);

  return null;
}
