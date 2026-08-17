/**
 * 内容注册表 — 为 PanelHost 注册默认内容渲染器
 *
 * 其他 feature 通过 registerContent() 注册自己的渲染器。
 */

import type { ReactNode } from "react";
import { registerContent } from "./components/PanelHost";
import type { Tab } from "./lib/layoutModel";
import { WorkbenchTerminal } from "./content/WorkbenchTerminal";
import { WorkbenchEditor } from "./content/WorkbenchEditor";

/** 注册欢迎页渲染器 */
registerContent("welcome", (_tab: Tab): ReactNode => {
  return (
    <div className="wb-welcome">
      <div className="wb-welcome-icon">★</div>
      <div className="wb-welcome-title">Orange Workbench</div>
      <div className="wb-welcome-hint">
        按 Ctrl+K 打开命令面板，或从侧栏导航开始
      </div>
    </div>
  );
});

/** 注册终端渲染器 */
registerContent("terminal", (tab: Tab): ReactNode => {
  return <WorkbenchTerminal params={tab.params} />;
});

/** 注册编辑器渲染器 */
registerContent("editor", (tab: Tab): ReactNode => {
  return <WorkbenchEditor params={tab.params} />;
});
