import { openPath } from "@tauri-apps/plugin-opener";
import { call } from "./tauri";
import type { ResourceDetail } from "./types";

/**
 * 获取资源首个可用物理路径；无可用路径时返回 null。
 * 供“在终端打开”等入口复用（文件夹/项目根目录）。
 */
export async function getResourcePath(resourceId: string): Promise<string | null> {
  try {
    const detail = await call<ResourceDetail>("get_resource", { id: resourceId });
    const location = detail.locations.find((l) => l.is_available) ?? detail.locations[0];
    return location?.path ?? null;
  } catch {
    return null;
  }
}

/**
 * 用系统默认程序打开资源的物理路径。
 * 对可执行文件（exe/快捷方式）会直接启动应用，对其他文件交给系统关联程序。
 * 失败时弹出可见提示，避免用户误以为“点击无反应”。
 */
export async function openResourceExternally(resourceId: string): Promise<void> {
  try {
    const detail = await call<ResourceDetail>("get_resource", { id: resourceId });
    const location = detail.locations.find((l) => l.is_available) ?? detail.locations[0];
    if (!location?.path) {
      throw new Error("该资源没有可用的物理路径");
    }
    await openPath(location.path);
  } catch (e) {
    console.error("打开资源失败", e);
    window.alert(`打开失败：${(e as Error).message ?? String(e)}`);
  }
}
