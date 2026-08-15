import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  Folder,
  FileText,
  Code2,
  Star,
  Trash2,
  ListTodo,
  Settings,
  Boxes,
} from "lucide-react";

const NAV = [
  { to: "/", label: "首页", icon: LayoutDashboard, end: true },
  { to: "/files", label: "文件", icon: Folder },
  { to: "/pages", label: "页面", icon: FileText },
  { to: "/projects", label: "代码项目", icon: Code2 },
  { to: "/favorites", label: "收藏", icon: Star },
  { to: "/trash", label: "回收站", icon: Trash2 },
  { to: "/tasks", label: "任务中心", icon: ListTodo },
  { to: "/settings", label: "设置", icon: Settings },
];

export function Sidebar() {
  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <span className="brand-mark">
          <Boxes size={18} />
        </span>
        <span className="brand-name">NexusFile</span>
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
        <span className="storage-text">本地工作台 v0.1</span>
      </div>
    </aside>
  );
}
