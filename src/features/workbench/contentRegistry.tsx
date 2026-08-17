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
import { WorkbenchFileTree } from "./content/WorkbenchFileTree";
import {
  ImagePreview,
  PdfPreview,
  CsvPreview,
  ArchivePreview,
  VideoPreview,
  AudioPreview,
} from "../preview/PreviewRenderers";

/** 注册欢迎页渲染器 */
registerContent("welcome", (_tab: Tab): ReactNode => {
  return (
    <div className="wb-welcome">
      <div className="wb-welcome-icon">★</div>
      <div className="wb-welcome-title">Orange Workbench</div>
      <div className="wb-welcome-hint">
        从左侧文件树选择文件，或按 Ctrl+K 打开命令面板
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

/** 注册文件树渲染器 */
registerContent("filetree", (tab: Tab, panelId: string): ReactNode => {
  return <WorkbenchFileTree params={tab.params} panelId={panelId} />;
});

/** 注册图片预览渲染器 */
registerContent("image", (tab: Tab): ReactNode => {
  return <ImagePreview tab={tab} />;
});

/** 注册 PDF 预览渲染器 */
registerContent("pdf", (tab: Tab): ReactNode => {
  return <PdfPreview tab={tab} />;
});

/** 注册 CSV 预览渲染器 */
registerContent("csv", (tab: Tab): ReactNode => {
  return <CsvPreview tab={tab} />;
});

/** 注册压缩包预览渲染器 */
registerContent("archive", (tab: Tab): ReactNode => {
  return <ArchivePreview tab={tab} />;
});

/** 注册视频预览渲染器 */
registerContent("video", (tab: Tab): ReactNode => {
  return <VideoPreview tab={tab} />;
});

/** 注册音频预览渲染器 */
registerContent("audio", (tab: Tab): ReactNode => {
  return <AudioPreview tab={tab} />;
});

/** 注册 Markdown 预览渲染器（暂时复用编辑器） */
registerContent("markdown", (tab: Tab): ReactNode => {
  return <WorkbenchEditor params={tab.params} />;
});
