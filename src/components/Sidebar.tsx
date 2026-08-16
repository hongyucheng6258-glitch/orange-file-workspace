import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  Folder,
  FileText,
  Code2,
  Star,
  Trash2,
  ListTodo,
  TerminalSquare,
  Settings,
  MonitorSmartphone,
  Activity,
} from "lucide-react";

const NAV = [
  { to: "/", label: "首页", icon: LayoutDashboard, end: true },
  { to: "/files", label: "文件", icon: Folder },
  { to: "/pages", label: "页面", icon: FileText },
  { to: "/projects", label: "代码项目", icon: Code2 },
  { to: "/favorites", label: "收藏", icon: Star },
  { to: "/trash", label: "回收站", icon: Trash2 },
  { to: "/tasks", label: "任务中心", icon: ListTodo },
  { to: "/runs", label: "运行中心", icon: Activity },
  { to: "/terminal", label: "终端", icon: TerminalSquare },
  { to: "/system", label: "电脑信息", icon: MonitorSmartphone },
  { to: "/settings", label: "设置", icon: Settings },
];

export function Sidebar() {
  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <span className="brand-mark">
          <img src="/icon.png" alt="" aria-hidden="true" />
        </span>
        <span className="brand-name">Orange</span>
      </div>
      <nav className="sidebar-nav">
        {NAV.map(({ to, label, icon: Icon, end }) => (
          <NavLink
            key={to}
            to={to}
            end={end}
            className={({ isActive }) => `nav-item ${isActive ? "active" : ""}`}
          >
            <Icon size={16} />
            <span>{label}</span>
          </NavLink>
        ))}
      </nav>
      <div className="sidebar-foot">
        <span className="storage-text">橙子的工作台</span>
      </div>
    </aside>
  );
}
