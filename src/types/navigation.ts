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
  items: NavItem[];
}

export type SidebarMode = "expanded" | "collapsed" | "drawer";