# 可扩展侧边栏布局 — 实施计划

日期：2026-08-18
依据：`docs/superpowers/specs/2026-08-18-extensible-sidebar-layout-design.md`
环境：Windows 11、Tauri 2、React 19、TypeScript 5、Vite

## 文件结构

| 文件 | 操作 | 职责 |
|---|---|---|
| `src/types/navigation.ts` | 新建 | 导航分组与侧栏状态类型定义 |
| `src/hooks/useSidebarLayout.ts` | 新建 | 侧栏模式、展开折叠、分组状态、持久化 |
| `src/components/Sidebar.tsx` | 重写 | 按分组渲染、折叠、收起态、抽屉内导航 |
| `src/layouts/AppShell.tsx` | 修改 | 连接 useSidebarLayout，向 Sidebar/Topbar 传递状态 |
| `src/components/Topbar.tsx` | 修改 | 接收 `onMenuToggle` 回调，显示菜单按钮 |
| `src/styles/tokens.css` | 修改 | 新增 `--sidebar-w-collapsed` 和抽屉动画令牌 |
| `src/styles/app.css` | 修改 | 新增分组、收起态、抽屉、遮罩和响应式样式 |

## 任务分解

### M1 类型定义与状态 Hook

**Task 1.1** 新建 `src/types/navigation.ts`，定义导航分组类型：

```typescript
import type { LucideIcon } from "lucide-react";

export interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
  end?: boolean;
}

export interface NavGroup {
  id: string;
  label: string;
  icon?: LucideIcon;
  items: NavItem[];
}

export type SidebarMode = "expanded" | "collapsed" | "drawer";
```

**Task 1.2** 新建 `src/hooks/useSidebarLayout.ts`，管理侧栏状态：

```typescript
import { useCallback, useEffect, useState } from "react";

export type SidebarMode = "expanded" | "collapsed" | "drawer";

interface SidebarLayoutState {
  mode: SidebarMode;
  collapsedGroups: Set<string>;
  isDrawerOpen: boolean;
}

const STORAGE_KEY = "sidebar-layout";
const DRAWER_BREAKPOINT = 960;
const COLLAPSE_BREAKPOINT = 1280;

function loadState(): { collapsedGroups: string[]; preference: "expanded" | "collapsed" | null } {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
  return { collapsedGroups: [], preference: null };
}

function saveState(state: { collapsedGroups: string[]; preference: "expanded" | "collapsed" | null }) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

export function useSidebarLayout() {
  const saved = loadState();
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set(saved.collapsedGroups));
  const [preference, setPreference] = useState<"expanded" | "collapsed" | null>(saved.preference);
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

  const toggleGroup = useCallback((groupId: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      saveState({ collapsedGroups: Array.from(next), preference: preference });
      return next;
    });
  }, [preference]);

  const toggleCollapse = useCallback(() => {
    setPreference((prev) => {
      const next = prev === "collapsed" ? "expanded" : "collapsed";
      saveState({ collapsedGroups: Array.from(collapsedGroups), preference: next });
      return next;
    });
  }, [collapsedGroups]);

  const openDrawer = useCallback(() => setIsDrawerOpen(true), []);
  const closeDrawer = useCallback(() => setIsDrawerOpen(false), []);

  // 持久化 collapsedGroups 变化
  useEffect(() => {
    saveState({ collapsedGroups: Array.from(collapsedGroups), preference });
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
```

**Task 1.3** 确认 `npm run build` 通过。

### M2 导航分组数据

**Task 2.1** `src/components/Sidebar.tsx` 顶部定义分组导航数据，替换原有 `NAV` 常量：

```typescript
import { useLocation } from "react-router-dom";
import {
  LayoutDashboard, Columns3, Folder, FileText, Code2, Star, Copy,
  Trash2, ListTodo, Activity, TerminalSquare, Settings, MonitorSmartphone,
  Search, Sparkles, ChevronDown, PanelLeftClose, PanelLeft, X,
} from "lucide-react";
import type { NavGroup, NavItem } from "../types/navigation";

const NAV_GROUPS: NavGroup[] = [
  {
    id: "overview",
    label: "概览",
    items: [
      { to: "/", label: "首页", icon: LayoutDashboard, end: true },
      { to: "/workbench", label: "工作台", icon: Columns3 },
    ],
  },
  {
    id: "resources",
    label: "资源管理",
    items: [
      { to: "/files", label: "文件", icon: Folder },
      { to: "/pages", label: "页面", icon: FileText },
      { to: "/projects", label: "代码项目", icon: Code2 },
      { to: "/favorites", label: "收藏", icon: Star },
    ],
  },
  {
    id: "tools",
    label: "工具",
    items: [
      { to: "/duplicates", label: "重复检测", icon: Copy },
      { to: "/search", label: "搜索", icon: Search },
    ],
  },
  {
    id: "execution",
    label: "执行环境",
    items: [
      { to: "/tasks", label: "任务中心", icon: ListTodo },
      { to: "/runs", label: "运行中心", icon: Activity },
      { to: "/terminal", label: "终端", icon: TerminalSquare },
    ],
  },
  {
    id: "system",
    label: "系统",
    items: [
      { to: "/trash", label: "回收站", icon: Trash2 },
      { to: "/system", label: "电脑信息", icon: MonitorSmartphone },
      { to: "/settings", label: "设置", icon: Settings },
    ],
  },
];
```

