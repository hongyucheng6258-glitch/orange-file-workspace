/**
 * Workbench 路由页 — 初始化工作台并渲染
 *
 * 首次进入时创建左右分屏：左侧文件树，右侧欢迎页。
 */

import { useEffect, useRef } from "react";
import { Workbench } from "../components/Workbench";
import { useLayoutStore } from "../stores/layoutStore";
import type { Tab } from "../lib/layoutModel";
import "../contentRegistry"; // 注册内容渲染器（副作用导入）

export function WorkbenchPage() {
  const initLayout = useLayoutStore((s) => s.initLayout);
  const splitPanel = useLayoutStore((s) => s.splitPanel);
  const root = useLayoutStore((s) => s.root);
  const initedRef = useRef(false);

  useEffect(() => {
    if (!initedRef.current) {
      initedRef.current = true;
      // 初始化为文件树面板
      initLayout({
        id: "init_filetree",
        title: "文件树",
        icon: "filetree",
        contentType: "filetree",
        params: {},
      });
      // 分割为左右两栏：左文件树，右欢迎页
      const state = useLayoutStore.getState();
      if (state.root && state.root.type === "panel") {
        const welcomeTab: Tab = {
          id: "init_welcome",
          title: "欢迎",
          icon: "star",
          contentType: "welcome",
          params: {},
        };
        splitPanel(state.root.id, "row", welcomeTab);
      }
    }
  }, [initLayout, splitPanel]);

  if (!root) return null;

  return (
    <div className="workbench-page">
      <Workbench />
    </div>
  );
}
