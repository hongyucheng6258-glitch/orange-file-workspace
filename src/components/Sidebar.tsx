import { useEffect } from "react";
import { NavLink, useLocation } from "react-router-dom";
import {
  LayoutDashboard,
  Columns3,
  Folder,
  FileText,
  Code2,
  Star,
  Copy,
  Trash2,
  ListTodo,
  Activity,
  TerminalSquare,
  Settings,
  MonitorSmartphone,
  Search,
  Sparkles,
  ChevronDown,
  PanelLeftClose,
  PanelLeft,
  X,
} from "lucide-react";
import { useSavedSearchStore } from "../features/search/stores/savedSearchStore";
import type { NavGroup, SidebarMode } from "../types/navigation";

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

interface SidebarProps {
  mode: SidebarMode;
  isDrawerOpen: boolean;
  onCloseDrawer: () => void;
  onToggleCollapse: () => void;
  collapsedGroups: Set<string>;
  onToggleGroup: (groupId: string) => void;
  isGroupCollapsed: (id: string) => boolean;
}

export function Sidebar({
  mode,
  isDrawerOpen,
  onCloseDrawer,
  onToggleCollapse,
  onToggleGroup,
  isGroupCollapsed,
}: SidebarProps) {
  const location = useLocation();
  const pinned = useSavedSearchStore((s) => s.pinned);
  const loadPinned = useSavedSearchStore((s) => s.loadPinned);

  useEffect(() => {
    loadPinned();
  }, [loadPinned]);

  // 检查当前路径所在分组
  const activeGroupIds = NAV_GROUPS.filter((g) =>
    g.items.some((item) => {
      if (item.end) return location.pathname === item.to;
      return location.pathname.startsWith(item.to);
    }),
  ).map((g) => g.id);

  // 智能集合也检查是否有激活
  const activeCollection = pinned.some(
    (s) => location.pathname === `/collections/${s.id}`,
  );
  if (activeCollection) activeGroupIds.push("_collections");

  const isCollapsed = mode === "collapsed";
  const isDrawer = mode === "drawer";
  const showAsDrawer = isDrawer && isDrawerOpen;

  const renderNavItem = (item: { to: string; label: string; icon: React.ComponentType<{ size: number }>; end?: boolean }, isCollection = false) => (
    <NavLink
      key={item.to}
      to={item.to}
      end={item.end}
      className={({ isActive }) =>
        `nav-item ${isCollection ? "nav-item-collection" : ""} ${isActive ? "active" : ""}`
      }
      title={isCollapsed ? item.label : undefined}
      onClick={isDrawer ? onCloseDrawer : undefined}
    >
      <item.icon size={16} />
      <span className={`nav-item-label ${isCollapsed ? "nav-item-label-hidden" : ""}`}>
        {item.label}
      </span>
    </NavLink>
  );

  const renderGroup = (group: NavGroup) => {
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
            {group.items.map((item) => renderNavItem(item))}
          </div>
        )}
      </div>
    );
  };

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
          {NAV_GROUPS.map(renderGroup)}

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
                      <span
                        className="collection-dot"
                        style={s.color ? { backgroundColor: s.color } : undefined}
                      >
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