**Task 2.2** 确认 `npm run build` 通过。

### M3 Sidebar 组件重写

**Task 3.1** 重写 `Sidebar.tsx`，接收 props 并渲染分组侧栏。新签名：

```typescript
interface SidebarProps {
  mode: SidebarMode;
  isDrawerOpen: boolean;
  onCloseDrawer: () => void;
  onToggleCollapse: () => void;
  collapsedGroups: Set<string>;
  onToggleGroup: (groupId: string) => void;
  isGroupCollapsed: (id: string) => boolean;
}
```

**Task 3.2** 实现分组渲染逻辑。用 `useLocation()` 获取当前路径，在渲染时检查每个组的 `items` 是否有匹配路径，匹配则自动展开该组。

```typescript
export function Sidebar({
  mode,
  isDrawerOpen,
  onCloseDrawer,
  onToggleCollapse,
  collapsedGroups,
  onToggleGroup,
  isGroupCollapsed,
}: SidebarProps) {
  const location = useLocation();
  const pinned = useSavedSearchStore((s) => s.pinned);
  const loadPinned = useSavedSearchStore((s) => s.loadPinned);

  useEffect(() => { loadPinned(); }, [loadPinned]);

  // 检查当前路径所在分组
  const activeGroupIds = NAV_GROUPS.filter((g) =>
    g.items.some((item) => {
      if (item.end) return location.pathname === item.to;
      return location.pathname.startsWith(item.to);
    })
  ).map((g) => g.id);

  const isCollapsed = mode === "collapsed";
  const isDrawer = mode === "drawer";
  const showAsDrawer = isDrawer && isDrawerOpen;

  return (
    <>
      {isDrawer && isDrawerOpen && (
        <div className="sidebar-shade" onClick={onCloseDrawer} />
      )}
      <aside
        className={`sidebar ${isCollapsed ? "sidebar-collapsed" : ""} ${showAsDrawer ? "sidebar-drawer-open" : ""} ${isDrawer ? "sidebar-drawer" : ""}`}
      >
        <div className="sidebar-brand">
          <span className="brand-mark">
            <img src="/icon.png" alt="" aria-hidden="true" />
          </span>
          <span className={`brand-name ${isCollapsed ? "brand-name-hidden" : ""}`}>
            Orange
          </span>
          {isCollapsed ? (
            <button className="sidebar-expand-btn" onClick={onToggleCollapse} aria-label="展开侧栏">
              <PanelLeft size={16} />
            </button>
          ) : (
            <button className="sidebar-collapse-btn" onClick={onToggleCollapse} aria-label="收起侧栏">
              <PanelLeftClose size={16} />
            </button>
          )}
          {isDrawer && (
            <button className="sidebar-drawer-close" onClick={onCloseDrawer} aria-label="关闭导航">
              <X size={16} />
            </button>
          )}
        </div>

        <nav className="sidebar-nav">
          {NAV_GROUPS.map((group) => {
            const shouldAutoExpand = activeGroupIds.includes(group.id);
            const groupCollapsed = shouldAutoExpand ? false : isGroupCollapsed(group.id);

            return (
              <div key={group.id} className="nav-group">
                <button
                  className="nav-group-title"
                  onClick={() => onToggleGroup(group.id)}
                  aria-expanded={!groupCollapsed}
                  aria-label={`${group.label} 分组`}
                >
                  <span className="nav-group-label">
                    {isCollapsed ? null : <span>{group.label}</span>}
                  </span>
                  {isCollapsed ? null : (
                    <ChevronDown
                      size={14}
                      className={`nav-group-chevron ${groupCollapsed ? "nav-group-chevron-collapsed" : ""}`}
                    />
                  )}
                </button>
                {groupCollapsed ? null : (
                  <div className="nav-group-items">
                    {group.items.map((item) => (
                      <NavLink
                        key={item.to}
                        to={item.to}
                        end={item.end}
                        className={({ isActive }) =>
                          `nav-item ${isActive ? "active" : ""}`
                        }
                        title={isCollapsed ? item.label : undefined}
                        onClick={isDrawer ? onCloseDrawer : undefined}
                      >
                        <item.icon size={16} />
                        <span className={`nav-item-label ${isCollapsed ? "nav-item-label-hidden" : ""}`}>
                          {item.label}
                        </span>
                      </NavLink>
                    ))}
                  </div>
                )}
              </div>
            );
          })}

          {/* 智能集合 */}
          {pinned.length > 0 && (
            <div className="nav-group">
              <button
                className="nav-group-title"
                onClick={() => onToggleGroup("_collections")}
                aria-expanded={!isGroupCollapsed("_collections")}
                aria-label="智能集合分组"
              >
                <span className="nav-group-label">
                  <Sparkles size={13} />
                  {isCollapsed ? null : <span>智能集合</span>}
                </span>
                {isCollapsed ? null : (
                  <ChevronDown
                    size={14}
                    className={`nav-group-chevron ${isGroupCollapsed("_collections") ? "nav-group-chevron-collapsed" : ""}`}
                  />
                )}
              </button>
              {isGroupCollapsed("_collections") ? null : (
                <div className="nav-group-items">
                  {(pinned.length > 8 ? pinned.slice(0, 8) : pinned).map((s) => (
                    <NavLink
                      key={s.id}
                      to={`/collections/${s.id}`}
                      className={({ isActive }) =>
                        `nav-item nav-item-collection ${isActive ? "active" : ""}`
                      }
                      title={isCollapsed ? s.name : undefined}
                      onClick={isDrawer ? onCloseDrawer : undefined}
                    >
                      <span className="collection-dot" style={s.color ? { backgroundColor: s.color } : undefined}>
                        {!s.color && <Sparkles size={13} />}
                      </span>
                      <span className={`nav-item-label ${isCollapsed ? "nav-item-label-hidden" : ""}`}>
                        {s.name}
                      </span>
                    </NavLink>
                  ))}
                  {pinned.length > 8 && (
                    <span className="nav-group-more">+{pinned.length - 8} 个集合</span>
                  )}
                </div>
              )}
            </div>
          )}
        </nav>

        <div className="sidebar-foot">
          <span className={`storage-text ${isCollapsed ? "storage-text-hidden" : ""}`}>
            橙子的工作台
          </span>
        </div>
      </aside>
    </>
  );
}
```

