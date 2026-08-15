import { Construction } from "lucide-react";

export function PlaceholderPage({ title }: { title: string }) {
  return (
    <div className="empty-state" style={{ height: "100%" }}>
      <div className="empty-icon">
        <Construction size={22} />
      </div>
      <span>{title}</span>
      <span style={{ fontSize: 12 }}>该模块将在后续阶段实现</span>
    </div>
  );
}
