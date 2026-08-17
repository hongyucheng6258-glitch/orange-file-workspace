import { useEffect, useMemo, useCallback } from "react";
import { Outlet, useNavigate } from "react-router-dom";
import { Sidebar } from "../components/Sidebar";
import { Topbar } from "../components/Topbar";
import { TaskBar } from "../features/tasks/components/TaskCenter";
import { ImportDropzone } from "../features/tasks/components/ImportDropzone";
import { useTaskStore } from "../features/tasks/stores/taskStore";
import { CommandPalette } from "../components/CommandPalette";
import { useCommandPalette } from "../hooks";
import type { Command } from "../types/phase1";

export function AppShell() {
  const startListening = useTaskStore((s) => s.startListening);
  const navigate = useNavigate();
  const palette = useCommandPalette();

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    startListening().then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, [startListening]);

  const goTo = useCallback(
    (path: string) => {
      navigate(path);
    },
    [navigate],
  );

  // 全局命令列表：导航 + 操作
  const commands = useMemo<Command[]>(
    () => [
      {
        id: "nav-home",
        label: "前往首页",
        category: "导航",
        keywords: ["home", "dashboard", "首页"],
        shortcut: "/",
        action: () => goTo("/"),
      },
      {
        id: "nav-files",
        label: "前往文件",
        category: "导航",
        keywords: ["file", "folder", "文件"],
        action: () => goTo("/files"),
      },
      {
        id: "nav-pages",
        label: "前往页面",
        category: "导航",
        keywords: ["page", "note", "页面"],
        action: () => goTo("/pages"),
      },
      {
        id: "nav-projects",
        label: "前往代码项目",
        category: "导航",
        keywords: ["project", "code", "项目", "代码"],
        action: () => goTo("/projects"),
      },
      {
        id: "nav-favorites",
        label: "前往收藏",
        category: "导航",
        keywords: ["favorite", "star", "收藏"],
        action: () => goTo("/favorites"),
      },
      {
        id: "nav-trash",
        label: "前往回收站",
        category: "导航",
        keywords: ["trash", "recycle", "回收站"],
        action: () => goTo("/trash"),
      },
      {
        id: "nav-tasks",
        label: "前往任务中心",
        category: "导航",
        keywords: ["task", "任务"],
        action: () => goTo("/tasks"),
      },
      {
        id: "nav-runs",
        label: "前往运行中心",
        category: "导航",
        keywords: ["run", "运行"],
        action: () => goTo("/runs"),
      },
      {
        id: "nav-terminal",
        label: "前往终端",
        category: "导航",
        keywords: ["terminal", "终端"],
        action: () => goTo("/terminal"),
      },
      {
        id: "nav-search",
        label: "搜索文件",
        category: "操作",
        keywords: ["search", "搜索", "查找"],
        action: () => goTo("/search"),
      },
      {
        id: "nav-settings",
        label: "前往设置",
        category: "导航",
        keywords: ["setting", "config", "设置"],
        action: () => goTo("/settings"),
      },
      {
        id: "nav-system",
        label: "前往电脑信息",
        category: "导航",
        keywords: ["system", "hardware", "电脑", "系统"],
        action: () => goTo("/system"),
      },
    ],
    [goTo],
  );

  return (
    <div className="app-shell">
      <Sidebar />
      <div className="app-main">
        <Topbar onCommandPaletteToggle={palette.toggle} />
        <div className="app-content">
          <Outlet />
        </div>
        <TaskBar />
      </div>
      {/* 全局拖拽导入：任意页面均可拖入文件/文件夹 */}
      <ImportDropzone />
      {/* 全局命令面板：Ctrl+K 唤起 */}
      <CommandPalette
        isOpen={palette.isOpen}
        onClose={palette.close}
        commands={commands}
      />
    </div>
  );
}