**Task 3.3** 确认 `npm run build` 通过。

### M4 AppShell 集成

**Task 4.1** `src/layouts/AppShell.tsx` 接入 `useSidebarLayout`，向子组件传递状态：

```typescript
import { useSidebarLayout } from "../hooks/useSidebarLayout";

export function AppShell() {
  // ... 现有代码 ...
  const sidebarLayout = useSidebarLayout();

  return (
    <div className="app-shell">
      <Sidebar
        mode={sidebarLayout.mode}
        isDrawerOpen={sidebarLayout.isDrawerOpen}
        onCloseDrawer={sidebarLayout.closeDrawer}
        onToggleCollapse={sidebarLayout.toggleCollapse}
        collapsedGroups={sidebarLayout.collapsedGroups}
        onToggleGroup={sidebarLayout.toggleGroup}
        isGroupCollapsed={sidebarLayout.isGroupCollapsed}
      />
      <div className="app-main">
        <Topbar
          onCommandPaletteToggle={palette.toggle}
          onMenuToggle={sidebarLayout.mode === "drawer" ? sidebarLayout.openDrawer : undefined}
        />
        {/* ... 其余不变 ... */}
      </div>
    </div>
  );
}
```

**Task 4.2** 确认 `npm run build` 通过。

### M5 Topbar 菜单按钮

**Task 5.1** `src/components/Topbar.tsx` 新增 `onMenuToggle` props，在抽屉模式下显示菜单按钮：

```typescript
interface TopbarProps {
  onCommandPaletteToggle?: () => void;
  onMenuToggle?: () => void;
}

export function Topbar({ onCommandPaletteToggle, onMenuToggle }: TopbarProps) {
  // ...
  return (
    <header className="topbar">
      {onMenuToggle && (
        <button
          className="btn btn-ghost sidebar-menu-btn"
          onClick={onMenuToggle}
          aria-label="打开导航菜单"
        >
          <span className="menu-icon">☰</span>
        </button>
      )}
      {/* ... 其余不变 ... */}
    </header>
  );
}
```

**Task 5.2** 确认 `npm run build` 通过。

### M6 CSS 样式

**Task 6.1** `src/styles/tokens.css` 新增令牌：

```css
--sidebar-w-collapsed: 56px;
--sidebar-drawer-z: 100;
```

**Task 6.2** 在 `app.css` 的 `.sidebar` 块后新增分组样式：

