import { useEffect } from "react";
import { Outlet } from "react-router-dom";
import { Sidebar } from "../components/Sidebar";
import { Topbar } from "../components/Topbar";
import { TaskBar } from "../features/tasks/components/TaskCenter";
import { ImportDropzone } from "../features/tasks/components/ImportDropzone";
import { useTaskStore } from "../features/tasks/stores/taskStore";

export function AppShell() {
  const startListening = useTaskStore((s) => s.startListening);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    startListening().then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, [startListening]);

  return (
    <div className="app-shell">
      <Sidebar />
      <div className="app-main">
        <Topbar />
        <div className="app-content">
          <Outlet />
        </div>
        <TaskBar />
      </div>
      {/* 全局拖拽导入：任意页面均可拖入文件/文件夹 */}
      <ImportDropzone />
    </div>
  );
}
