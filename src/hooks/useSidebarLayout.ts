import { useCallback, useEffect, useState } from "react";
import type { SidebarMode } from "../types/navigation";

const STORAGE_KEY = "sidebar-layout";
const DRAWER_BREAKPOINT = 960;
const COLLAPSE_BREAKPOINT = 1280;

interface SavedState {
  collapsedGroups: string[];
  preference: "expanded" | "collapsed" | null;
}

function loadState(): SavedState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return JSON.parse(raw);
  } catch {
    /* ignore */
  }
  return { collapsedGroups: [], preference: null };
}

function saveState(state: SavedState) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

export function useSidebarLayout() {
  const saved = loadState();
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(
    () => new Set(saved.collapsedGroups),
  );
  const [preference, setPreference] = useState<"expanded" | "collapsed" | null>(
    saved.preference,
  );
  const [windowWidth, setWindowWidth] = useState(window.innerWidth);
  const [isDrawerOpen, setIsDrawerOpen] = useState(false);

  useEffect(() => {
    const onResize = () => setWindowWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  // 计算当前模式：抽屉优先于宽度规则，宽度规则优先于手动偏好
  let mode: SidebarMode;
  if (windowWidth < DRAWER_BREAKPOINT) {
    mode = "drawer";
  } else if (windowWidth < COLLAPSE_BREAKPOINT && preference !== "expanded") {
    mode = "collapsed";
  } else if (preference === "collapsed" && windowWidth >= DRAWER_BREAKPOINT) {
    mode = "collapsed";
  } else {
    mode = "expanded";
  }

  const toggleGroup = useCallback(
    (groupId: string) => {
      setCollapsedGroups((prev) => {
        const next = new Set(prev);
        if (next.has(groupId)) next.delete(groupId);
        else next.add(groupId);
        return next;
      });
    },
    [],
  );

  const toggleCollapse = useCallback(() => {
    setPreference((prev) => (prev === "collapsed" ? "expanded" : "collapsed"));
  }, []);

  const openDrawer = useCallback(() => setIsDrawerOpen(true), []);
  const closeDrawer = useCallback(() => setIsDrawerOpen(false), []);

  // 抽屉打开时，按 Esc 关闭
  useEffect(() => {
    if (!isDrawerOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setIsDrawerOpen(false);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [isDrawerOpen]);

  // 持久化状态
  useEffect(() => {
    saveState({
      collapsedGroups: Array.from(collapsedGroups),
      preference,
    });
  }, [collapsedGroups, preference]);

  return {
    mode,
    isDrawerOpen,
    openDrawer,
    closeDrawer,
    collapsedGroups,
    toggleGroup,
    toggleCollapse,
    isGroupCollapsed: (id: string) => collapsedGroups.has(id),
  };
}