```css
/* ===== 导航分组 ===== */
.nav-group {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.nav-group + .nav-group {
  margin-top: 4px;
  padding-top: 4px;
  border-top: 1px solid var(--border);
}

.nav-group-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 6px 10px;
  border: none;
  border-radius: var(--radius-s);
  background: transparent;
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 550;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  cursor: pointer;
  transition: color var(--dur-fast) var(--ease);
}

.nav-group-title:hover {
  color: var(--text);
}

.nav-group-chevron {
  transition: transform var(--dur-fast) var(--ease);
}

.nav-group-chevron-collapsed {
  transform: rotate(-90deg);
}

.nav-group-items {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.nav-group-more {
  padding: 4px 10px;
  font-size: 11px;
  color: var(--text-tertiary);
}
```

**Task 6.3** 新增收起态样式：

```css
/* ===== 侧栏收起态 ===== */
.sidebar-collapsed {
  width: var(--sidebar-w-collapsed);
}

.sidebar-collapsed .sidebar-brand {
  padding: 12px 8px;
  justify-content: center;
}

.sidebar-collapsed .brand-name,
.sidebar-collapsed .brand-name-hidden,
.sidebar-collapsed .sidebar-foot .storage-text,
.sidebar-collapsed .storage-text-hidden {
  display: none;
}

.sidebar-collapsed .sidebar-nav {
  padding: 8px 4px;
}

.sidebar-collapsed .nav-group-title {
  justify-content: center;
  padding: 6px 4px;
  font-size: 0;
  text-transform: none;
  letter-spacing: 0;
}

.sidebar-collapsed .nav-group-title .nav-group-label {
  display: flex;
  justify-content: center;
}

.sidebar-collapsed .nav-item {
  justify-content: center;
  padding: 8px 4px;
}

.sidebar-collapsed .nav-item-label,
.sidebar-collapsed .nav-item-label-hidden {
  display: none;
}

.sidebar-collapsed .nav-group-items {
  position: fixed;
  /* 浮动菜单由 JS 动态控制，这里不做全局定位 */
}

.sidebar-collapse-btn,
.sidebar-expand-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 1px solid var(--border);
  border-radius: var(--radius-s);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.sidebar-collapse-btn:hover,
.sidebar-expand-btn:hover {
  background: var(--surface-hover);
  color: var(--text);
}
```

**Task 6.4** 新增抽屉与遮罩样式：

```css
/* ===== 侧栏抽屉模式 ===== */
.sidebar-drawer {
  position: fixed;
  inset: 0 auto 0 0;
  width: var(--sidebar-w);
  z-index: var(--sidebar-drawer-z);
  transform: translateX(-100%);
  transition: transform 200ms ease;
  box-shadow: none;
}

.sidebar-drawer-open {
  transform: translateX(0);
  box-shadow: 8px 0 32px rgba(0, 0, 0, 0.15);
}

.sidebar-shade {
  position: fixed;
  inset: 0;
  z-index: calc(var(--sidebar-drawer-z) - 1);
  background: rgba(0, 0, 0, 0.25);
  animation: shade-fade-in 160ms ease;
}

@keyframes shade-fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

.sidebar-drawer-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  margin-left: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius-s);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}

.sidebar-menu-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: 1px solid var(--border);
  border-radius: var(--radius-s);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  margin-right: 4px;
}

.sidebar-menu-btn .menu-icon {
  font-size: 16px;
  line-height: 1;
}
```

**Task 6.5** 新增响应式样式，与 `app.css` 末尾的 `@media (max-width: 900px)` 块合并：

```css
@media (max-width: 960px) {
  .sidebar:not(.sidebar-drawer-open) {
    /* 窄窗口默认侧栏不显示（drawer 模式由 JS 控制） */
    display: none;
  }
}
```

**Task 6.6** 确认 `npm run build` 通过。

### M7 验证

**Task 7.1** 启动应用，逐一验证：

1. 展开状态下，13 个入口按 5 个分组显示，分组可展开折叠。
2. 当前路由所在分组自动展开，其余分组按持久化状态显示。
3. 点击收起按钮，侧栏缩为 56px，只显示图标。
4. 收起态悬停图标显示 title 提示。
5. 调整窗口到 960px 以下，侧栏自动隐藏，顶栏出现菜单按钮。
6. 点击菜单按钮，抽屉从左侧滑出，遮罩覆盖内容区。
7. 点击遮罩或按 Esc，抽屉关闭。
8. 智能集合固定为独立分组，超过 8 个时显示 "+N 个集合"。
9. 刷新页面后，分组折叠状态和手动收起偏好保留。

## 验证命令

- `npm run build`：tsc + vite 打包通过，无类型错误。
- `npm run tauri dev`：启动后人工验证 9 项验收清单。

## 回退方案

如果分组折叠状态与路由自动展开产生冲突，统一规则为：当前路由所在分组强制展开，忽略该组的持久化折叠状态；其余分组按持久化状态显示。上述逻辑已在 `Task 3.2` 的 `shouldAutoExpand` 中实现。