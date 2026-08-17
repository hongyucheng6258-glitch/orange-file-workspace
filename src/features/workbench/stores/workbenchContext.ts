/**
 * 工作台上下文 Store — 跨面板共享状态
 *
 * 持有当前工作目录（cwd）和当前目录资源 ID，
 * 供文件树面板写入、终端面板读取。
 */

import { create } from "zustand";

export interface WorkbenchContextState {
  /** 当前工作目录的物理路径 */
  cwd: string | null;
  /** 当前工作目录的资源 ID */
  cwdResourceId: string | null;
  /** 当前工作目录的显示名称 */
  cwdName: string | null;
  /** 设置当前工作目录 */
  setCwd: (path: string | null, resourceId: string | null, name?: string | null) => void;
}

export const useWorkbenchContext = create<WorkbenchContextState>((set) => ({
  cwd: null,
  cwdResourceId: null,
  cwdName: null,
  setCwd: (path, resourceId, name) =>
    set({ cwd: path, cwdResourceId: resourceId, cwdName: name }),
}));
