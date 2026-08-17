/**
 * Workbench 路由页 — 初始化工作台并渲染
 *
 * 首次进入时创建一个欢迎标签，用户可通过工具栏的分割按钮添加更多面板。
 */

import { useEffect, useRef } from "react";
import { Workbench } from "../components/Workbench";
import { useLayoutStore } from "../stores/layoutStore";
import "../contentRegistry"; // 注册内容渲染器（副作用导入）

export function WorkbenchPage() {
  const initLayout = useLayoutStore((s) => s.initLayout);
  const root = useLayoutStore((s) => s.root);
  const initedRef = useRef(false);

  useEffect(() => {
    if (!initedRef.current) {
      initedRef.current = true;
      initLayout({
        id: "init_tab",
        title: "欢迎",
        icon: "star",
        contentType: "welcome",
        params: {},
      });
    }
  }, [initLayout]);

  if (!root) return null;

  return (
    <div className="workbench-page">
      <Workbench />
    </div>
  );
}
